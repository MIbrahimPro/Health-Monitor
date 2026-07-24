use aegis_core::camera::start_camera_loop;
use anyhow::Result;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting Aegis Daemon...");

    let (tx, mut rx) = mpsc::channel(100);

    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    // Start the camera loop in the background
    start_camera_loop(tx, Arc::new(AtomicBool::new(false)))?;

    // Listen for vital stats updates
    println!("Listening for vital stats...");
    while let Some(stats) = rx.recv().await {
        println!("Received raw pulse: {:.4}", stats.raw_pulse);
    }

    Ok(())
}
