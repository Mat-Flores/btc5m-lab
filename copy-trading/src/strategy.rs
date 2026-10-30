//! Stratégies interchangeables, choisies dans la config (`strategy`) : exécution (poser à son prix ou prendre le
//! carnet), taille d'achat, taille de vente, annulation. Chaque famille est un trait ; en ajouter une = un `impl` + un nom
//! dans `build`.
use crate::sizing;

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct StrategyCfg {
    /// `maker` : je pose un ordre limite à son prix (il est teneur de marché) ; `taker` : je prends le carnet (FAK).
    #[serde(default = "maker")]
    pub exec: String,
    /// `cumulative` : mon engagement suit son cumul sur le token ; `first_fill` : une entrée, sur son 1er remplissage.
    #[serde(default = "cumulative")]
    pub buy: String,
    /// `lots` : sa fraction vendue, par lots ≥ minimum d'ordre ; `all` : toute ma position à son prix.
    #[serde(default = "lots")]
    pub sell: String,
    /// Annulation de mon ordre posé quand il n'a plus rempli à ce prix depuis ce délai (min).
    #[serde(default = "ttl")]
    pub cancel_after_min: u64,
    /// `proportional` : multiple de sa part engagée sur le token, et plafond par token (part de ma valeur).
    #[serde(default = "one")]
    pub leverage: f64,
    #[serde(default = "max_frac")]
    pub max_frac: f64,
}
fn one() -> f64 {
    1.0
}
fn max_frac() -> f64 {
    0.15
}
fn maker() -> String {
    "maker".into()
}
fn cumulative() -> String {
    "cumulative".into()
}
fn lots() -> String {
    "lots".into()
}
fn ttl() -> u64 {
    360
}
impl Default for StrategyCfg {
    fn default() -> Self {
        StrategyCfg {
            exec: maker(),
            buy: cumulative(),
            sell: lots(),
            cancel_after_min: ttl(),
            leverage: one(),
            max_frac: max_frac(),
        }
    }
}

/// Ce qu'on sait au moment d'un de ses achats sur un token.
pub struct BuyCtx<'a> {
    /// Son 1er achat vu sur ce token ($) et son cumul d'achats sur ce token depuis le début du run ($).
    pub his_first_usd: f64,
    pub his_cum_usd: f64,
    pub his_value: f64,
    pub my_value: f64,
    pub my_cash: f64,
    /// Ce que j'ai déjà engagé sur ce token : rempli + posé non rempli ($).
    pub committed_usd: f64,
    pub tiers: &'a [(f64, f64)],
    pub min_usd: f64,
    pub max_usd: f64,
}

pub trait BuySizer {
    /// $ à ajouter à mes achats sur ce token (0 : rien).
    fn add_usd(&self, c: &BuyCtx) -> f64;
}

/// Une seule entrée par token, dimensionnée sur son 1er remplissage (règle du 2026-10-28).
pub struct FirstFill;
/// Mon engagement total sur le token = palier de SON cumul sur le token × ma valeur ; je complète à chaque palier franchi.
pub struct Cumulative;

impl BuySizer for FirstFill {
    fn add_usd(&self, c: &BuyCtx) -> f64 {
        if c.committed_usd > 0.0 {
            return 0.0;
        }
        sizing::buy_usd(
            c.his_first_usd,
            c.his_value,
            c.my_value,
            c.my_cash,
            c.tiers,
            c.min_usd,
            c.max_usd,
        )
    }
}

impl BuySizer for Cumulative {
    fn add_usd(&self, c: &BuyCtx) -> f64 {
        if c.his_value <= 0.0 {
            return 0.0;
        }
        let m = sizing::stake_frac(c.his_cum_usd / c.his_value, c.tiers);
        if m <= 0.0 {
            return 0.0;
        }
        let target = (m * c.my_value).max(c.min_usd).min(c.max_usd);
        let add = ((target - c.committed_usd).min(c.my_cash) * 100.0 + 1e-9).floor() / 100.0;
        if add + 1e-9 < c.min_usd {
            0.0
        } else {
            add
        }
    }
}

/// Mon engagement sur le token = `leverage` × sa part engagée (son cumul / sa valeur) × ma valeur, plafonné à `max_frac`
/// de ma valeur. Sans relèvement au minimum : ses petits achats s'accumulent jusqu'à faire un ordre, donc plusieurs
/// entrées par token, et ses petites positions (perdantes sur 30 j, rejeu du 2026-10-30) restent ignorées.
pub struct Proportional {
    pub leverage: f64,
    pub max_frac: f64,
}

impl BuySizer for Proportional {
    fn add_usd(&self, c: &BuyCtx) -> f64 {
        if c.his_value <= 0.0 {
            return 0.0;
        }
        let target = (self.leverage * c.his_cum_usd / c.his_value * c.my_value)
            .min(self.max_frac * c.my_value);
        let add = ((target - c.committed_usd).min(c.my_cash).min(c.max_usd) * 100.0 + 1e-9)
            .floor()
            / 100.0;
        if add + 1e-9 < c.min_usd {
            0.0
        } else {
            add
        }
    }
}

