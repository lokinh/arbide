use crate::core::MarketUpdate;
use futures_util::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub fn start_binance_ws(symbol: String, tx: Sender<MarketUpdate>, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("Failed to create Tokio runtime for Binance WS: {e}");
                return;
            }
        };
        rt.block_on(async move {
            run_ws_loop(symbol, tx, running).await;
        });
    });
}

async fn run_ws_loop(symbol: String, tx: Sender<MarketUpdate>, running: Arc<AtomicBool>) {
    let symbol_lower = symbol.to_lowercase();
    let stream_name = format!("{symbol_lower}@bookTicker");
    let url = format!("wss://stream.binance.com/stream?streams={}", stream_name);
    let exchange = "binance".to_string();

    while running.load(Ordering::SeqCst) {
        println!("Connecting Binance WS: {url}");
        match connect_async(&url).await {
            Ok((mut ws_stream, _)) => {
                println!("Binance WS connected for symbol {symbol}");
                while running.load(Ordering::SeqCst) {
                    let msg: Message = match ws_stream.next().await {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            eprintln!("Binance WS error: {e}");
                            break;
                        }
                        None => break,
                    };

                    if !msg.is_text() {
                        continue;
                    }

                    if let Some((bid, ask)) = parse_book_ticker(msg.to_text().unwrap_or("")) {
                        // println!(
                        //     "Binance WS tick {}: bid={} ask={}",
                        //     symbol, bid, ask
                        // );
                        let _ = tx.send(MarketUpdate::bid(&symbol, &exchange, bid, 1.0));
                        let _ = tx.send(MarketUpdate::ask(&symbol, &exchange, ask, 1.0));
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to connect Binance WS: {e}");
            }
        }

        if !running.load(Ordering::SeqCst) {
            break;
        }
        sleep(Duration::from_secs(3)).await;
    }
}

fn parse_book_ticker(raw: &str) -> Option<(f64, f64)> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let data = v.get("data")?;
    let bid_str = data.get("b")?.as_str()?;
    let ask_str = data.get("a")?.as_str()?;
    let bid = bid_str.parse().ok()?;
    let ask = ask_str.parse().ok()?;
    Some((bid, ask))
}

