use crate::core::ArbitrageOpportunity;
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
}

pub struct SimpleRiskManager {
    max_trade_size: f64,
    min_profit_bps: f64,
    opportunities_seen: AtomicU64,
    opportunities_taken: AtomicU64,
    daily_pnl: std::sync::atomic::AtomicU64,
    win_trades: AtomicU64,
    total_exposure: std::sync::atomic::AtomicU64,
}

impl Default for SimpleRiskManager {
    fn default() -> Self {
        Self {
            max_trade_size: 0.5,
            min_profit_bps: 5.0,
            opportunities_seen: AtomicU64::new(0),
            opportunities_taken: AtomicU64::new(0),
            daily_pnl: std::sync::atomic::AtomicU64::new(0),
            win_trades: AtomicU64::new(0),
            total_exposure: std::sync::atomic::AtomicU64::new(0),
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
        // println!(
        //     "[DEBUG] Risk limits updated: Max trade={} BTC, Min profit={} bps",
        //     self.max_trade_size, self.min_profit_bps
        // );
    }

    pub fn assess_opportunity(&self, opp: &ArbitrageOpportunity) -> Assessment {
        self.opportunities_seen.fetch_add(1, Ordering::Relaxed);

        let fees_bps = 20.0;
        let net_profit_bps = opp.profit_bps - fees_bps;

        // println!(
        //     "[DEBUG] Gross: {} bps, Fees: {} bps, Net: {} bps, Min Required: {} bps",
        //     opp.profit_bps, fees_bps, net_profit_bps, self.min_profit_bps
        // );

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
        let fees = (recommended_size * opp.buy_price + recommended_size * opp.sell_price) * 0.001;
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

        let take_rate = if opportunities_seen > 0 {
            opportunities_taken as f64 / opportunities_seen as f64
        } else {
            0.0
        };

        let win_rate = if opportunities_taken > 0 {
            win_trades as f64 / opportunities_taken as f64
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
        }
    }
}
