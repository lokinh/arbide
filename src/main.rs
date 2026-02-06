use arbide::engine::Engine;
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

    println!("⚡ Arbide Ultra-Fast Initialization...");
    println!("⚡ Rust - Zero external dependencies for core engine");
    println!("⚡ Build: release recommended for best performance\n");

    let engine = Engine::new();
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
