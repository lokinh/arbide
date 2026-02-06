# Arbide — Real-Time Arbitrage Detection (Simulator)

`arbide` is a small Rust project that simulates multiple exchange feeds, maintains simplified order books, detects cross-exchange arbitrage opportunities using best bid/ask, applies basic risk checks, and logs results to CSV.

## Features

- Multi-exchange feed simulator: `binance`, `coinbase`, `kraken`, `bybit`
- Simplified order book (top levels) with best bid/ask
- Cross-exchange arbitrage detection with a configurable minimum profit (bps)
- Basic risk checks (fee-adjusted profit threshold + max trade size)
- CSV output for downstream tooling

## Requirements

- Rust toolchain + Cargo (stable)

## Run

From the `arbide` directory:

```bash
cargo run --release
```

Stop safely with `Ctrl+C`.

## Outputs

- `arbitrage_opportunities.csv`
  - Header: `timestamp,symbol,buy_exchange,sell_exchange,buy_price,sell_price,profit_bps,net_profit_bps,latency_ns,decision`
- `session_summary.txt`

## Quick configuration (code)

### Symbol (default: `BTCUSDT`)

Edit `src/engine.rs` and search for `BTCUSDT`.

### Minimum profit (bps)

Edit `src/engine.rs`:

- `detector.set_min_profit_bps(5.0);`

### Risk limits

Edit `src/engine.rs`:

- `risk_manager.set_risk_limits(10.0, -5.0);`
  - Param 1: max trade size (BTC)
  - Param 2: min net profit after fees (bps)

## Source layout

- `src/core.rs`: market types, order book, arbitrage detector
- `src/feeds.rs`: exchange feed simulators + `ExchangeManager`
- `src/risk.rs`: simple risk manager
- `src/engine.rs`: engine loop + CSV logging + stats
- `src/main.rs`: entrypoint + Ctrl+C shutdown

