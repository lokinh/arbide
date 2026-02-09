use crate::core::ArbitrageOpportunity;
use crate::exchange_fees::{arb_round_trip_bps, maker_bps};
use crate::position::{InventoryLimits, Position, Side};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Decision {
    Approved = 0,
    RejectedProfit = 1,
    RejectedSize = 2,
}

#[derive(Clone, Debug)]
pub struct Assessment {
    pub decision: Decision,
    pub recommended_size: f64,
    pub reason: String,
    pub net_profit_bps: f64,
}

/// Result of assessing a quote/fill for market-making (inventory, exposure, min edge).
#[derive(Clone, Debug)]
pub struct QuoteFillAssessment {
    pub allowed: bool,
    pub reason: String,
    /// Gross edge in bps (spread captured before fee).
    pub gross_edge_bps: f64,
    /// Net edge in bps (gross minus maker fee); can be negative.
    pub edge_bps: f64,
    pub net_pnl: f64,
}

#[derive(Clone, Debug, Default)]
pub struct RiskReport {
    pub opportunities_seen: u64,
    pub opportunities_taken: u64,
    pub take_rate: f64,
    pub daily_pnl: f64,
    pub total_exposure: f64,
    pub active_positions: u64,
    pub current_drawdown: f64,
    pub win_rate: f64,
    /// Market-making: quotes sent (updates).
    pub quotes_sent: u64,
    /// Market-making: simulated fills.
    pub fills: u64,
    /// Market-making: inventory skew (base_qty).
    pub inventory_skew: f64,
}

pub struct SimpleRiskManager {
    max_trade_size: f64,
    min_profit_bps: f64,
    /// Minimum expected edge (bps) for a quote/fill after fees and volatility.
    min_edge_bps: f64,
    opportunities_seen: AtomicU64,
    opportunities_taken: AtomicU64,
    daily_pnl: std::sync::atomic::AtomicU64,
    win_trades: AtomicU64,
    total_exposure: std::sync::atomic::AtomicU64,
    quotes_sent: AtomicU64,
    fills: AtomicU64,
    fill_win_count: AtomicU64,
}

impl Default for SimpleRiskManager {
    fn default() -> Self {
        Self {
            max_trade_size: 0.5,
            min_profit_bps: 5.0,
            min_edge_bps: 10.0,
            opportunities_seen: AtomicU64::new(0),
            opportunities_taken: AtomicU64::new(0),
            daily_pnl: std::sync::atomic::AtomicU64::new(0),
            win_trades: AtomicU64::new(0),
            total_exposure: std::sync::atomic::AtomicU64::new(0),
            quotes_sent: AtomicU64::new(0),
            fills: AtomicU64::new(0),
            fill_win_count: AtomicU64::new(0),
        }
    }
}

