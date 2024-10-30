mod fs;

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use cdl_catalog::DatasetCatalog;
use cdl_fs::GlobalPath;
use fuser::{mount2, MountOption};
use tracing::instrument;

use self::fs::CdlFS;

#[async_trait]
pub trait FileMount {
    async fn mount_to(self, catalog: DatasetCatalog, path: &Path) -> Result<()>;
}

#[async_trait]
impl FileMount for GlobalPath {
    #[instrument(skip_all)]
    async fn mount_to(self, catalog: DatasetCatalog, path: &Path) -> Result<()> {
        let mountpoint = ::tokio::fs::canonicalize(path).await?;
        let options = vec![
            // MountOption::AllowRoot,
            // MountOption::AutoUnmount,
            MountOption::FSName(CdlFS::NAME.into()),
            MountOption::RO,
        ];

        let fs = CdlFS::load(catalog, self).await?;
        mount2(fs, mountpoint, &options)?;
        todo!()
    }
}
