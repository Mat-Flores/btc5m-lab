//! Taille de mise. Règle choisie par l'utilisateur (2026-10-28), à la place de celle de SPEC.md §3 : m = min(100 %, K × p),
//! p = mise du wallet suivi / valeur totale de SON portefeuille, m = part de MON portefeuille, plafonnée par mon cash.

/// Part de mon portefeuille à miser quand il mise la part `p` du sien.
pub fn stake_frac(p: f64, k: f64) -> f64 { if p > 0.0 { (k * p).min(1.0) } else { 0.0 } }

/// Ma mise en $ (au cent inférieur) : m × ma valeur, plafonnée par mon cash et par `max_usd`.
pub fn buy_usd(his_usd: f64, his_value: f64, my_value: f64, my_cash: f64, k: f64, max_usd: f64) -> f64 {
    if his_value <= 0.0 { return 0.0; }
    let usd = (stake_frac(his_usd / his_value, k) * my_value).min(my_cash).min(max_usd).max(0.0);
    (usd * 100.0 + 1e-9).floor() / 100.0
}

/// Vente : s'il vend X % de sa position, je vends X % de la mienne (au centième de part inférieur).
pub fn sell_shares(his_sold: f64, his_before: f64, mine: f64) -> f64 {
    if his_before <= 0.0 || mine <= 0.0 { return 0.0; }
    ((his_sold / his_before).min(1.0) * mine * 100.0 + 1e-9).floor() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stake_is_k_times_p_capped_at_all() {
        // (p, m) avec K = 100 : les points de la spec, plus deux mises réelles de ce wallet (~5,45 M$)
        for (p, m) in [(0.00001, 0.001), (0.0001, 0.01), (0.005, 0.5), (0.01, 1.0), (0.05, 1.0), (0.075, 1.0), (0.10, 1.0), (0.15, 1.0), (0.60, 1.0)] {
            assert!((stake_frac(p, 100.0) - m).abs() < 1e-12, "p={p}");
        }
        assert_eq!(stake_frac(0.0, 100.0), 0.0);
        assert_eq!(stake_frac(-0.1, 100.0), 0.0);
        assert_eq!(stake_frac(0.005, 25.0), 0.125);
    }

    #[test]
    fn my_stake_is_capped_by_cash_and_order_cap() {
        // il mise 54 500 $ sur 5,45 M$ (1 %) → 100 % de mes 20 $, mais seulement 12,34 $ de cash
        assert_eq!(buy_usd(54_500.0, 5_450_000.0, 20.0, 12.34, 100.0, 50.0), 12.34);
        assert_eq!(buy_usd(5_450.0, 5_450_000.0, 20.0, 20.0, 100.0, 50.0), 2.0);
        assert_eq!(buy_usd(54_500.0, 5_450_000.0, 20.0, 20.0, 100.0, 5.0), 5.0);
        assert_eq!(buy_usd(9.0, 0.0, 20.0, 20.0, 100.0, 50.0), 0.0);
    }

    #[test]
    fn sells_follow_his_fraction() {
        assert_eq!(sell_shares(50.0, 200.0, 10.0), 2.5);
        assert_eq!(sell_shares(300.0, 200.0, 10.0), 10.0);
        assert_eq!(sell_shares(50.0, 0.0, 10.0), 0.0);
        assert_eq!(sell_shares(50.0, 200.0, 0.0), 0.0);
    }
}
