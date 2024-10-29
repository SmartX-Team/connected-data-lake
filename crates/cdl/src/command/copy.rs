use anyhow::Result;
use cdl_catalog::DatasetCatalog;
use cdl_fs::GlobalPath;
use clap::Parser;
use tracing::{instrument, Level};

#[derive(Clone, Debug, PartialEq, Parser)]
pub struct CopyArgs {
    pub from: GlobalPath,
    pub to: GlobalPath,
}

impl CopyArgs {
    #[instrument(skip_all, err(level = Level::ERROR))]
    pub(super) async fn execute(self, catalog: DatasetCatalog) -> Result<()> {
        self.from.copy_all(&catalog, &self.to).await
    }
}
