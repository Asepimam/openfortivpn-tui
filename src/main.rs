mod app;
mod actions;
mod config;
mod runtime;
mod ui;
mod vpn;

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use actions::{apply_initial_config, save_all_config};
use app::App;
use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let debug_enabled = std::env::args().any(|arg| arg == "-d" || arg == "--debug");
    setup_logging(debug_enabled)?;

    let cfg = Config::load().unwrap_or_default();
    let mut app = App::new(debug_enabled);
    apply_initial_config(&mut app, &cfg);

    let mut terminal = runtime::init_terminal()?;
    let result = runtime::run(&mut terminal, &mut app).await;
    save_all_config(&app).ok();
    runtime::restore_terminal(&mut terminal)?;
    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }
    Ok(())
}

fn setup_logging(debug_enabled: bool) -> Result<()> {
    if !debug_enabled {
        return Ok(());
    }

    use std::fs::OpenOptions;
    let log_file = std::env::temp_dir().join("openfortivpn-tui.log");
    let file = OpenOptions::new().create(true).append(true).open(&log_file)?;
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false);
    tracing_subscriber::registry().with(file_layer).init();
    Ok(())
}
