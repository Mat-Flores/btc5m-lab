//! Les trois comptes, tenus en parallèle sur les mêmes décisions (mêmes tailles) :
//! observé = aux prix du wallet suivi ; demandé = au meilleur prix du carnet à l'observation ; réel = fills (live) ou
//! fills sur le carnet réel (paper). La résolution d'un marché paie 1 par part gagnante, 0 sinon, dans les trois.
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub start: f64,
    pub cash: f64,
    pub pos: BTreeMap<String, f64>,
}

impl Account {
    pub fn new(cash: f64) -> Self {
        Account {
            start: cash,
            cash,
            pos: BTreeMap::new(),
        }
    }
    pub fn shares(&self, token: &str) -> f64 {
        self.pos.get(token).copied().unwrap_or(0.0)
    }
    pub fn buy(&mut self, token: &str, shares: f64, cost: f64) {
        if shares <= 0.0 {
            return;
        }
        self.cash -= cost;
        *self.pos.entry(token.into()).or_default() += shares;
    }
    pub fn sell(&mut self, token: &str, shares: f64, proceeds: f64) {
        if shares <= 0.0 || self.shares(token) <= 0.0 {
            return;
        }
        self.cash += proceeds;
        let left = self.shares(token) - shares;
        if left > 1e-9 {
            self.pos.insert(token.into(), left);
        } else {
            self.pos.remove(token);
        }
    }
    /// Marché résolu : `payout` par part (1 gagnant, 0 perdant).
    pub fn resolve(&mut self, token: &str, payout: f64) {
        if let Some(s) = self.pos.remove(token) {
            self.cash += s * payout;
        }
    }
    /// Valeur aux prix `marks` ; les positions sans prix observé ne sont pas valorisées et sont comptées à part.
    pub fn value(&self, marks: &BTreeMap<String, f64>) -> (f64, usize) {
        let mut unmarked = 0;
        let v = self
            .pos
            .iter()
            .map(|(t, s)| {
                marks.get(t).map_or_else(
                    || {
                        unmarked += 1;
                        0.0
                    },
                    |p| s * p,
                )
            })
            .sum::<f64>();
        (self.cash + v, unmarked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buys_sells_and_resolution_settle_cash() {
        let mut a = Account::new(20.0);
        a.buy("t", 10.0, 5.0);
        a.buy("u", 4.0, 2.0);
        assert_eq!((a.cash, a.shares("t")), (13.0, 10.0));
        a.sell("t", 2.5, 1.5);
        assert_eq!((a.cash, a.shares("t")), (14.5, 7.5));
        let marks = BTreeMap::from([("t".to_string(), 0.6)]);
        assert_eq!(a.value(&marks), (14.5 + 4.5, 1));
        a.resolve("t", 1.0);
        a.resolve("u", 0.0);
        assert_eq!((a.cash, a.pos.len()), (22.0, 0));
        a.sell("t", 1.0, 1.0); // plus rien à vendre : rien ne bouge
        assert_eq!((a.cash, a.shares("t")), (22.0, 0.0));
    }
}
