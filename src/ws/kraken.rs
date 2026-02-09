use crate::core::MarketUpdate;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub fn start_kraken_ws(symbol: String, tx: Sender<MarketUpdate>, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("Failed to create Tokio runtime for Kraken WS: {e}");
                return;
            }
        };
        rt.block_on(async move {
            run_ws_loop(symbol, tx, running).await;
        });
    });
}

async fn run_ws_loop(symbol: String, tx: Sender<MarketUpdate>, running: Arc<AtomicBool>) {
    let url = "wss://ws.kraken.com";
    let exchange = "kraken".to_string();

    // Kraken uses pairs like "XBT/USDT" or "XBT/USD"
    let pair = if symbol.eq_ignore_ascii_case("BTCUSDT") {
        "XBT/USDT"
    } else {
        "XBT/USD"
    };

    while running.load(Ordering::SeqCst) {
        println!("Connecting Kraken WS: {url} pair={pair}");
        match connect_async(url).await {
            Ok((mut ws_stream, _)) => {
                println!("Kraken WS connected for pair {}", pair);

                let sub = serde_json::json!({
                    "event": "subscribe",
                    "pair": [pair],
                    "subscription": { "name": "spread" }
                });

                if ws_stream
                    .send(Message::Text(sub.to_string()))
                    .await
                    .is_err()
                {
                    eprintln!("Kraken WS failed to send subscribe");
                    continue;
                }

                while running.load(Ordering::SeqCst) {
                    let msg: Message = match ws_stream.next().await {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            eprintln!("Kraken WS error: {e}");
                            break;
                        }
                        None => break,
                    };

                    if !msg.is_text() {
                        continue;
                    }

                    if let Some((bid, ask)) = parse_spread(msg.to_text().unwrap_or("")) {
                        // println!("Kraken WS tick {}: bid={} ask={}", pair, bid, ask);
                        let _ = tx.send(MarketUpdate::bid(&symbol, &exchange, bid, 1.0));
                        let _ = tx.send(MarketUpdate::ask(&symbol, &exchange, ask, 1.0));
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to connect Kraken WS: {e}");
            }
        }

        if !running.load(Ordering::SeqCst) {
            break;
        }
        sleep(Duration::from_secs(3)).await;
    }
}

// Kraken spread data: [bid, ask, timestamp, bid_volume, ask_volume] — indices 0 and 1 are prices.
fn parse_spread(raw: &str) -> Option<(f64, f64)> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    if v.is_object() {
        return None;
    }
    let arr = v.as_array()?;
    if arr.len() < 2 {
        return None;
    }
    let spread = &arr[1];
    let spread_arr = spread.as_array()?;
    if spread_arr.len() < 2 {
        return None;
    }
    let bid_str = spread_arr[0].as_str()?;
    let ask_str = spread_arr[1].as_str()?;
    let bid = bid_str.parse().ok()?;
    let ask = ask_str.parse().ok()?;
    Some((bid, ask))
}

