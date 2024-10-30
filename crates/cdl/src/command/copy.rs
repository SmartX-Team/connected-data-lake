use anyhow::Result;
use cdl_catalog::DatasetCatalog;
use cdl_fs::GlobalPath;
use clap::Parser;
use tracing::instrument;

/// Copy the dataset's data into the other's specific directory
///
#[derive(Clone, Debug, PartialEq, Parser)]
pub struct CopyArgs {
    pub from: GlobalPath,
    pub to: GlobalPath,
}

impl CopyArgs {
    #[instrument(skip_all)]
    pub(super) async fn execute(self, catalog: DatasetCatalog) -> Result<()> {
        self.from.copy_all(&catalog, &self.to).await
    }
}
