use crate::core::MarketUpdate;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub fn start_bybit_ws(symbol: String, tx: Sender<MarketUpdate>, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("Failed to create Tokio runtime for Bybit WS: {e}");
                return;
            }
        };
        rt.block_on(async move {
            run_ws_loop(symbol, tx, running).await;
        });
    });
}

async fn run_ws_loop(symbol: String, tx: Sender<MarketUpdate>, running: Arc<AtomicBool>) {
    let url = "wss://stream.bybit.com/v5/public/linear";
    let exchange = "bybit".to_string();
    let topic = format!("tickers.{}", symbol);

    while running.load(Ordering::SeqCst) {
        println!("Connecting Bybit WS: {url} topic={topic}");
        match connect_async(url).await {
            Ok((mut ws_stream, _)) => {
                println!("Bybit WS connected for symbol {}", symbol);
                let sub = serde_json::json!({
                    "op": "subscribe",
                    "args": [topic]
                });
                if ws_stream
                    .send(Message::Text(sub.to_string()))
                    .await
                    .is_err()
                {
                    eprintln!("Bybit WS failed to send subscribe");
                    continue;
                }

                while running.load(Ordering::SeqCst) {
                    let msg: Message = match ws_stream.next().await {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            eprintln!("Bybit WS error: {e}");
                            break;
                        }
                        None => break,
                    };

                    if !msg.is_text() {
                        continue;
                    }

                    if let Some((bid, ask)) = parse_ticker(msg.to_text().unwrap_or("")) {
                        // println!("Bybit WS tick {}: bid={} ask={}", symbol, bid, ask);
                        let _ = tx.send(MarketUpdate::bid(&symbol, &exchange, bid, 1.0));
                        let _ = tx.send(MarketUpdate::ask(&symbol, &exchange, ask, 1.0));
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to connect Bybit WS: {e}");
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
    let data = v.get("data")?;
    let arr = data.as_array()?;
    let first = arr.get(0)?;
    let bid_str = first.get("bid1Price")?.as_str()?;
    let ask_str = first.get("ask1Price")?.as_str()?;
    let bid = bid_str.parse().ok()?;
    let ask = ask_str.parse().ok()?;
    Some((bid, ask))
}

