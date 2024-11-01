mod args;
mod command;

use anyhow::Result;
use clap::Parser;
use tracing::{debug, error, info};

#[::tokio::main]
async fn main() {
    let args = self::args::Args::parse();

    ::ark_core::tracer::init_once();
    info!("Welcome to Connected Data Lake!");

    match try_main(args).await {
        Ok(()) => info!("Done"),
        Err(error) => error!("{error}"),
    }
}

async fn try_main(args: self::args::Args) -> Result<()> {
    debug!("Starting Connected Data Lake CLI");
    args.execute().await
}
