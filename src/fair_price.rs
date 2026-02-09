// Fair price and per-exchange snapshot model for market-making.
// Uses WS feed data to maintain ExchangeQuoteSnapshot per exchange and compute FairPriceSnapshot.

use crate::core::timestamp_ns;
use std::collections::HashMap;

/// Per-exchange best bid/ask snapshot with freshness and latency.
#[derive(Clone, Debug, Default)]
pub struct ExchangeQuoteSnapshot {
    pub bid: f64,
    pub ask: f64,
    pub mid: f64,
    pub last_ts_ns: u64,
    /// Estimated latency (receive time - exchange timestamp) in nanoseconds.
    pub latency_ns: u64,
}

impl ExchangeQuoteSnapshot {
    pub fn new(bid: f64, ask: f64, received_ns: u64, exchange_ts_ns: u64) -> Self {
        let mid = if bid > 0.0 && ask > 0.0 {
            (bid + ask) / 2.0
        } else {
            0.0
        };
        let latency_ns = received_ns.saturating_sub(exchange_ts_ns);
        Self {
            bid,
            ask,
            mid,
            last_ts_ns: received_ns,
            latency_ns,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.bid > 0.0 && self.ask > 0.0 && self.mid > 0.0
    }

    /// Staleness: seconds since last update (for weighting).
    pub fn age_sec(&self, now_ns: u64) -> f64 {
        (now_ns.saturating_sub(self.last_ts_ns)) as f64 / 1e9
    }
}

/// Aggregated fair price with optional volatility estimate.
#[derive(Clone, Debug, Default)]
pub struct FairPriceSnapshot {
    pub fair_mid: f64,
    pub volatility_bps: f64,
    pub computed_at_ns: u64,
}

/// Rolling window for fair mid history (for volatility).
const VOLATILITY_WINDOW: usize = 64;

/// Sanity range for BTC price (exclude corrupted feeds).
const MID_MIN: f64 = 1_000.0;
const MID_MAX: f64 = 500_000.0;

/// Maintains per-exchange snapshots and computes fair price with optional staleness/volatility.
pub struct FairPriceEngine {
    symbol: String,
    snapshots: HashMap<String, ExchangeQuoteSnapshot>,
    mid_history: [f64; VOLATILITY_WINDOW],
    mid_history_len: usize,
    mid_history_idx: usize,
    /// Max age in seconds to include an exchange in fair price (staleness threshold).
    pub max_stale_sec: f64,
}

impl FairPriceEngine {
    pub fn new(symbol: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            snapshots: HashMap::new(),
            mid_history: [0.0; VOLATILITY_WINDOW],
            mid_history_len: 0,
            mid_history_idx: 0,
            max_stale_sec: 5.0,
        }
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Update one exchange's snapshot from a bid or ask level (we need both for mid; caller sends both).
    pub fn update_bid(&mut self, exchange: &str, bid: f64, _qty: f64, ts_ns: u64) {
        let now = timestamp_ns();
        let snap = self.snapshots.entry(exchange.to_string()).or_default();
        snap.bid = bid;
        snap.last_ts_ns = now;
        snap.latency_ns = now.saturating_sub(ts_ns);
        if snap.ask > 0.0 {
            snap.mid = (snap.bid + snap.ask) / 2.0;
        }
    }

    pub fn update_ask(&mut self, exchange: &str, ask: f64, _qty: f64, ts_ns: u64) {
        let now = timestamp_ns();
        let snap = self.snapshots.entry(exchange.to_string()).or_default();
        snap.ask = ask;
        snap.last_ts_ns = now;
        snap.latency_ns = now.saturating_sub(ts_ns);
        if snap.bid > 0.0 {
            snap.mid = (snap.bid + snap.ask) / 2.0;
        }
    }

    /// Get snapshot for an exchange (e.g. primary).
    pub fn get_snapshot(&self, exchange: &str) -> Option<&ExchangeQuoteSnapshot> {
        self.snapshots.get(exchange)
    }

    /// Get primary snapshot if valid and mid in sane range, else first such snapshot (fallback).
    pub fn get_primary_or_any(&self, primary: &str) -> Option<(String, ExchangeQuoteSnapshot)> {
        let sane = |s: &ExchangeQuoteSnapshot| s.is_valid() && s.mid >= MID_MIN && s.mid <= MID_MAX;
        if let Some(s) = self.snapshots.get(primary) {
            if sane(s) {
                return Some((primary.to_string(), s.clone()));
            }
        }
        for (name, s) in &self.snapshots {
            if sane(s) {
                return Some((name.clone(), s.clone()));
            }
        }
        None
    }

    /// Compute weighted mid from all valid, non-stale snapshots. Lower weight for stale/high-latency.
    pub fn compute_fair_snapshot(&mut self) -> FairPriceSnapshot {
        let now = timestamp_ns();
        let mut total_weight = 0.0;
        let mut weighted_mid = 0.0;

        for snap in self.snapshots.values() {
            if !snap.is_valid() || snap.mid < MID_MIN || snap.mid > MID_MAX {
                continue;
            }
            let age_sec = snap.age_sec(now);
            if age_sec > self.max_stale_sec {
                continue;
            }
            let staleness = 1.0 / (1.0 + age_sec);
            let lat_penalty = 1.0 / (1.0 + (snap.latency_ns as f64 / 1e9));
            let w = staleness * lat_penalty;
            weighted_mid += snap.mid * w;
            total_weight += w;
        }

        let fair_mid = if total_weight > 0.0 {
            weighted_mid / total_weight
        } else {
            0.0
        };

        if fair_mid > 0.0 {
            self.push_mid(fair_mid);
        }

        let volatility_bps = self.volatility_bps();

        FairPriceSnapshot {
            fair_mid,
            volatility_bps,
            computed_at_ns: now,
        }
    }

    fn push_mid(&mut self, mid: f64) {
        self.mid_history[self.mid_history_idx] = mid;
        self.mid_history_idx = (self.mid_history_idx + 1) % VOLATILITY_WINDOW;
        if self.mid_history_len < VOLATILITY_WINDOW {
            self.mid_history_len += 1;
        }
    }

    /// Rolling std of mid in bps (short-term volatility).
    pub fn volatility_bps(&self) -> f64 {
        if self.mid_history_len < 2 {
            return 0.0;
        }
        let n = self.mid_history_len;
        let mean: f64 = self.mid_history[..n].iter().sum::<f64>() / n as f64;
        let var = self.mid_history[..n]
            .iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f64>()
            / (n - 1) as f64;
        let std = var.sqrt();
        if mean > 0.0 {
            (std / mean) * 10_000.0
        } else {
            0.0
        }
    }

    /// Staleness score [0,1] for quoting: higher = more stale data overall.
    pub fn staleness_score(&self, now_ns: u64) -> f64 {
        let mut max_age_sec = 0.0f64;
        for snap in self.snapshots.values() {
            if snap.is_valid() {
                let a = snap.age_sec(now_ns);
                if a > max_age_sec {
                    max_age_sec = a;
                }
            }
        }
        (max_age_sec / (self.max_stale_sec + 0.1)).min(1.0)
    }

    /// Average latency score (normalized) for spread adjustment.
    pub fn latency_score_ns(&self) -> u64 {
        let mut sum = 0u64;
        let mut n = 0usize;
        for snap in self.snapshots.values() {
            if snap.is_valid() {
                sum += snap.latency_ns;
                n += 1;
            }
        }
        if n == 0 {
            0
        } else {
            sum / n as u64
        }
    }
}
