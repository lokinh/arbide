# Arbide — Real-Time Arbitrage Detection & Market-Making (Simulator)

`arbide` is a small Rust project that connects to multiple exchange WebSocket feeds, maintains order book snapshots, and supports two modes: **cross-exchange arbitrage detection** and **single-exchange market-making** with a fair price derived from all feeds.

## Features

- **Multi-exchange WebSocket feeds**: Binance, Coinbase, Kraken, Bybit (real-time best bid/ask)
- **Two run modes**:
  - **Arb mode** (`--mode arb`): Cross-exchange arbitrage detection; logs opportunities to CSV
  - **MM mode** (`--mode mm`, default): Market-making on one primary exchange using a weighted fair price from all feeds; latency- and volatility-aware spread; simulated fills and position/inventory tracking
- Fair price from weighted mid across exchanges (staleness and latency weighted)
- Short-term volatility estimate (rolling window) and staleness score for spread adjustment
- Basic risk checks: inventory limits, exposure, minimum edge (MM); fee-adjusted profit and max size (Arb)
- CSV output for analysis

## Requirements

- Rust toolchain + Cargo (stable)

## Run

From the `arbide` directory:

```bash
# Market-making mode (default): primary exchange quoting, fair price from 4 feeds, simulated fills
cargo run --release

# Cross-exchange arbitrage detection only (legacy)
cargo run --release -- --mode arb
```

Stop safely with `Ctrl+C`.

## Outputs

- **Arb mode**: `arbitrage_opportunities.csv`  
  Header: `timestamp,symbol,buy_exchange,sell_exchange,buy_price,sell_price,profit_bps,net_profit_bps,latency_ns,decision`
- **MM mode**: `mm_activity.csv`  
  Header: `timestamp,side,quote_price,quote_size,fill_price,fill_size,gross_edge_bps,net_edge_bps,net_pnl`  
  `gross_edge_bps` = spread captured (bps) before fee; `net_edge_bps` = gross minus maker fee (can be negative if spread &lt; fee).
- `session_summary.txt` (both modes)

## Configuration

### Run mode

- `--mode mm` (default): Market-making on primary exchange
- `--mode arb`: Cross-exchange arbitrage detection only

### MM mode (code)

In `src/engine.rs`, `MmConfig` (defaults):

- **Primary exchange**: `binance` (where bid/ask are quoted and fills are simulated)
- **Target spread (min)**: `target_spread_bps_min` (default 2 bps so quotes land inside the book and you get fills). With 2 bps, gross edge ≈ 1 bps per fill; maker fee (e.g. 10 bps) is higher, so **net_edge_bps is negative** and P&L per fill is negative. Raise to ~20–40 bps so that gross &gt; fee and net is positive (fewer fills).
- **Max quote size**: `max_quote_size` (e.g. 0.01 BTC per order)
- **Max position**: `max_position_btc` (long/short limit)
- **Spread alpha**: `spread_alpha` — scales latency/volatility/staleness add-on to spread

Fair price is computed in `src/fair_price.rs` (weighted mid, staleness, latency); volatility is a rolling std of fair mid in bps.

### Arb mode (code)

- Symbol: `BTCUSDT` in engine/feeds
- Min gross profit: `detector.set_min_profit_bps(25.0)` so that after per-exchange taker fees net ≥ 5 bps for low-fee pairs
- Risk: `risk_manager.set_risk_limits(max_trade_btc, min_profit_bps)`; fees per exchange in `src/exchange_fees.rs`

## Source layout

- `src/core.rs`: market types, order book, arbitrage detector (Arb mode)
- `src/feeds.rs`: exchange WebSocket feeds + `ExchangeManager`
- `src/fair_price.rs`: per-exchange quote snapshots, fair price, volatility, staleness
- `src/position.rs`: position and inventory limits for MM
- `src/risk.rs`: risk manager (arb opportunities + MM quote/fill assessment)
- `src/engine.rs`: engine loop, mode dispatch, CSV logging, stats
- `src/main.rs`: entrypoint, `--mode` parsing, Ctrl+C shutdown
- `src/ws/`: WebSocket clients per exchange