/// Prix de ma vente : le sien, sauf s'il sort en gain (son prix ≥ son prix moyen) à un prix qui me ferait sortir en
/// perte : je pose alors à mon point mort, au tick supérieur. S'il sort en perte, ou si son prix moyen est inconnu, je
/// le suis.
pub fn exit_price(his_price: f64, his_avg: Option<f64>, my_avg: f64, tick: f64) -> f64 {
    match his_avg {
        Some(h) if his_price >= h && his_price < my_avg => {
            let be = ((my_avg / tick - 1e-9).ceil() * tick * 1e6).round() / 1e6;
            be.min(1.0 - tick)
        }
        _ => his_price,
    }
}

/// Ce qu'on sait au moment d'une de ses ventes sur un token que je détiens.
pub struct SellCtx {
    /// Part cumulée de sa position vendue depuis mon entrée (0..1).
    pub his_sold_frac: f64,
    /// Son prix de vente et mon prix moyen d'achat.
    pub his_price: f64,
    pub my_avg_cost: f64,
    /// Mes parts : détenues (posées en vente comprises), déjà vendues depuis l'entrée, posées en vente non remplies.
    pub held: f64,
    pub sold: f64,
    pub listed: f64,
    pub min_shares: f64,
}

pub trait SellSizer {
    /// Parts à ajouter à mes ventes posées (0 : rien).
    fn add_shares(&self, c: &SellCtx) -> f64;
}

/// Toute ma position à son prix.
pub struct AllAtHisPrice;
/// Sa fraction vendue, par lots vendables. Le minimum d'ordre ne bloque jamais : en gain (son prix ≥ mon prix moyen) je
/// pose au moins un lot à chaque vente qu'il pose, ce qui garde son écart de teneur de marché ; en perte je suis sa
/// fraction et je sors en entier dès qu'il a vendu la moitié. Un reste invendable (< minimum) part avec le dernier lot.
pub struct Lots;

fn cents(x: f64) -> f64 {
    (x * 100.0 + 1e-9).floor() / 100.0
}

impl SellSizer for AllAtHisPrice {
    fn add_shares(&self, c: &SellCtx) -> f64 {
        cents((c.held - c.listed).max(0.0))
    }
}

impl SellSizer for Lots {
    fn add_shares(&self, c: &SellCtx) -> f64 {
        let free = (c.held - c.listed).max(0.0);
        if free <= 0.0 {
            return 0.0;
        }
        let target = c.his_sold_frac * (c.held + c.sold) - c.sold - c.listed;
        let in_gain = c.his_price >= c.my_avg_cost;
        let want = if target >= c.min_shares {
            target
        } else if in_gain {
            c.min_shares
        } else if c.his_sold_frac >= 0.5 {
            free
        } else {
            return 0.0;
        };
        let want = want.min(free);
        // un reste sous le minimum ne pourrait plus jamais être vendu : il part avec ce lot
        cents(if free - want < c.min_shares {
            free
        } else {
            want
        })
    }
}

pub struct Strategy {
    pub maker: bool,
    pub buy: Box<dyn BuySizer>,
    pub sell: Box<dyn SellSizer>,
    pub cancel_after_ms: u64,
}

