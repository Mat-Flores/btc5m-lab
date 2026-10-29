//! Taille de mise. Règle choisie par l'utilisateur (2026-10-28), à la place de celle de SPEC.md §3 : des paliers.
//! p = mise du wallet suivi / valeur totale de SON portefeuille ; m = part de MON portefeuille, celle du plus haut palier
//! atteint (`tiers` : [seuil de p, m], seuils croissants), relevée au minimum d'ordre et plafonnée par mon cash.

/// Part de mon portefeuille à miser quand il mise la part `p` du sien : 0 sous le premier seuil.
pub fn stake_frac(p: f64, tiers: &[(f64, f64)]) -> f64 {
    tiers
        .iter()
        .filter(|t| p >= t.0)
        .last()
        .map_or(0.0, |t| t.1)
}

/// Ma mise en $ (au cent) : m × ma valeur, relevée au minimum d'ordre `min_usd` si mon cash le permet, plafonnée par mon
/// cash et par `max_usd`. 0 : pas d'ordre.
pub fn buy_usd(
    his_usd: f64,
    his_value: f64,
    my_value: f64,
    my_cash: f64,
    tiers: &[(f64, f64)],
    min_usd: f64,
    max_usd: f64,
) -> f64 {
    if his_value <= 0.0 {
        return 0.0;
    }
    let m = stake_frac(his_usd / his_value, tiers);
    if m <= 0.0 {
        return 0.0;
    }
    let cap = my_cash.min(max_usd);
    let min = (min_usd * 100.0 - 1e-9).ceil() / 100.0;
    let usd = ((m * my_value).max(min).min(cap) * 100.0 + 1e-9).floor() / 100.0;
    if usd + 1e-9 < min {
        0.0
    } else {
        usd
    }
}

/// Vente : s'il vend X % de sa position, je vends X % de la mienne (au centième de part inférieur).
pub fn sell_shares(his_sold: f64, his_before: f64, mine: f64) -> f64 {
    if his_before <= 0.0 || mine <= 0.0 {
        return 0.0;
    }
    ((his_sold / his_before).min(1.0) * mine * 100.0 + 1e-9).floor() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Barème E (config.example.json).
    const E: [(f64, f64); 3] = [(0.00001, 0.03), (0.001, 0.05), (0.01, 0.10)];

    #[test]
    fn tiers_give_the_highest_reached() {
        // (p, m) : les points de la spec, plus les seuils du barème et la poussière
        for (p, m) in [
            (0.000005, 0.0),
            (0.00001, 0.03),
            (0.0005, 0.03),
            (0.001, 0.05),
            (0.005, 0.05),
            (0.01, 0.10),
            (0.05, 0.10),
            (0.075, 0.10),
            (0.10, 0.10),
            (0.15, 0.10),
            (0.60, 0.10),
        ] {
            assert_eq!(stake_frac(p, &E), m, "p={p}");
        }
        assert_eq!(stake_frac(0.0, &E), 0.0);
        assert_eq!(stake_frac(0.5, &[]), 0.0);
    }

    #[test]
    fn my_stake_is_raised_to_the_minimum_and_capped_by_cash() {
        // il mise 547 $ sur 5,47 M$ (0,01 %) → 3 % de 20 $ = 0,60 $, relevé au minimum de 5 parts à 0,50 = 2,50 $
        assert_eq!(buy_usd(547.0, 5_470_000.0, 20.0, 20.0, &E, 2.5, 20.0), 2.5);
        // il mise 547 000 $ (10 %) → 10 % de 20 $ = 2 $, au-dessus du minimum de 5 parts à 0,30 = 1,50 $
        assert_eq!(
            buy_usd(547_000.0, 5_470_000.0, 20.0, 20.0, &E, 1.5, 20.0),
            2.0
        );
        // cash insuffisant pour le minimum : pas d'ordre
        assert_eq!(
            buy_usd(547_000.0, 5_470_000.0, 20.0, 1.2, &E, 1.5, 20.0),
            0.0
        );
        // poussière (< 0,001 %) : pas d'ordre ; valeur du wallet inconnue : pas d'ordre
        assert_eq!(buy_usd(40.0, 5_470_000.0, 20.0, 20.0, &E, 0.5, 20.0), 0.0);
        assert_eq!(buy_usd(547_000.0, 0.0, 20.0, 20.0, &E, 0.5, 20.0), 0.0);
        // grand capital : la part pure, plafonnée par l'ordre maximal
        assert_eq!(
            buy_usd(547_000.0, 5_470_000.0, 1000.0, 1000.0, &E, 2.5, 50.0),
            50.0
        );
    }

    #[test]
    fn sells_follow_his_fraction() {
        assert_eq!(sell_shares(50.0, 200.0, 10.0), 2.5);
        assert_eq!(sell_shares(300.0, 200.0, 10.0), 10.0);
        assert_eq!(sell_shares(50.0, 0.0, 10.0), 0.0);
        assert_eq!(sell_shares(50.0, 200.0, 0.0), 0.0);
    }
}
