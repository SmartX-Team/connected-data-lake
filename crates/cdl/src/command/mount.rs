use std::path::PathBuf;

use anyhow::Result;
use cdl_catalog::DatasetCatalog;
use cdl_fs::GlobalPath;
use cdl_fuse::FileMount;
use clap::Parser;
use tracing::instrument;

/// Mount the dataset into the specified local directory
///
#[derive(Clone, Debug, PartialEq, Parser)]
pub struct MountArgs {
    pub from: GlobalPath,
    pub to: PathBuf,
}

impl MountArgs {
    #[instrument(skip_all)]
    pub(super) async fn execute(self, catalog: DatasetCatalog) -> Result<()> {
        self.from.mount_to(catalog, &self.to).await
    }
}