impl SimpleRiskManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_risk_limits(&mut self, max_trade: f64, min_profit: f64) {
        self.max_trade_size = max_trade;
        self.min_profit_bps = min_profit;
    }

    pub fn set_min_edge_bps(&mut self, bps: f64) {
        self.min_edge_bps = bps;
    }

    /// Assess whether a quote/fill is allowed (inventory, exposure, minimum edge).
    /// `primary_exchange`: exchange where we are maker; fee uses that exchange's maker rate.
    pub fn assess_quote_fill(
        &self,
        limits: &InventoryLimits,
        position: &Position,
        side: Side,
        fill_price: f64,
        fill_size: f64,
        edge_bps: f64,
        mark_price: f64,
        primary_exchange: &str,
    ) -> QuoteFillAssessment {
        self.quotes_sent.fetch_add(1, Ordering::Relaxed);

        if edge_bps < self.min_edge_bps {
            return QuoteFillAssessment {
                allowed: false,
                reason: format!("Edge {} bps below min {} bps", edge_bps, self.min_edge_bps),
                gross_edge_bps: edge_bps,
                edge_bps,
                net_pnl: 0.0,
            };
        }
        let (can_add, limit_reason) = match side {
            Side::Buy => (
                limits.can_add_long(position, fill_size),
                "long limit",
            ),
            Side::Sell => (
                limits.can_add_short(position, fill_size),
                "short limit",
            ),
        };
        if !can_add {
            return QuoteFillAssessment {
                allowed: false,
                reason: limit_reason.to_string(),
                gross_edge_bps: edge_bps,
                edge_bps,
                net_pnl: 0.0,
            };
        }
        let new_base = match side {
            Side::Buy => position.base_qty + fill_size,
            Side::Sell => position.base_qty - fill_size,
        };
        if new_base.abs() * mark_price > limits.max_notional_exposure {
            return QuoteFillAssessment {
                allowed: false,
                reason: "exposure limit".to_string(),
                gross_edge_bps: edge_bps,
                edge_bps,
                net_pnl: 0.0,
            };
        }
        let fees_bps = maker_bps(primary_exchange);
        let net_bps = edge_bps - fees_bps;
        if net_bps < 0.0 {
            return QuoteFillAssessment {
                allowed: false,
                reason: format!("Net edge negative (gross {} bps - fee {} bps)", edge_bps, fees_bps),
                gross_edge_bps: edge_bps,
                edge_bps: net_bps,
                net_pnl: 0.0,
            };
        }
        let notional = fill_size * fill_price;
        let fees = notional * (fees_bps / 10_000.0);
        let gross = (edge_bps / 10_000.0) * notional;
        let net_pnl = gross - fees;

        QuoteFillAssessment {
            allowed: true,
            reason: "OK".to_string(),
            gross_edge_bps: edge_bps,
            edge_bps: net_bps,
            net_pnl,
        }
    }

    pub fn record_fill(&self, net_pnl: f64, exposure_delta: f64) {
        self.fills.fetch_add(1, Ordering::Relaxed);
        if net_pnl > 0.0 {
            self.fill_win_count.fetch_add(1, Ordering::Relaxed);
        }
        let cur = f64::from_bits(self.daily_pnl.load(Ordering::Relaxed));
        self.daily_pnl.store((cur + net_pnl).to_bits(), Ordering::Relaxed);
        let exp = f64::from_bits(self.total_exposure.load(Ordering::Relaxed));
        self.total_exposure
            .store((exp + exposure_delta).to_bits(), Ordering::Relaxed);
    }

    pub fn assess_opportunity(&self, opp: &ArbitrageOpportunity) -> Assessment {
        self.opportunities_seen.fetch_add(1, Ordering::Relaxed);

        let fees_bps = arb_round_trip_bps(&opp.buy_exchange, &opp.sell_exchange);
        let net_profit_bps = opp.profit_bps - fees_bps;

        if net_profit_bps < self.min_profit_bps {
            return Assessment {
                decision: Decision::RejectedProfit,
                recommended_size: 0.0,
                reason: format!(
                    "Net profit below threshold ({} < {} bps)",
                    net_profit_bps, self.min_profit_bps
                ),
                net_profit_bps,
            };
        }

        let recommended_size = self.max_trade_size;
        if recommended_size < 0.001 {
            return Assessment {
                decision: Decision::RejectedSize,
                recommended_size: 0.0,
                reason: format!("Recommended trade size too small: {}", recommended_size),
                net_profit_bps,
            };
        }

        self.opportunities_taken.fetch_add(1, Ordering::Relaxed);

        let gross_pnl = (opp.sell_price - opp.buy_price) * recommended_size;
        let buy_notional = recommended_size * opp.buy_price;
        let sell_notional = recommended_size * opp.sell_price;
        let fees = buy_notional * (crate::exchange_fees::taker_bps(&opp.buy_exchange) / 10_000.0)
            + sell_notional * (crate::exchange_fees::taker_bps(&opp.sell_exchange) / 10_000.0);
        let net_pnl = gross_pnl - fees;

        let current = f64::from_bits(self.daily_pnl.load(Ordering::Relaxed));
        self.daily_pnl.store((current + net_pnl).to_bits(), Ordering::Relaxed);

        if net_pnl > 0.0 {
            self.win_trades.fetch_add(1, Ordering::Relaxed);
        }

        let exposure_increment =
            (recommended_size * opp.buy_price).abs() + (recommended_size * opp.sell_price).abs();
        let exposure_cur = f64::from_bits(self.total_exposure.load(Ordering::Relaxed));
        self.total_exposure
            .store((exposure_cur + exposure_increment).to_bits(), Ordering::Relaxed);

        // println!(
        //     "[DEBUG] APPROVED: Size={} BTC, Expected P&L=${}",
        //     recommended_size, net_pnl
        // );

        Assessment {
            decision: Decision::Approved,
            recommended_size,
            reason: "Trade approved".to_string(),
            net_profit_bps,
        }
    }

    pub fn generate_report(&self) -> RiskReport {
        let opportunities_seen = self.opportunities_seen.load(Ordering::Relaxed);
        let opportunities_taken = self.opportunities_taken.load(Ordering::Relaxed);
        let daily_pnl = f64::from_bits(self.daily_pnl.load(Ordering::Relaxed));
        let win_trades = self.win_trades.load(Ordering::Relaxed);
        let total_exposure = f64::from_bits(self.total_exposure.load(Ordering::Relaxed));
        let quotes_sent = self.quotes_sent.load(Ordering::Relaxed);
        let fills = self.fills.load(Ordering::Relaxed);
        let fill_win_count = self.fill_win_count.load(Ordering::Relaxed);

        let take_rate = if opportunities_seen > 0 {
            opportunities_taken as f64 / opportunities_seen as f64
        } else {
            0.0
        };

        let win_rate = if opportunities_taken > 0 {
            win_trades as f64 / opportunities_taken as f64
        } else if fills > 0 {
            fill_win_count as f64 / fills as f64
        } else {
            0.0
        };

        RiskReport {
            opportunities_seen,
            opportunities_taken,
            take_rate,
            daily_pnl,
            total_exposure,
            active_positions: 0,
            current_drawdown: 0.0,
            win_rate,
            quotes_sent,
            fills,
            inventory_skew: 0.0,
        }
    }
}
