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
        let fs = self.from.open(catalog).await?;
        fs.copy_to(&self.to).await
    }
}
