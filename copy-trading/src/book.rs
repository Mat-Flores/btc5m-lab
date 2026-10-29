//! Carnet CLOB `GET /book?token_id=` (format vérifié sur réponse réelle, 2026-10-28) : prix « demandé », borne de prix
//! à ±`slip` du meilleur prix, et remplissage calculé en remontant ce carnet réel (paper).
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Book {
    /// Condition id du marché (pour la résolution).
    pub market: String,
    /// Bids du meilleur au pire, asks du meilleur au pire : (prix, parts).
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
    pub tick: f64,
    pub min_size: f64,
    pub neg_risk: bool,
    /// Horodatage du carnet donné par le CLOB (ms).
    pub ts_ms: u64,
}

fn num(v: &Value) -> Option<f64> { v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_f64()) }

pub fn parse(body: &[u8]) -> Result<Book, String> {
    let v: Value = serde_json::from_slice(body).map_err(|e| format!("book json: {e}"))?;
    let side = |k: &str| -> Result<Vec<(f64, f64)>, String> {
        v[k].as_array().ok_or(format!("book: pas de {k}"))?.iter().map(|l| Ok((num(&l["price"]).ok_or("prix")?, num(&l["size"]).ok_or("taille")?))).collect::<Result<_, &str>>().map_err(String::from)
    };
    // le CLOB range les niveaux du pire au meilleur ; on ne s'y fie pas, on trie
    let (mut bids, mut asks) = (side("bids")?, side("asks")?);
    bids.sort_by(|a, b| b.0.total_cmp(&a.0));
    asks.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(Book {
        market: v["market"].as_str().ok_or("book: pas de market")?.to_string(), bids, asks,
        tick: num(&v["tick_size"]).ok_or("book: pas de tick_size")?, min_size: num(&v["min_order_size"]).ok_or("book: pas de min_order_size")?,
        neg_risk: v["neg_risk"].as_bool().unwrap_or(false), ts_ms: v["timestamp"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
    })
}

fn ticks(x: f64, tick: f64) -> f64 { x / tick + 1e-9 }

/// Prix limite d'achat : meilleur ask + `slip`, ramené sur la grille vers le bas (jamais au-delà de la borne), < 1.
pub fn buy_limit(ask: f64, slip: f64, tick: f64) -> f64 { (ticks((ask * (1.0 + slip)).min(1.0 - tick), tick).floor() * tick * 1e6).round() / 1e6 }
/// Prix limite de vente : meilleur bid − `slip`, ramené sur la grille vers le haut, ≥ un tick.
pub fn sell_limit(bid: f64, slip: f64, tick: f64) -> f64 { ((ticks((bid * (1.0 - slip)).max(tick), tick) - 2e-9).ceil() * tick * 1e6).round() / 1e6 }

/// Achat de `usd` en remontant les asks jusqu'à `limit` : (parts, coût). Rien dans la borne → (0, 0).
pub fn walk_buy(asks: &[(f64, f64)], limit: f64, usd: f64) -> (f64, f64) {
    let (mut shares, mut cost) = (0.0, 0.0);
    for &(p, s) in asks.iter().take_while(|l| l.0 <= limit + 1e-9) {
        let take = s.min((usd - cost) / p);
        if take <= 0.0 { break; }
        shares += take; cost += take * p;
    }
    (shares, cost)
}

/// Vente de `shares` en descendant les bids jusqu'à `limit` : (parts vendues, produit).
pub fn walk_sell(bids: &[(f64, f64)], limit: f64, shares: f64) -> (f64, f64) {
    let (mut sold, mut got) = (0.0, 0.0);
    for &(p, s) in bids.iter().take_while(|l| l.0 >= limit - 1e-9) {
        let take = s.min(shares - sold);
        if take <= 0.0 { break; }
        sold += take; got += take * p;
    }
    (sold, got)
}

#[cfg(test)]
mod tests {
    use super::*;

    // réponse réelle (2026-10-28, raccourcie à 2 niveaux par côté), niveaux du pire au meilleur
    const REAL: &str = r#"{"market":"0x2789","asset_id":"1804","timestamp":"1793240248866","hash":"4e4a","bids":[{"price":"0.01","size":"219878.45"},{"price":"0.48","size":"100"}],"asks":[{"price":"0.99","size":"220429.46"},{"price":"0.50","size":"40"}],"min_order_size":"5","tick_size":"0.01","neg_risk":false,"last_trade_price":"0.490"}"#;

    #[test]
    fn real_book_reads_best_first() {
        let b = parse(REAL.as_bytes()).unwrap();
        assert_eq!((b.bids[0], b.asks[0]), ((0.48, 100.0), (0.50, 40.0)));
        assert_eq!((b.tick, b.min_size, b.neg_risk, b.ts_ms, b.market.as_str()), (0.01, 5.0, false, 1793240248866, "0x2789"));
        assert!(parse(b"{}").is_err());
    }

    #[test]
    fn limits_never_leave_the_band() {
        assert_eq!(buy_limit(0.50, 0.10, 0.01), 0.55);
        assert_eq!(buy_limit(0.31, 0.10, 0.01), 0.34); // 0.341 → 0.34
        assert_eq!(buy_limit(0.95, 0.10, 0.01), 0.99);
        assert_eq!(buy_limit(0.555, 0.10, 0.001), 0.61); // 0.6105 → 0.610
        assert_eq!(sell_limit(0.50, 0.10, 0.01), 0.45);
        assert_eq!(sell_limit(0.31, 0.10, 0.01), 0.28); // 0.279 → 0.28
        assert_eq!(sell_limit(0.01, 0.10, 0.01), 0.01);
    }

    #[test]
    fn fills_come_only_from_levels_inside_the_limit() {
        let asks = [(0.50, 4.0), (0.52, 10.0), (0.60, 100.0)];
        let (s, c) = walk_buy(&asks, 0.55, 5.0);
        assert!((c - 5.0).abs() < 1e-9 && (s - (4.0 + 3.0 / 0.52)).abs() < 1e-9);
        let (s, c) = walk_buy(&asks, 0.55, 100.0); // la borne arrête avant 0.60
        assert!((s - 14.0).abs() < 1e-9 && (c - 7.2).abs() < 1e-9);
        assert_eq!(walk_buy(&asks, 0.45, 5.0), (0.0, 0.0));
        let bids = [(0.48, 3.0), (0.40, 50.0)];
        assert_eq!(walk_sell(&bids, 0.43, 10.0), (3.0, 1.44));
        assert_eq!(walk_sell(&bids, 0.50, 10.0), (0.0, 0.0));
    }
}
