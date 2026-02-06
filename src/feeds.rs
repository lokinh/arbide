use crate::core::MarketUpdate;
use crate::ws::{binance, bybit, coinbase, kraken};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

pub trait ExchangeFeed: Send + Sync + 'static {
    fn exchange_name(&self) -> &str;
    fn start(&self, symbol: String, tx: Sender<MarketUpdate>);
    fn stop(&self);
}

pub struct BinanceFeed {
    running: Arc<AtomicBool>,
}

impl BinanceFeed {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for BinanceFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl ExchangeFeed for BinanceFeed {
    fn exchange_name(&self) -> &str {
        "binance"
    }
    fn start(&self, symbol: String, tx: Sender<MarketUpdate>) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let running = Arc::clone(&self.running);
        binance::start_binance_ws(symbol, tx, running);
    }
    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

pub struct CoinbaseFeed {
    running: Arc<AtomicBool>,
}

impl CoinbaseFeed {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for CoinbaseFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl ExchangeFeed for CoinbaseFeed {
    fn exchange_name(&self) -> &str {
        "coinbase"
    }
    fn start(&self, symbol: String, tx: Sender<MarketUpdate>) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let running = Arc::clone(&self.running);
        coinbase::start_coinbase_ws(symbol, tx, running);
    }
    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

pub struct KrakenFeed {
    running: Arc<AtomicBool>,
}

impl KrakenFeed {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for KrakenFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl ExchangeFeed for KrakenFeed {
    fn exchange_name(&self) -> &str {
        "kraken"
    }
    fn start(&self, symbol: String, tx: Sender<MarketUpdate>) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let running = Arc::clone(&self.running);
        kraken::start_kraken_ws(symbol, tx, running);
    }
    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

pub struct BybitFeed {
    running: Arc<AtomicBool>,
}

impl BybitFeed {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for BybitFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl ExchangeFeed for BybitFeed {
    fn exchange_name(&self) -> &str {
        "bybit"
    }
    fn start(&self, symbol: String, tx: Sender<MarketUpdate>) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let running = Arc::clone(&self.running);
        bybit::start_bybit_ws(symbol, tx, running);
    }
    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

pub struct ExchangeManager {
    feeds: Vec<Box<dyn ExchangeFeed>>,
}

impl ExchangeManager {
    pub fn new() -> Self {
        Self { feeds: Vec::new() }
    }

    pub fn add_feed(&mut self, feed: Box<dyn ExchangeFeed>) {
        self.feeds.push(feed);
    }

    pub fn start_all(&self, symbol: &str, tx: Sender<MarketUpdate>) {
        let symbol = symbol.to_uppercase();
        println!("Starting {} exchange feeds...", self.feeds.len());
        for feed in &self.feeds {
            println!("  Starting {} feed", feed.exchange_name());
            feed.start(symbol.clone(), tx.clone());
        }
    }

    pub fn stop_all(&self) {
        println!("Stopping all exchange feeds...");
        for feed in &self.feeds {
            feed.stop();
        }
    }

    pub fn exchange_count(&self) -> usize {
        self.feeds.len()
    }

    pub fn exchange_names(&self) -> Vec<String> {
        self.feeds.iter().map(|f| f.exchange_name().to_string()).collect()
    }
}

impl Default for ExchangeManager {
    fn default() -> Self {
        Self::new()
    }
}
