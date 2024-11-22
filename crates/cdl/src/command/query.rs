use anyhow::Result;
use cdl_catalog::DatasetCatalog;
use cdl_fs::GlobalPath;
use clap::Parser;
use tracing::instrument;

/// Query the given SQL into the specific dataset
///
#[derive(Clone, Debug, PartialEq, Parser)]
pub struct QueryArgs {
    pub target: GlobalPath,
    pub sql: String,
}

impl QueryArgs {
    #[instrument(skip_all)]
    pub(super) async fn execute(self, catalog: DatasetCatalog) -> Result<()> {
        let fs = self.target.open(catalog).await?;
        let df = fs.query(&self.sql).await?;
        df.show_limit(10).await?;
        Ok(())
    }
}
