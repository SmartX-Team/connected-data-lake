use anyhow::Result;
use cdl_catalog::DatasetCatalog;
use cdl_fs::register_handlers;
use clap::Parser;

#[derive(Clone, Debug, PartialEq, Parser)]
pub struct Args {
    #[command(flatten)]
    pub catalog: DatasetCatalog,

    #[command(subcommand)]
    pub command: crate::command::Command,
}

impl Args {
    pub(super) async fn execute(self) -> Result<()> {
        register_handlers();
        self.command.execute(self.catalog).await
    }
}
