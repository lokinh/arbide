use crate::core::MarketUpdate;
use rand::Rng;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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
        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            let base_price = 50_000.0;
            let volatility = 0.001;
            let exchange = "binance".to_string();
            while running.load(Ordering::SeqCst) {
                let mid_price = base_price * (1.0 + (rng.gen::<f64>() - 0.5) * 2.0 * volatility);
                let half_spread = (rng.gen::<f64>() * 0.1 + 0.15).min(1.0);
                let bid = mid_price - half_spread;
                let ask = mid_price + half_spread;
                let _ = tx.send(MarketUpdate::bid(&symbol, &exchange, bid, 150.0));
                let _ = tx.send(MarketUpdate::ask(&symbol, &exchange, ask, 150.0));
                let delay = rng.gen_range(35..=45);
                thread::sleep(Duration::from_millis(delay));
            }
        });
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
        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            let base_price = 50_000.0;
            let volatility = 0.0012;
            let exchange = "coinbase".to_string();
            while running.load(Ordering::SeqCst) {
                let mid_price = base_price * (1.0 + (rng.gen::<f64>() - 0.5) * 2.0 * volatility);
                let half_spread = (rng.gen::<f64>() * 0.2 + 0.4).min(2.0);
                let bid = mid_price - half_spread;
                let ask = mid_price + half_spread;
                let _ = tx.send(MarketUpdate::bid(&symbol, &exchange, bid, 120.0));
                let _ = tx.send(MarketUpdate::ask(&symbol, &exchange, ask, 120.0));
                let delay = rng.gen_range(50..=70);
                thread::sleep(Duration::from_millis(delay));
            }
        });
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
        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            let base_price = 50_000.0;
            let volatility = 0.0015;
            let exchange = "kraken".to_string();
            while running.load(Ordering::SeqCst) {
                let mid_price = base_price * (1.0 + (rng.gen::<f64>() - 0.5) * 2.0 * volatility);
                let half_spread = (rng.gen::<f64>() * 0.4 + 0.6).min(3.0);
                let bid = mid_price - half_spread;
                let ask = mid_price + half_spread;
                let _ = tx.send(MarketUpdate::bid(&symbol, &exchange, bid, 80.0));
                let _ = tx.send(MarketUpdate::ask(&symbol, &exchange, ask, 80.0));
                let delay = rng.gen_range(70..=150);
                thread::sleep(Duration::from_millis(delay));
            }
        });
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
        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            let base_price = 50_000.0;
            let volatility = 0.002;
            let exchange = "bybit".to_string();
            while running.load(Ordering::SeqCst) {
                let lag = rng.gen_range(0.98..=1.02);
                let mid_price = base_price * (1.0 + (rng.gen::<f64>() - 0.5) * 2.0 * volatility) * lag;
                let half_spread = (rng.gen::<f64>() * 0.15 + 0.25).min(1.5);
                let bid = mid_price - half_spread;
                let ask = mid_price + half_spread;
                let _ = tx.send(MarketUpdate::bid(&symbol, &exchange, bid, 200.0));
                let _ = tx.send(MarketUpdate::ask(&symbol, &exchange, ask, 200.0));
                let delay = rng.gen_range(45..=65);
                thread::sleep(Duration::from_millis(delay));
            }
        });
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
