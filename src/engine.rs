use crate::core::{ArbitrageDetector, MarketUpdateType, timestamp_ns};
use crate::feeds::{BinanceFeed, BybitFeed, CoinbaseFeed, ExchangeManager, KrakenFeed};
use crate::risk::{Decision, SimpleRiskManager};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const CSV_HEADER: &str = "timestamp,symbol,buy_exchange,sell_exchange,buy_price,sell_price,profit_bps,net_profit_bps,latency_ns,decision\n";

struct PerfTracker {
    total_updates: AtomicU64,
    total_latency_ns: AtomicU64,
    min_latency_ns: AtomicU64,
    max_latency_ns: AtomicU64,
    arbitrage_opportunities: AtomicU64,
    trades_executed: AtomicU64,
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
            trades_executed: AtomicU64::new(0),
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
    fn record_trade(&self) {
        self.trades_executed.fetch_add(1, Ordering::Relaxed);
    }

    fn print_stats(&self) {
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
        let trades = self.trades_executed.load(Ordering::Relaxed);

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                    🚀 ULTRA-FAST ARBIDE 🚀                    ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
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
        println!("║ Opportunities:     {:>8}                              ║", opportunities);
        println!("║ Trades Executed:   {:>8}                              ║", trades);
        if opportunities > 0 {
            println!(
                "║ Execution Rate:    {:>7.1}%                              ║",
                (trades as f64 / opportunities as f64) * 100.0
            );
        }
        println!("╚══════════════════════════════════════════════════════════════╝\n");
    }
}

pub struct Engine {
    detector: ArbitrageDetector,
    risk_manager: Arc<SimpleRiskManager>,
    exchange_manager: ExchangeManager,
    perf: Arc<PerfTracker>,
    csv_path: String,
    pub running: Arc<AtomicBool>,
}

impl Engine {
    pub fn new() -> Self {
        let mut detector = ArbitrageDetector::new();
        let mut exchange_manager = ExchangeManager::new();
        exchange_manager.add_feed(Box::new(BinanceFeed::new()));
        exchange_manager.add_feed(Box::new(CoinbaseFeed::new()));
        exchange_manager.add_feed(Box::new(KrakenFeed::new()));
        exchange_manager.add_feed(Box::new(BybitFeed::new()));

        let symbol = "BTCUSDT";
        for name in exchange_manager.exchange_names() {
            detector.add_orderbook(symbol, &name);
        }
        detector.set_min_profit_bps(5.0);

        let mut risk_manager = SimpleRiskManager::new();
        risk_manager.set_risk_limits(10.0, -5.0);

        Self {
            detector,
            risk_manager: Arc::new(risk_manager),
            exchange_manager,
            perf: Arc::new(PerfTracker::new()),
            csv_path: "arbitrage_opportunities.csv".to_string(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn run(mut self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.csv_path)
            .expect("open CSV");
        let _ = file.write_all(CSV_HEADER.as_bytes());

        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║        ⚡ ULTRA-FAST ARBIDE ENGINE STARTING ⚡                ║");
        println!("║                  (Rust - Zero External Deps)                 ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║ Symbol:            BTCUSDT                                   ║");
        println!(
            "║ Exchanges:         {} active feeds                              ║",
            self.exchange_manager.exchange_count()
        );
        println!("║ Risk Management:   BASIC (Ultra-fast mode)                   ║");
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
                    perf.print_stats();
                    let report = risk.generate_report();
                    println!(
                        "📊 RISK SUMMARY: P&L: ${:.2} | Exposure: ${:.0} | Take Rate: {:.1}%",
                        report.daily_pnl, report.total_exposure, report.take_rate * 100.0
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

            if let Some(book) = self.detector.get_orderbook_mut(&update.symbol, &update.exchange) {
                match update.type_ {
                    MarketUpdateType::BidUpdate => book.update_bid(update.price, update.quantity),
                    MarketUpdateType::AskUpdate => book.update_ask(update.price, update.quantity),
                    _ => {}
                }
            }

            let opportunities = self.detector.check_arbitrage(&update.symbol, update.timestamp_ns);
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

                if assessment.decision == Decision::Approved {
                    println!("==> APPROVED ARBITRAGE OPPORTUNITY <==");
                    self.perf.record_trade();
                } else {
                    println!("==> ARBITRAGE OPPORTUNITY (REJECTED) <==");
                }
                println!(
                    "Symbol: {} | Buy: {} @ ${:.2} | Sell: {} @ ${:.2}",
                    opp.symbol, opp.buy_exchange, opp.buy_price, opp.sell_exchange, opp.sell_price
                );
                println!(
                    "Gross Profit: {:.1} bps | Net Profit: {:.1} bps | Latency: {} μs",
                    opp.profit_bps, assessment.net_profit_bps, opp.latency_ns / 1000
                );
                if assessment.decision != Decision::Approved {
                    println!("✗ Rejected: {}", assessment.reason);
                } else {
                    println!("✓ Trade Size: {:.4} BTC", assessment.recommended_size);
                    let gross_pnl =
                        (opp.sell_price - opp.buy_price) * assessment.recommended_size;
                    let fees = (assessment.recommended_size * opp.buy_price
                        + assessment.recommended_size * opp.sell_price)
                        * 0.001;
                    println!("$ Expected P&L: ${:.2}", gross_pnl - fees);
                }
                println!("----------------------------------------");
            }
        }

        self.exchange_manager.stop_all();
        let _ = stats_handle.join();

        self.perf.print_stats();
        self.print_final_summary();
        println!("✅ Arbide engine stopped safely.");
    }

    fn print_final_summary(&self) {
        let report = self.risk_manager.generate_report();
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                   FINAL SESSION SUMMARY                      ║");
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
            "Arbide Ultra-Fast Session Summary\n==================================\nMode: Rust (Ultra-Fast)\nOpportunities Found: {}\nTrades Executed: {}\nTake Rate: {}%\nWin Rate: {}%\nTotal P&L: ${}\nTotal Exposure: ${}\n",
            report.opportunities_seen,
            report.opportunities_taken,
            report.take_rate * 100.0,
            report.win_rate * 100.0,
            report.daily_pnl,
            report.total_exposure
        );
        let _ = std::fs::write("session_summary.txt", summary);
        println!("\n📄 Session summary saved to: session_summary.txt");
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}