pub fn build(c: &StrategyCfg) -> Strategy {
    Strategy {
        maker: match c.exec.as_str() {
            "maker" => true,
            "taker" => false,
            x => panic!("config : strategy.exec inconnu : {x} (maker | taker)"),
        },
        buy: match c.buy.as_str() {
            "cumulative" => Box::new(Cumulative),
            "first_fill" => Box::new(FirstFill),
            "proportional" => Box::new(Proportional {
                leverage: c.leverage,
                max_frac: c.max_frac,
            }),
            x => panic!("config : strategy.buy inconnu : {x} (cumulative | first_fill | proportional)"),
        },
        sell: match c.sell.as_str() {
            "lots" => Box::new(Lots),
            "all" => Box::new(AllAtHisPrice),
            x => panic!("config : strategy.sell inconnu : {x} (lots | all)"),
        },
        cancel_after_ms: c.cancel_after_min * 60_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const E: [(f64, f64); 3] = [(0.00001, 0.03), (0.001, 0.05), (0.01, 0.10)];

    fn buy(first: f64, cum: f64, committed: f64) -> BuyCtx<'static> {
        BuyCtx {
            his_first_usd: first,
            his_cum_usd: cum,
            his_value: 5_000_000.0,
            my_value: 100.0,
            my_cash: 80.0,
            committed_usd: committed,
            tiers: &E,
            min_usd: 0.35,
            max_usd: 20.0,
        }
    }

    #[test]
    fn cumulative_tops_up_at_each_tier_and_first_fill_enters_once() {
        // 3 $ au 1er palier, rien de plus tant qu'il reste au même palier, +2 $ quand son cumul passe 0,1 % (5 000 $)
        assert_eq!(Cumulative.add_usd(&buy(60.0, 60.0, 0.0)), 3.0);
        assert_eq!(Cumulative.add_usd(&buy(60.0, 4_000.0, 3.0)), 0.0);
        assert_eq!(Cumulative.add_usd(&buy(60.0, 6_000.0, 3.0)), 2.0);
        assert_eq!(Cumulative.add_usd(&buy(60.0, 40.0, 0.0)), 0.0, "poussière");
        assert_eq!(FirstFill.add_usd(&buy(60.0, 60.0, 0.0)), 3.0);
        assert_eq!(FirstFill.add_usd(&buy(60.0, 6_000.0, 3.0)), 0.0);
    }

    #[test]
    fn proportional_accumulates_his_small_buys_and_is_capped() {
        let p = Proportional {
            leverage: 1.0,
            max_frac: 0.15,
        };
        let c = |cum: f64, committed: f64| BuyCtx {
            his_cum_usd: cum,
            my_value: 1000.0,
            my_cash: 800.0,
            min_usd: 2.5,
            max_usd: 150.0,
            ..buy(0.0, cum, committed)
        };
        // 5 M$ chez lui, 1 000 $ chez moi : 10 k$ → 2 $ (< minimum, on attend), 15 k$ → 3 $, puis le complément
        assert_eq!(p.add_usd(&c(10_000.0, 0.0)), 0.0);
        assert_eq!(p.add_usd(&c(15_000.0, 0.0)), 3.0);
        assert_eq!(p.add_usd(&c(50_000.0, 3.0)), 7.0);
        // 10 % de son portefeuille → 100 $ ; 50 % → plafond 15 % de ma valeur
        assert_eq!(p.add_usd(&c(500_000.0, 0.0)), 100.0);
        assert_eq!(p.add_usd(&c(2_500_000.0, 100.0)), 50.0);
    }

    #[test]
    fn exit_waits_for_my_break_even_only_when_he_exits_in_gain() {
        // lui : entré à 0,50, sort à 0,52 ; moi entré à 0,537 → je pose à 0,54
        assert_eq!(exit_price(0.52, Some(0.50), 0.537, 0.01), 0.54);
        // sa sortie me laisse en gain : son prix
        assert_eq!(exit_price(0.52, Some(0.50), 0.51, 0.01), 0.52);
        // il coupe en perte, ou son prix moyen est inconnu : je le suis
        assert_eq!(exit_price(0.40, Some(0.50), 0.537, 0.01), 0.40);
        assert_eq!(exit_price(0.52, None, 0.537, 0.01), 0.52);
        assert_eq!(exit_price(0.98, Some(0.90), 0.995, 0.01), 0.99);
    }

    fn sell(frac: f64, price: f64, held: f64, sold: f64, listed: f64) -> SellCtx {
        SellCtx {
            his_sold_frac: frac,
            his_price: price,
            my_avg_cost: 0.17,
            held,
            sold,
            listed,
            min_shares: 5.0,
        }
    }

    #[test]
    fn lots_never_get_stuck_under_the_minimum() {
        // en gain : au moins un lot, et tout si le reste serait invendable
        assert_eq!(Lots.add_shares(&sell(0.001, 0.18, 20.0, 0.0, 0.0)), 5.0);
        assert_eq!(Lots.add_shares(&sell(0.001, 0.18, 8.0, 0.0, 0.0)), 8.0);
        assert_eq!(Lots.add_shares(&sell(0.001, 0.18, 20.0, 0.0, 20.0)), 0.0);
        // en perte : sa fraction quand elle fait un lot, sinon on attend, et tout dès la moitié vendue
        assert_eq!(Lots.add_shares(&sell(0.01, 0.10, 20.0, 0.0, 0.0)), 0.0);
        assert_eq!(Lots.add_shares(&sell(0.30, 0.10, 20.0, 0.0, 0.0)), 6.0);
        assert_eq!(Lots.add_shares(&sell(0.60, 0.10, 8.0, 0.0, 0.0)), 8.0);
        assert_eq!(
            AllAtHisPrice.add_shares(&sell(0.0, 0.1, 7.5, 0.0, 2.5)),
            5.0
        );
    }

    #[test]
    fn unknown_names_are_refused() {
        let ok = build(&StrategyCfg::default());
        assert!(ok.maker && ok.cancel_after_ms == 21_600_000);
        let bad = StrategyCfg {
            buy: "x".into(),
            ..StrategyCfg::default()
        };
        assert!(std::panic::catch_unwind(|| build(&bad)).is_err());
    }
}
