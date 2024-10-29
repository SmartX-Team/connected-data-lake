use anyhow::Result;
use cdl_catalog::DatasetCatalog;
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
        self.catalog.init();
        self.command.execute(self.catalog).await
    }
}
