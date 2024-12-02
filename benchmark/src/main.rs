mod args;
mod command;
mod ins;

use anyhow::Result;
use clap::Parser;
use tracing::{debug, error, info};

#[::tokio::main]
async fn main() {
    let args = self::args::Args::parse();

    ::cdl_k8s_core::otel::init_once();
    info!("Welcome to Connected Data Lake Benchmark!");

    match try_main(args).await {
        Ok(()) => info!("Done"),
        Err(error) => error!("{error}"),
    }
}

async fn try_main(args: self::args::Args) -> Result<()> {
    debug!("Starting Connected Data Lake CLI Benchmark");
    args.execute().await
}
