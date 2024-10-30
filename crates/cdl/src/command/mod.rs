pub mod copy;
pub mod mount;

use anyhow::Result;
use cdl_catalog::DatasetCatalog;
use clap::Subcommand;

#[derive(Clone, Debug, PartialEq, Subcommand)]
pub enum Command {
    Cp(self::copy::CopyArgs),
    Mount(self::mount::MountArgs),
}

impl Command {
    pub(super) async fn execute(self, catalog: DatasetCatalog) -> Result<()> {
        match self {
            Self::Cp(args) => args.execute(catalog).await,
            Self::Mount(args) => args.execute(catalog).await,
        }
    }
}
