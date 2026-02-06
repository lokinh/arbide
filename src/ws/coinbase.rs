use crate::core::MarketUpdate;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub fn start_coinbase_ws(symbol: String, tx: Sender<MarketUpdate>, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("Failed to create Tokio runtime for Coinbase WS: {e}");
                return;
            }
        };
        rt.block_on(async move {
            run_ws_loop(symbol, tx, running).await;
        });
    });
}

async fn run_ws_loop(symbol: String, tx: Sender<MarketUpdate>, running: Arc<AtomicBool>) {
    // Coinbase Advanced Trade / public market data feed
    let url = "wss://advanced-trade-ws.coinbase.com";
    let exchange = "coinbase".to_string();

    // Coinbase uses product_id like "BTC-USD"
    // For BTCUSDT we approximate with BTC-USDT if available; otherwise BTC-USD
    let product_id = if symbol.eq_ignore_ascii_case("BTCUSDT") {
        "BTC-USDT"
    } else {
        "BTC-USD"
    };

    while running.load(Ordering::SeqCst) {
        println!("Connecting Coinbase WS: {url} product_id={product_id}");
        match connect_async(url).await {
            Ok((mut ws_stream, _)) => {
                println!("Coinbase WS connected for product {}", product_id);

                let sub = serde_json::json!({
                    "type": "subscribe",
                    "product_ids": [product_id],
                    "channel": "ticker"
                });

                if ws_stream
                    .send(Message::Text(sub.to_string()))
                    .await
                    .is_err()
                {
                    eprintln!("Coinbase WS failed to send subscribe");
                    continue;
                }

                while running.load(Ordering::SeqCst) {
                    let msg: Message = match ws_stream.next().await {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            eprintln!("Coinbase WS error: {e}");
                            break;
                        }
                        None => break,
                    };

                    if !msg.is_text() {
                        continue;
                    }

                    if let Some((bid, ask)) = parse_ticker(msg.to_text().unwrap_or("")) {
                        // println!("Coinbase WS tick {}: bid={} ask={}", product_id, bid, ask);
                        let _ = tx.send(MarketUpdate::bid(&symbol, &exchange, bid, 1.0));
                        let _ = tx.send(MarketUpdate::ask(&symbol, &exchange, ask, 1.0));
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to connect Coinbase WS: {e}");
            }
        }

        if !running.load(Ordering::SeqCst) {
            break;
        }
        sleep(Duration::from_secs(3)).await;
    }
}

fn parse_ticker(raw: &str) -> Option<(f64, f64)> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    if v.get("type")?.as_str()? != "ticker" {
        return None;
    }
    let bid_str = v.get("best_bid")?.as_str()?;
    let ask_str = v.get("best_ask")?.as_str()?;
    let bid = bid_str.parse().ok()?;
    let ask = ask_str.parse().ok()?;
    Some((bid, ask))
}

