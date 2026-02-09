// Position and inventory tracking for the primary exchange (market-making mode).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Default)]
pub struct Position {
    pub base_qty: f64,
    pub quote_qty: f64,
    pub entry_value_long: f64,
    pub entry_value_short: f64,
    pub realized_pnl: f64,
}

impl Position {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn unrealized_pnl(&self, mark_price: f64) -> f64 {
        if self.base_qty.abs() < 1e-12 {
            return 0.0;
        }
        if self.base_qty > 0.0 {
            (mark_price * self.base_qty) - self.entry_value_long
        } else {
            self.entry_value_short - (mark_price * (-self.base_qty))
        }
    }

    /// Apply a fill and return realized PnL from this fill (e.g. from closing part of position).
    pub fn apply_fill(&mut self, side: Side, price: f64, qty: f64) -> f64 {
        let mut realized = 0.0;
        match side {
            Side::Buy => {
                if self.base_qty < 0.0 {
                    let close_short = qty.min(-self.base_qty);
                    let avg_short = self.entry_value_short / (-self.base_qty).max(1e-12);
                    realized = (avg_short - price) * close_short;
                    self.entry_value_short -= avg_short * close_short;
                    self.base_qty += close_short;
                    let add_long = qty - close_short;
                    if add_long > 0.0 {
                        self.base_qty += add_long;
                        self.entry_value_long += price * add_long;
                    }
                } else {
                    self.base_qty += qty;
                    self.entry_value_long += price * qty;
                }
                self.quote_qty -= price * qty;
            }
            Side::Sell => {
                if self.base_qty > 0.0 {
                    let close_long = qty.min(self.base_qty);
                    let avg_long = self.entry_value_long / self.base_qty.max(1e-12);
                    realized = (price - avg_long) * close_long;
                    self.entry_value_long -= avg_long * close_long;
                    self.base_qty -= close_long;
                    let add_short = qty - close_long;
                    if add_short > 0.0 {
                        self.base_qty -= add_short;
                        self.entry_value_short += price * add_short;
                    }
                } else {
                    self.base_qty -= qty;
                    self.entry_value_short += price * qty;
                }
                self.quote_qty += price * qty;
            }
        }
        self.realized_pnl += realized;
        realized
    }
}

#[derive(Clone, Debug)]
pub struct InventoryLimits {
    pub max_long_btc: f64,
    pub max_short_btc: f64,
    pub max_notional_exposure: f64,
}

impl Default for InventoryLimits {
    fn default() -> Self {
        Self {
            max_long_btc: 1.0,
            max_short_btc: 1.0,
            max_notional_exposure: 100_000.0,
        }
    }
}

impl InventoryLimits {
    pub fn new(max_long_btc: f64, max_short_btc: f64, max_notional_exposure: f64) -> Self {
        Self {
            max_long_btc,
            max_short_btc,
            max_notional_exposure,
        }
    }

    pub fn can_add_long(&self, position: &Position, qty: f64) -> bool {
        if qty <= 0.0 {
            return true;
        }
        let new_long = (position.base_qty + qty).max(0.0);
        new_long <= self.max_long_btc
    }

    pub fn can_add_short(&self, position: &Position, qty: f64) -> bool {
        if qty <= 0.0 {
            return true;
        }
        let new_short = (-(position.base_qty - qty)).max(0.0);
        new_short <= self.max_short_btc
    }

    pub fn current_exposure(&self, position: &Position, mark_price: f64) -> f64 {
        (position.base_qty.abs() * mark_price).min(self.max_notional_exposure)
    }

    pub fn within_exposure(&self, position: &Position, mark_price: f64) -> bool {
        position.base_qty.abs() * mark_price <= self.max_notional_exposure
    }
}
