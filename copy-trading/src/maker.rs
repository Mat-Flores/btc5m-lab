//! Ordres posés. Les SIENS ne sont pas publics : on les déduit de ses remplissages (un ordre = token, sens, prix) et on
//! en tient les statistiques (âge, cadence, cumul) pour la rétro-ingénierie de sa stratégie. Les MIENS sont posés à son
//! prix ; en paper, leur remplissage se calcule sur les trades publics observés après la pose, derrière la file déjà
//! présente à ce prix dans le carnet (prudent : les annulations devant moi ne me font pas avancer).

/// Mon ordre posé.
#[derive(Debug, Clone, PartialEq)]
pub struct Rest {
    pub id: u64,
    pub token: String,
    pub buy: bool,
    pub price: f64,
    /// Parts demandées, parts remplies, $ échangés.
    pub size: f64,
    pub filled: f64,
    pub usd: f64,
    /// Parts devant moi dans la file à ce prix (paper).
    pub ahead: f64,
    pub posted_ms: u64,
    /// Identifiant CLOB (live).
    pub live_id: Option<String>,
}

impl Rest {
    pub fn left(&self) -> f64 {
        (self.size - self.filled).max(0.0)
    }
    /// Un trade public : parts remplies pour moi. `taker_buy` : le preneur achetait (il vide les asks, donc mes ventes).
    pub fn on_trade(&mut self, price: f64, size: f64, taker_buy: bool) -> f64 {
        if self.buy == taker_buy || self.left() <= 0.0 {
            return 0.0;
        }
        const E: f64 = 1e-9;
        // au-delà de mon prix, mon niveau a forcément été vidé avant : je suis rempli (au plus la taille du trade)
        let through = if self.buy {
            price < self.price - E
        } else {
            price > self.price + E
        };
        let mut q = if through {
            size
        } else if (price - self.price).abs() <= E {
            let used = self.ahead.min(size);
            self.ahead -= used;
            size - used
        } else {
            return 0.0;
        };
        q = q.min(self.left());
        self.filled += q;
        self.usd += q * self.price;
        q
    }
}

/// Son ordre déduit : token, sens, prix ; statistiques cumulées de ses remplissages.
#[derive(Debug, Clone, PartialEq)]
pub struct HisOrder {
    pub first_ms: u64,
    pub last_ms: u64,
    pub fills: u32,
    pub usd: f64,
    pub shares: f64,
    /// Plus long écart observé entre deux remplissages (ms).
    pub max_gap_ms: u64,
}

impl HisOrder {
    pub fn new(at: u64) -> Self {
        HisOrder {
            first_ms: at,
            last_ms: at,
            fills: 0,
            usd: 0.0,
            shares: 0.0,
            max_gap_ms: 0,
        }
    }
    /// Un remplissage de plus ; rend l'écart depuis le précédent (ms).
    pub fn fill(&mut self, at: u64, usd: f64, shares: f64) -> u64 {
        let gap = if self.fills == 0 {
            0
        } else {
            at.saturating_sub(self.last_ms)
        };
        self.max_gap_ms = self.max_gap_ms.max(gap);
        self.last_ms = at;
        self.fills += 1;
        self.usd += usd;
        self.shares += shares;
        gap
    }
}

/// Clé d'un ordre : token, achat, prix en micro-unités.
pub type Key = (String, bool, i64);
pub fn key(token: &str, buy: bool, price: f64) -> Key {
    (token.to_string(), buy, (price * 1e6).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rest(buy: bool, ahead: f64) -> Rest {
        Rest {
            id: 1,
            token: "t".into(),
            buy,
            price: 0.07,
            size: 40.0,
            filled: 0.0,
            usd: 0.0,
            ahead,
            posted_ms: 0,
            live_id: None,
        }
    }

    #[test]
    fn the_queue_ahead_is_served_first() {
        let mut r = rest(true, 100.0);
        assert_eq!(
            r.on_trade(0.07, 60.0, true),
            0.0,
            "un preneur acheteur ne touche pas mes achats"
        );
        assert_eq!(r.on_trade(0.07, 60.0, false), 0.0);
        assert_eq!(
            r.on_trade(0.08, 60.0, false),
            0.0,
            "au-dessus de mon prix d'achat : pas moi"
        );
        assert!((r.on_trade(0.07, 60.0, false) - 20.0).abs() < 1e-9 && r.ahead == 0.0);
        assert!(
            (r.on_trade(0.06, 50.0, false) - 20.0).abs() < 1e-9,
            "niveau traversé : le reste"
        );
        assert!((r.usd - 40.0 * 0.07).abs() < 1e-9 && r.left() == 0.0);
        let mut s = rest(false, 0.0);
        assert!((s.on_trade(0.08, 10.0, true) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn his_order_keeps_cadence() {
        let mut h = HisOrder::new(1_000);
        assert_eq!(h.fill(1_000, 3.0, 40.0), 0);
        assert_eq!(h.fill(61_000, 7.0, 100.0), 60_000);
        assert_eq!((h.fills, h.usd, h.max_gap_ms), (2, 10.0, 60_000));
        assert_eq!(key("t", true, 0.07), ("t".into(), true, 70_000));
    }
}
