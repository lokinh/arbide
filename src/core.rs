use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LEVELS: usize = 10;

pub fn timestamp_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Debug, Default)]
pub struct PriceLevel {
    pub price: f64,
    pub quantity: f64,
    pub timestamp_ns: u64,
}

impl PriceLevel {
    pub fn new(price: f64, quantity: f64) -> Self {
        Self {
            price,
            quantity,
            timestamp_ns: timestamp_ns(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketUpdateType {
    BidUpdate,
    AskUpdate,
    Trade,
}

#[derive(Clone, Debug)]
pub struct MarketUpdate {
    pub type_: MarketUpdateType,
    pub symbol: String,
    pub exchange: String,
    pub price: f64,
    pub quantity: f64,
    pub timestamp_ns: u64,
    pub sequence_id: u64,
}

impl MarketUpdate {
    pub fn bid(symbol: &str, exchange: &str, price: f64, quantity: f64) -> Self {
        Self {
            type_: MarketUpdateType::BidUpdate,
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            price,
            quantity,
            timestamp_ns: timestamp_ns(),
            sequence_id: 0,
        }
    }
    pub fn ask(symbol: &str, exchange: &str, price: f64, quantity: f64) -> Self {
        Self {
            type_: MarketUpdateType::AskUpdate,
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            price,
            quantity,
            timestamp_ns: timestamp_ns(),
            sequence_id: 0,
        }
    }
}

#[derive(Debug)]
pub struct FastOrderBook {
    symbol: String,
    exchange: String,
    bids: BookSide,
    asks: BookSide,
}

#[derive(Debug, Default)]
struct BookSide {
    levels: [PriceLevel; MAX_LEVELS],
    count: AtomicU64,
    last_update_ns: AtomicU64,
}

impl FastOrderBook {
    pub fn new(symbol: &str, exchange: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            bids: BookSide::default(),
            asks: BookSide::default(),
        }
    }

    pub fn update_bid(&mut self, price: f64, quantity: f64) {
        let count = self.bids.count.load(Ordering::Relaxed) as usize;
        let levels = &mut self.bids.levels;

        for i in 0..count {
            if (levels[i].price - price).abs() < 1e-12 {
                levels[i].quantity = quantity;
                levels[i].timestamp_ns = timestamp_ns();
                self.bids.last_update_ns.store(timestamp_ns(), Ordering::Relaxed);
                return;
            }
            if levels[i].price < price {
                let j_end = count.min(MAX_LEVELS - 1);
                for j in (i + 1..=j_end).rev() {
                    levels[j] = levels[j - 1].clone();
                }
                levels[i] = PriceLevel::new(price, quantity);
                if count < MAX_LEVELS {
                    self.bids.count.store((count + 1) as u64, Ordering::Relaxed);
                }
                self.bids.last_update_ns.store(timestamp_ns(), Ordering::Relaxed);
                return;
            }
        }
        if count < MAX_LEVELS {
            levels[count] = PriceLevel::new(price, quantity);
            self.bids.count.store((count + 1) as u64, Ordering::Relaxed);
            self.bids.last_update_ns.store(timestamp_ns(), Ordering::Relaxed);
        }
    }

    pub fn update_ask(&mut self, price: f64, quantity: f64) {
        let count = self.asks.count.load(Ordering::Relaxed) as usize;
        let levels = &mut self.asks.levels;

        for i in 0..count {
            if (levels[i].price - price).abs() < 1e-12 {
                levels[i].quantity = quantity;
                levels[i].timestamp_ns = timestamp_ns();
                self.asks.last_update_ns.store(timestamp_ns(), Ordering::Relaxed);
                return;
            }
            if levels[i].price > price {
                let j_end = count.min(MAX_LEVELS - 1);
                for j in (i + 1..=j_end).rev() {
                    levels[j] = levels[j - 1].clone();
                }
                levels[i] = PriceLevel::new(price, quantity);
                if count < MAX_LEVELS {
                    self.asks.count.store((count + 1) as u64, Ordering::Relaxed);
                }
                self.asks.last_update_ns.store(timestamp_ns(), Ordering::Relaxed);
                return;
            }
        }
        if count < MAX_LEVELS {
            levels[count] = PriceLevel::new(price, quantity);
            self.asks.count.store((count + 1) as u64, Ordering::Relaxed);
            self.asks.last_update_ns.store(timestamp_ns(), Ordering::Relaxed);
        }
    }

    pub fn get_best_bid_ask(&self) -> (f64, f64) {
        let count_bid = self.bids.count.load(Ordering::Relaxed) as usize;
        let count_ask = self.asks.count.load(Ordering::Relaxed) as usize;
        let best_bid = if count_bid > 0 {
            self.bids.levels[0].price
        } else {
            0.0
        };
        let best_ask = if count_ask > 0 {
            self.asks.levels[0].price
        } else {
            0.0
        };
        (best_bid, best_ask)
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }
    pub fn exchange(&self) -> &str {
        &self.exchange
    }
}

#[derive(Clone, Debug)]
pub struct ArbitrageOpportunity {
    pub symbol: String,
    pub buy_exchange: String,
    pub sell_exchange: String,
    pub buy_price: f64,
    pub sell_price: f64,
    pub profit_bps: f64,
    pub detected_at_ns: u64,
    pub latency_ns: u64,
}

impl ArbitrageOpportunity {
    pub fn new(
        symbol: &str,
        buy_exchange: &str,
        sell_exchange: &str,
        buy_price: f64,
        sell_price: f64,
        update_time_ns: u64,
    ) -> Self {
        let detected_at_ns = timestamp_ns();
        let latency_ns = detected_at_ns.saturating_sub(update_time_ns);
        let profit_bps = ((sell_price - buy_price) / buy_price) * 10_000.0;
        Self {
            symbol: symbol.to_string(),
            buy_exchange: buy_exchange.to_string(),
            sell_exchange: sell_exchange.to_string(),
            buy_price,
            sell_price,
            profit_bps,
            detected_at_ns,
            latency_ns,
        }
    }
}

pub struct ArbitrageDetector {
    books: HashMap<String, HashMap<String, FastOrderBook>>,
    min_profit_bps: f64,
}

impl ArbitrageDetector {
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
            min_profit_bps: 5.0,
        }
    }

    pub fn add_orderbook(&mut self, symbol: &str, exchange: &str) {
        self.books
            .entry(symbol.to_string())
            .or_default()
            .insert(exchange.to_string(), FastOrderBook::new(symbol, exchange));
    }

    pub fn set_min_profit_bps(&mut self, bps: f64) {
        self.min_profit_bps = bps;
    }

    pub fn get_orderbook_mut(
        &mut self,
        symbol: &str,
        exchange: &str,
    ) -> Option<&mut FastOrderBook> {
        self.books.get_mut(symbol)?.get_mut(exchange)
    }

    pub fn check_arbitrage(&self, symbol: &str, update_time_ns: u64) -> Vec<ArbitrageOpportunity> {
        let mut opportunities = Vec::new();
        let exchanges = match self.books.get(symbol) {
            Some(ex) if ex.len() >= 2 => ex,
            _ => return opportunities,
        };

        let exchanges: Vec<_> = exchanges.iter().collect();
        for i in 0..exchanges.len() {
            for j in (i + 1)..exchanges.len() {
                let (ex1, book1) = exchanges[i];
                let (ex2, book2) = exchanges[j];
                let (bid1, ask1) = book1.get_best_bid_ask();
                let (bid2, ask2) = book2.get_best_bid_ask();

                if ask1 > 0.0 && bid2 > 0.0 && bid2 > ask1 {
                    let profit_bps = ((bid2 - ask1) / ask1) * 10_000.0;
                    if profit_bps >= self.min_profit_bps {
                        opportunities.push(ArbitrageOpportunity::new(
                            symbol, ex1, ex2, ask1, bid2, update_time_ns,
                        ));
                    }
                }
                if ask2 > 0.0 && bid1 > 0.0 && bid1 > ask2 {
                    let profit_bps = ((bid1 - ask2) / ask2) * 10_000.0;
                    if profit_bps >= self.min_profit_bps {
                        opportunities.push(ArbitrageOpportunity::new(
                            symbol, ex2, ex1, ask2, bid1, update_time_ns,
                        ));
                    }
                }
            }
        }
        opportunities
    }
}

impl Default for ArbitrageDetector {
    fn default() -> Self {
        Self::new()
    }
}
