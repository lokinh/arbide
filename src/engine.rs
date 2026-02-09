use crate::core::{timestamp_ns, MarketUpdateType};
use crate::exchange_fees::maker_bps;
use crate::fair_price::FairPriceEngine;
use crate::feeds::{BinanceFeed, BybitFeed, CoinbaseFeed, ExchangeManager, KrakenFeed};
use crate::position::{InventoryLimits, Position, Side};
use crate::risk::SimpleRiskManager;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const ARB_CSV_HEADER: &str =
    "timestamp,symbol,buy_exchange,sell_exchange,buy_price,sell_price,profit_bps,net_profit_bps,latency_ns,decision\n";
const MM_CSV_HEADER: &str =
    "timestamp,side,quote_price,quote_size,fill_price,fill_size,gross_edge_bps,net_edge_bps,net_pnl\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    Arb,
    Mm,
}

struct PerfTracker {
    total_updates: AtomicU64,
    total_latency_ns: AtomicU64,
    min_latency_ns: AtomicU64,
    max_latency_ns: AtomicU64,
    arbitrage_opportunities: AtomicU64,
    start_time: Instant,
}

impl PerfTracker {
    fn new() -> Self {
        Self {
            total_updates: AtomicU64::new(0),
            total_latency_ns: AtomicU64::new(0),
            min_latency_ns: AtomicU64::new(u64::MAX),
            max_latency_ns: AtomicU64::new(0),
            arbitrage_opportunities: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    fn record_update_latency(&self, latency_ns: u64) {
        self.total_updates.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ns.fetch_add(latency_ns, Ordering::Relaxed);
        let mut current = self.min_latency_ns.load(Ordering::Relaxed);
        while latency_ns < current {
            match self.min_latency_ns.compare_exchange_weak(
                current, latency_ns, Ordering::Relaxed, Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }
        current = self.max_latency_ns.load(Ordering::Relaxed);
        while latency_ns > current {
            match self.max_latency_ns.compare_exchange_weak(
                current, latency_ns, Ordering::Relaxed, Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }
    }

    fn record_arbitrage(&self) {
        self.arbitrage_opportunities.fetch_add(1, Ordering::Relaxed);
    }

    fn print_stats(&self, mode: RunMode) {
        let updates = self.total_updates.load(Ordering::Relaxed);
        if updates == 0 {
            println!("No updates processed yet.");
            return;
        }
        let runtime_sec = self.start_time.elapsed().as_secs_f64();
        let avg_latency = self.total_latency_ns.load(Ordering::Relaxed) / updates;
        let min_lat = self.min_latency_ns.load(Ordering::Relaxed);
        let max_lat = self.max_latency_ns.load(Ordering::Relaxed);
        let opportunities = self.arbitrage_opportunities.load(Ordering::Relaxed);

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                    🚀 ULTRA-FAST ARBIDE 🚀                    ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!(
            "║ Mode:              {:>8}                              ║",
            match mode {
                RunMode::Arb => "arb",
                RunMode::Mm => "mm",
            }
        );
        println!(
            "║ Runtime:           {:>8.1} seconds                        ║",
            runtime_sec
        );
        println!("║ Total Updates:     {:>8}                              ║", updates);
        println!(
            "║ Updates/sec:       {:>8.1}                              ║",
            updates as f64 / runtime_sec
        );
        println!(
            "║ Avg Latency:       {:>8} μs                             ║",
            if min_lat == u64::MAX { 0 } else { avg_latency / 1000 }
        );
        println!(
            "║ Min Latency:       {:>8} μs                             ║",
            if min_lat == u64::MAX { 0 } else { min_lat / 1000 }
        );
        println!(
            "║ Max Latency:       {:>8} μs                             ║",
            max_lat / 1000
        );
        if mode == RunMode::Arb {
            println!("║ Opportunities:     {:>8}                              ║", opportunities);
        }
        println!("╚══════════════════════════════════════════════════════════════╝\n");
    }
}

#[derive(Clone)]
pub struct MmConfig {
    pub primary_exchange: String,
    pub target_spread_bps_min: f64,
    pub max_quote_size: f64,
    pub max_position_btc: f64,
    pub spread_alpha: f64,
}

impl Default for MmConfig {
    fn default() -> Self {
        Self {
            primary_exchange: "binance".to_string(),
            // Spread = 2× maker fee so net edge >= 0. Fill zone widened so we "fill" when quote is within fee of book.
            target_spread_bps_min: 20.0,
            max_quote_size: 0.01,
            max_position_btc: 1.0,
            spread_alpha: 0.2,
        }
    }
}

pub struct Engine {
    mode: RunMode,
    fair_price_engine: Option<FairPriceEngine>,
    detector: Option<crate::core::ArbitrageDetector>,
    risk_manager: Arc<SimpleRiskManager>,
    exchange_manager: ExchangeManager,
    perf: Arc<PerfTracker>,
    csv_path: String,
    mm_csv_path: String,
    mm_config: Option<MmConfig>,
    pub running: Arc<AtomicBool>,
}

impl Engine {
    pub fn new() -> Self {
        Self::new_with_mode(RunMode::Mm)
    }

    pub fn new_with_mode(mode: RunMode) -> Self {
        let mut exchange_manager = ExchangeManager::new();
        exchange_manager.add_feed(Box::new(BinanceFeed::new()));
        exchange_manager.add_feed(Box::new(CoinbaseFeed::new()));
        exchange_manager.add_feed(Box::new(KrakenFeed::new()));
        exchange_manager.add_feed(Box::new(BybitFeed::new()));

        let symbol = "BTCUSDT";
        let mut risk_manager = SimpleRiskManager::new();
        risk_manager.set_risk_limits(10.0, 0.0);
        risk_manager.set_min_edge_bps(0.0);

        let (fair_price_engine, detector, csv_path, mm_csv_path, mm_config) = match mode {
            RunMode::Arb => {
                let mut detector = crate::core::ArbitrageDetector::new();
                for name in exchange_manager.exchange_names() {
                    detector.add_orderbook(symbol, &name);
                }
                // Gross min so that after per-exchange taker fees, net >= risk min (5 bps)
                detector.set_min_profit_bps(25.0);
                (
                    None,
                    Some(detector),
                    "arbitrage_opportunities.csv".to_string(),
                    String::new(),
                    None,
                )
            }
            RunMode::Mm => (
                Some(FairPriceEngine::new(symbol)),
                None,
                String::new(),
                "mm_activity.csv".to_string(),
                Some(MmConfig::default()),
            ),
        };

        Self {
            mode,
            fair_price_engine,
            detector,
            risk_manager: Arc::new(risk_manager),
            exchange_manager,
            perf: Arc::new(PerfTracker::new()),
            csv_path,
            mm_csv_path,
            mm_config,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn run(mut self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        match self.mode {
            RunMode::Arb => self.run_arb(),
            RunMode::Mm => self.run_mm(),
        }
    }

    fn run_arb(&mut self) {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.csv_path)
            .expect("open CSV");
        let _ = file.write_all(ARB_CSV_HEADER.as_bytes());

        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║        ⚡ ARBIDE ENGINE (ARB MODE) ⚡                        ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║ Symbol:            BTCUSDT                                   ║");
        println!(
            "║ Exchanges:         {} active feeds                              ║",
            self.exchange_manager.exchange_count()
        );
        println!("║ Min Profit:        5.0 bps (after fees)                     ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!("\nPress Ctrl+C to stop safely...\n");

        let (tx_feed, rx_feed) = std::sync::mpsc::channel();
        self.exchange_manager.start_all("BTCUSDT", tx_feed);

        let running_stats = Arc::clone(&self.running);
        let perf = Arc::clone(&self.perf);
        let risk = Arc::clone(&self.risk_manager);
        let stats_handle = thread::spawn(move || {
            while running_stats.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(10));
                if running_stats.load(Ordering::SeqCst) {
                    perf.print_stats(RunMode::Arb);
                    let report = risk.generate_report();
                    println!(
                        "📊 RISK: P&L: ${:.2} | Exposure: ${:.0} | Take Rate: {:.1}%",
                        report.daily_pnl, report.total_exposure, report.take_rate * 100.0
                    );
                }
            }
        });

        let detector = self.detector.as_mut().expect("arb mode");
        while self.running.load(Ordering::SeqCst) {
            let update = match rx_feed.recv_timeout(Duration::from_millis(500)) {
                Ok(u) => u,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };

            if let Some(book) = detector.get_orderbook_mut(&update.symbol, &update.exchange) {
                match update.type_ {
                    MarketUpdateType::BidUpdate => book.update_bid(update.price, update.quantity),
                    MarketUpdateType::AskUpdate => book.update_ask(update.price, update.quantity),
                    _ => {}
                }
            }

            let opportunities = detector.check_arbitrage(&update.symbol, update.timestamp_ns);
            let processing_end = timestamp_ns();
            let processing_latency = processing_end.saturating_sub(update.timestamp_ns);
            self.perf.record_update_latency(processing_latency);

            for opp in opportunities {
                self.perf.record_arbitrage();
                let assessment = self.risk_manager.assess_opportunity(&opp);
                let decision_code = assessment.decision as i32;
                let line = format!(
                    "{},{},{},{},{:.2},{:.2},{:.1},{:.1},{},{}\n",
                    opp.detected_at_ns,
                    opp.symbol,
                    opp.buy_exchange,
                    opp.sell_exchange,
                    opp.buy_price,
                    opp.sell_price,
                    opp.profit_bps,
                    assessment.net_profit_bps,
                    opp.latency_ns,
                    decision_code
                );
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            }
        }

        self.exchange_manager.stop_all();
        let _ = stats_handle.join();
        self.perf.print_stats(RunMode::Arb);
        self.print_final_summary_arb();
        println!("✅ Arbide engine stopped safely.");
    }

    fn run_mm(&mut self) {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.mm_csv_path)
            .expect("open MM CSV");
        let _ = file.write_all(MM_CSV_HEADER.as_bytes());

        let config = self.mm_config.as_ref().expect("mm config").clone();
        let limits = InventoryLimits::new(
            config.max_position_btc,
            config.max_position_btc,
            100_000.0,
        );
        let mut position = Position::new();
        let fair_engine = self.fair_price_engine.as_mut().expect("mm mode");

        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║        ⚡ ARBIDE ENGINE (MARKET-MAKING MODE) ⚡                ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║ Symbol:            BTCUSDT                                   ║");
        println!("║ Primary exchange:  {}                                        ║", config.primary_exchange);
        println!("║ Target spread:     {:.0} bps (min)                           ║", config.target_spread_bps_min);
        println!("║ Max quote size:    {:.4} BTC                                 ║", config.max_quote_size);
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!("\nPress Ctrl+C to stop safely...\n");

        let (tx_feed, rx_feed) = std::sync::mpsc::channel();
        self.exchange_manager.start_all("BTCUSDT", tx_feed);

        let mut mm_debug_count: u64 = 0;
        const MM_DEBUG_EVERY: u64 = 3000;

        let running_stats = Arc::clone(&self.running);
        let perf = Arc::clone(&self.perf);
        let risk = Arc::clone(&self.risk_manager);
        let stats_handle = thread::spawn(move || {
            while running_stats.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(10));
                if running_stats.load(Ordering::SeqCst) {
                    perf.print_stats(RunMode::Mm);
                    let report = risk.generate_report();
                    println!(
                        "📊 MM: P&L: ${:.2} | Fills: {} | Quotes: {} | Win rate: {:.1}%",
                        report.daily_pnl, report.fills, report.quotes_sent, report.win_rate * 100.0
                    );
                }
            }
        });

        while self.running.load(Ordering::SeqCst) {
            let update = match rx_feed.recv_timeout(Duration::from_millis(500)) {
                Ok(u) => u,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };

            let processing_start = timestamp_ns();
            match update.type_ {
                MarketUpdateType::BidUpdate => {
                    fair_engine.update_bid(&update.exchange, update.price, update.quantity, update.timestamp_ns);
                }
                MarketUpdateType::AskUpdate => {
                    fair_engine.update_ask(&update.exchange, update.price, update.quantity, update.timestamp_ns);
                }
                _ => {}
            }

            let fair = fair_engine.compute_fair_snapshot();
            if fair.fair_mid <= 0.0 {
                self.perf.record_update_latency(timestamp_ns().saturating_sub(processing_start));
                continue;
            }

            let now_ns = timestamp_ns();
            let latency_score_ns = fair_engine.latency_score_ns();
            let latency_score = (latency_score_ns as f64 / 1e9).min(1.0);
            let vol_score = (fair.volatility_bps / 100.0).min(2.0);
            let staleness = fair_engine.staleness_score(now_ns);
            let effective_spread_bps = config.target_spread_bps_min
                + config.spread_alpha * (latency_score * 10.0 + vol_score * 10.0 + staleness * 10.0);
            let half_spread = (effective_spread_bps / 10_000.0) * fair.fair_mid / 2.0;

            let mut quote_bid = fair.fair_mid - half_spread;
            let mut quote_ask = fair.fair_mid + half_spread;
            let skew = position.base_qty * 0.001;
            if position.base_qty > 0.0 {
                quote_ask -= skew;
                quote_bid -= skew;
            } else if position.base_qty < 0.0 {
                quote_bid += skew;
                quote_ask += skew;
            }

            let (effective_primary_name, primary) = match fair_engine.get_primary_or_any(&config.primary_exchange) {
                Some((name, snap)) => (name, snap),
                None => {
                    mm_debug_count = mm_debug_count.wrapping_add(1);
                    if mm_debug_count % 10000 == 1 {
                        eprintln!(
                            "[MM debug] no valid snapshot for any exchange (primary={})",
                            config.primary_exchange
                        );
                    }
                    self.perf.record_update_latency(timestamp_ns().saturating_sub(processing_start));
                    continue;
                }
            };

            let fill_size = config.max_quote_size;
            let fee_bps = maker_bps(&effective_primary_name);
            let fill_zone = primary.bid * (fee_bps / 10_000.0);
            let primary_spread_bps = ((primary.ask - primary.bid) / fair.fair_mid) * 10_000.0;
            let half_spread_bps = (half_spread / fair.fair_mid) * 10_000.0;
            let buy_cond = quote_bid >= primary.bid - fill_zone;
            let sell_cond = quote_ask <= primary.ask + fill_zone;

            mm_debug_count = mm_debug_count.wrapping_add(1);
            if mm_debug_count % MM_DEBUG_EVERY == 0 {
                eprintln!(
                    "[MM debug] fair_mid={:.2} primary_bid={:.2} primary_ask={:.2} quote_bid={:.2} quote_ask={:.2} | primary_spread_bps={:.2} half_spread_bps={:.2} | buy_cond={} sell_cond={}",
                    fair.fair_mid, primary.bid, primary.ask, quote_bid, quote_ask,
                    primary_spread_bps, half_spread_bps, buy_cond, sell_cond
                );
            }

            // Simulate fills when our quote is inside or at the primary's spread (we get hit as maker).
            // Buy: our bid >= primary's best bid => we're best bid, assume filled at our quote_bid.
            if buy_cond && limits.can_add_long(&position, fill_size) {
                let fill_price = quote_bid;
                let edge_bps = ((fair.fair_mid - fill_price) / fair.fair_mid) * 10_000.0;
                let assessment = self.risk_manager.assess_quote_fill(
                    &limits,
                    &position,
                    Side::Buy,
                    fill_price,
                    fill_size,
                    edge_bps,
                    fair.fair_mid,
                    &effective_primary_name,
                );
                if assessment.allowed {
                    let _ = position.apply_fill(Side::Buy, fill_price, fill_size);
                    self.risk_manager.record_fill(assessment.net_pnl, fill_size * fill_price);
                    let line = format!(
                        "{},buy,{:.2},{:.4},{:.2},{:.4},{:.1},{:.1},{:.4}\n",
                        now_ns,
                        quote_bid,
                        fill_size,
                        fill_price,
                        fill_size,
                        assessment.gross_edge_bps,
                        assessment.edge_bps,
                        assessment.net_pnl
                    );
                    let _ = file.write_all(line.as_bytes());
                    let _ = file.flush();
                }
            }
            // Sell: our ask <= primary's best ask => we're best ask, assume filled at our quote_ask.
            if sell_cond && limits.can_add_short(&position, fill_size) {
                let fill_price = quote_ask;
                let edge_bps = ((fill_price - fair.fair_mid) / fair.fair_mid) * 10_000.0;
                let assessment = self.risk_manager.assess_quote_fill(
                    &limits,
                    &position,
                    Side::Sell,
                    fill_price,
                    fill_size,
                    edge_bps,
                    fair.fair_mid,
                    &effective_primary_name,
                );
                if assessment.allowed {
                    let _ = position.apply_fill(Side::Sell, fill_price, fill_size);
                    self.risk_manager.record_fill(assessment.net_pnl, fill_size * fill_price);
                    let line = format!(
                        "{},sell,{:.2},{:.4},{:.2},{:.4},{:.1},{:.1},{:.4}\n",
                        now_ns,
                        quote_ask,
                        fill_size,
                        fill_price,
                        fill_size,
                        assessment.gross_edge_bps,
                        assessment.edge_bps,
                        assessment.net_pnl
                    );
                    let _ = file.write_all(line.as_bytes());
                    let _ = file.flush();
                }
            }

            self.perf.record_update_latency(timestamp_ns().saturating_sub(processing_start));
        }

        self.exchange_manager.stop_all();
        let _ = stats_handle.join();
        self.perf.print_stats(RunMode::Mm);
        self.print_final_summary_mm(&position);
        println!("✅ Arbide engine stopped safely.");
    }

    fn print_final_summary_arb(&self) {
        let report = self.risk_manager.generate_report();
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                   FINAL SESSION SUMMARY (ARB)                ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!(
            "║ Opportunities Found:  {:>8}                              ║",
            report.opportunities_seen
        );
        println!(
            "║ Trades Executed:      {:>8}                              ║",
            report.opportunities_taken
        );
        println!(
            "║ Take Rate:            {:>7.1}%                              ║",
            report.take_rate * 100.0
        );
        println!(
            "║ Win Rate:             {:>7.1}%                              ║",
            report.win_rate * 100.0
        );
        println!(
            "║ Total P&L:            ${:>7.2}                              ║",
            report.daily_pnl
        );
        println!(
            "║ Total Exposure:       ${:>7.0}                              ║",
            report.total_exposure
        );
        println!("╚══════════════════════════════════════════════════════════════╝");
        let summary = format!(
            "Arbide Session Summary (ARB)\n==================================\nOpportunities: {}\nTrades: {}\nTake Rate: {}%\nWin Rate: {}%\nP&L: ${}\nExposure: ${}\n",
            report.opportunities_seen,
            report.opportunities_taken,
            report.take_rate * 100.0,
            report.win_rate * 100.0,
            report.daily_pnl,
            report.total_exposure
        );
        let _ = std::fs::write("session_summary.txt", summary);
    }

    fn print_final_summary_mm(&self, position: &Position) {
        let report = self.risk_manager.generate_report();
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                   FINAL SESSION SUMMARY (MM)                 ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!(
            "║ Quotes sent:          {:>8}                              ║",
            report.quotes_sent
        );
        println!(
            "║ Fills:                {:>8}                              ║",
            report.fills
        );
        println!(
            "║ Win Rate:             {:>7.1}%                              ║",
            report.win_rate * 100.0
        );
        println!(
            "║ Total P&L:            ${:>7.2}                              ║",
            report.daily_pnl
        );
        println!(
            "║ Inventory (BTC):       {:>7.4}                              ║",
            position.base_qty
        );
        println!("╚══════════════════════════════════════════════════════════════╝");
        let summary = format!(
            "Arbide Session Summary (MM)\n==================================\nQuotes: {}\nFills: {}\nWin Rate: {}%\nP&L: ${}\nInventory: {} BTC\n",
            report.quotes_sent,
            report.fills,
            report.win_rate * 100.0,
            report.daily_pnl,
            position.base_qty
        );
        let _ = std::fs::write("session_summary.txt", summary);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}
