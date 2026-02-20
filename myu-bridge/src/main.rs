use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use myu_bridge::config;
use myu_bridge::controller::Controller;
use myu_bridge::http::MigoyugoHttpClient;
use myu_bridge::socket::{BridgeEvent, MigoyugoSocketClient};
use tokio::sync::mpsc;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the KDL configuration file
    #[arg(short, long, default_value = "bridge_config.kdl")]
    config: String,

    /// Base URL for Migoyugo (useful for local testing)
    #[arg(long, default_value = "https://migoyugo-back-end.onrender.com")]
    url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup tracing
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber).context("Failed to set tracing subscriber")?;

    let args = Args::parse();

    let email = std::env::var("MIGOYUGO_EMAIL").context("MIGOYUGO_EMAIL environment variable is required")?;
    let password = std::env::var("MIGOYUGO_PASSWORD").context("MIGOYUGO_PASSWORD environment variable is required")?;

    let config_path = std::path::Path::new(&args.config);
    let bridge_config = config::parse_config(config_path)?;
    tracing::info!("Loaded {} config rules", bridge_config.accept_rules.len());

    let mut http_client = MigoyugoHttpClient::new(&args.url);
    http_client.login(&email, &password).await?;

    let token = http_client.get_token().context("Token must be present after successful login")?.to_string();

    let (event_tx, event_rx) = mpsc::channel::<BridgeEvent>(100);

    let socket_client = MigoyugoSocketClient::connect(&args.url, &token, event_tx).await?;

    let mut controller =
        Controller::new(Arc::new(http_client), Arc::new(socket_client), bridge_config.accept_rules, event_rx);

    tracing::info!("Myu-Bridge started and connected to {}", args.url);

    // Run the main event loop
    controller.run().await?;

    Ok(())
}
