use anyhow::Result;
use clap::Parser;
use lob_engine::cli::{run, Cli};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .flatten_event(true)
        .init();

    let cli = Cli::parse();
    run(cli).await
}
