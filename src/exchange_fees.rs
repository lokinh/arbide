// Per-exchange maker/taker fees (bps). Retail/default tier, spot. Update from official fee pages as needed.

const FEES: &[(&str, f64, f64)] = &[
    // (exchange_id, maker_bps, taker_bps)
    ("binance", 10.0, 10.0),   // 0.10% each
    ("bybit", 10.0, 10.0),     // 0.10% each (VIP 0)
    ("coinbase", 40.0, 60.0),  // 0.40% maker, 0.60% taker (entry tier)
    ("kraken", 23.0, 40.0),    // 0.23% maker, 0.40% taker (standard)
];

pub fn maker_bps(exchange: &str) -> f64 {
    FEES.iter()
        .find(|(name, _, _)| *name == exchange)
        .map(|(_, m, _)| *m)
        .unwrap_or(10.0)
}

pub fn taker_bps(exchange: &str) -> f64 {
    FEES.iter()
        .find(|(name, _, _)| *name == exchange)
        .map(|(_, _, t)| *t)
        .unwrap_or(10.0)
}

/// Total fee in bps for arb: buy on buy_exchange (taker) + sell on sell_exchange (taker).
pub fn arb_round_trip_bps(buy_exchange: &str, sell_exchange: &str) -> f64 {
    taker_bps(buy_exchange) + taker_bps(sell_exchange)
}
