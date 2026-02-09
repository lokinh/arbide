use arbide::engine::{Engine, RunMode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

fn main() {
    ctrlc::set_handler(move || {
        println!("\n🛑 Received Ctrl+C. Initiating graceful shutdown...");
        SHUTDOWN.store(true, Ordering::SeqCst);
    })
    .expect("set Ctrl+C handler");

    let mode = std::env::args()
        .find(|a| a == "--mode" || a == "-m")
        .and_then(|_| std::env::args().nth(2))
        .and_then(|s| match s.as_str() {
            "arb" => Some(RunMode::Arb),
            "mm" => Some(RunMode::Mm),
            _ => None,
        })
        .unwrap_or(RunMode::Mm);

    println!("⚡ Arbide Ultra-Fast Initialization...");
    println!(
        "⚡ Mode: {}",
        match mode {
            RunMode::Arb => "arb (cross-exchange arbitrage detection)",
            RunMode::Mm => "mm (market-making on primary exchange)",
        }
    );
    println!("⚡ Build: release recommended for best performance\n");

    let engine = Engine::new_with_mode(mode);
    let running = Arc::clone(&engine.running);

    let engine_handle = thread::spawn(move || {
        engine.run();
    });

    while !SHUTDOWN.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(100));
    }

    println!("\n🛑 Shutting down Arbide Engine...");
    running.store(false, Ordering::SeqCst);
    let _ = engine_handle.join();

    println!("✅ Arbide stopped.");
}
