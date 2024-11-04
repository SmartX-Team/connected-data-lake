use std::{
    collections::BTreeSet, fmt, ops::Range, path::PathBuf, str::FromStr, sync::Arc,
    time::SystemTime,
};

use async_trait::async_trait;
use bytes::Bytes;
use cdl_catalog::DatasetCatalog;
use deltalake::{
    aws::{storage::S3ObjectStoreFactory, S3LogStoreFactory},
    logstore::logstores,
    storage::{
        factories,
        file::FileStorageBackend,
        object_store::{
            GetOptions, GetRange, MultipartUpload, PutMultipartOpts, PutOptions, PutPayload,
            PutResult,
        },
        GetResult, ListResult, ObjectStoreFactory, ObjectStoreRef, ObjectStoreResult,
        StorageOptions,
    },
    DeltaResult, DeltaTableError, ObjectMeta, ObjectStore, ObjectStoreError, Path,
};
use futures::{
    stream::{self, BoxStream},
    StreamExt, TryStreamExt,
};
use glob::{glob, PatternError};
use tokio::fs;
use tracing::info;
use url::Url;

const NAME: &str = "CachedStorage";

/// Register an [ObjectStoreFactory]
pub fn register_handlers() {
    info!("Registering store: S3");
    let object_stores = Arc::new(CachedObjectStoreFactory::default());
    let log_stores = Arc::new(S3LogStoreFactory::default());
    for scheme in ["s3", "s3a"].iter() {
        let url = Url::parse(&format!("{}://", scheme)).unwrap();
        factories().insert(url.clone(), object_stores.clone());
        logstores().insert(url.clone(), log_stores.clone());
    }
}

#[derive(Clone, Debug, Default)]
pub struct CachedObjectStoreFactory {
    backend: S3ObjectStoreFactory,
}

impl ObjectStoreFactory for CachedObjectStoreFactory {
    fn parse_url_opts(
        &self,
        url: &Url,
        options: &StorageOptions,
    ) -> DeltaResult<(ObjectStoreRef, Path)> {
        self.backend
            .parse_url_opts(url, options)
            .and_then(|(backend, path)| {
                match CachedObjectStoreBackend::load_local(backend, options)? {
                    Ok(cached) => Ok((Arc::new(cached) as ObjectStoreRef, path)),
                    Err(backend) => Ok((backend, path)),
                }
            })
    }
}

pub struct CachedObjectStoreBackend {
    backend: ObjectStoreRef,
    cache: ObjectStoreRef,
    cache_dir: String,
    threshold_object_size: usize,
    threshold_total_size: u64,
}

impl CachedObjectStoreBackend {
    pub fn load_local(
        backend: ObjectStoreRef,
        options: &StorageOptions,
    ) -> DeltaResult<Result<Self, ObjectStoreRef>> {
        fn parse_key<T>(options: &StorageOptions, key: &str) -> DeltaResult<Option<T>>
        where
            T: FromStr,
        {
            options
                .0
                .get(key)
                .map(|value| {
                    value
                        .parse()
                        .map_err(|_| DeltaTableError::generic(format!("Failed to parse {key}")))
                })
                .transpose()
        }

        let cache_dir = options
            .0
            .get(DatasetCatalog::KEY_CACHE_DIR)
            .cloned()
            .unwrap_or_else(DatasetCatalog::default_cache_dir);
        let threshold_total_size = parse_key(options, DatasetCatalog::KEY_MAX_CACHE_SIZE)?
            .unwrap_or(DatasetCatalog::default_max_cache_size());
        let threshold_object_size = parse_key(options, DatasetCatalog::KEY_MIN_CACHE_OBJECT_SIZE)?
            .unwrap_or(DatasetCatalog::default_min_cache_object_size());

        if threshold_total_size > 0 {
            Ok(Ok(Self {
                backend,
                cache: {
                    ::std::fs::create_dir_all(&cache_dir)?;
                    Arc::new(FileStorageBackend::try_new(&cache_dir)?)
                },
                cache_dir,
                threshold_object_size,
                threshold_total_size,
            }))
        } else {
            Ok(Err(backend))
        }
    }
}

impl fmt::Debug for CachedObjectStoreBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { backend, cache, .. } = self;
        write!(
            f,
            "CachedObjectStoreBackend {{ backend: {backend:?}, cache: {cache:?} }}",
        )
    }
}

impl fmt::Display for CachedObjectStoreBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { backend, cache, .. } = self;
        write!(
            f,
            "CachedObjectStoreBackend {{ backend: {backend}, cache: {cache} }}",
        )
    }
}

#[async_trait]
impl ObjectStore for CachedObjectStoreBackend {
    async fn put(&self, location: &Path, payload: PutPayload) -> ObjectStoreResult<PutResult> {
        self.backend.put(location, payload).await
    }

    async fn put_opts(
        &self,
        location: &Path,
        payload: PutPayload,
        opts: PutOptions,
    ) -> ObjectStoreResult<PutResult> {
        self.backend.put_opts(location, payload, opts).await
    }

    async fn put_multipart(&self, location: &Path) -> ObjectStoreResult<Box<dyn MultipartUpload>> {
        self.backend.put_multipart(location).await
    }

    async fn put_multipart_opts(
        &self,
        location: &Path,
        opts: PutMultipartOpts,
    ) -> ObjectStoreResult<Box<dyn MultipartUpload>> {
        self.backend.put_multipart_opts(location, opts).await
    }

    async fn get_opts(&self, location: &Path, options: GetOptions) -> ObjectStoreResult<GetResult> {
        let clone_options = || GetOptions {
            if_match: options.if_match.clone(),
            if_none_match: options.if_none_match.clone(),
            if_modified_since: options.if_modified_since.clone(),
            if_unmodified_since: options.if_unmodified_since.clone(),
            range: options.range.clone(),
            version: options.version.clone(),
            head: options.head.clone(),
        };

        match self.cache.get_opts(location, clone_options()).await {
            Ok(result) => Ok(result),
            Err(ObjectStoreError::NotFound { .. }) => {
                let requested_size = if location
                    .filename()
                    .map(|filename| filename.ends_with(".parquet"))
                    .unwrap_or_default()
                {
                    match options.range {
                        Some(GetRange::Bounded(Range { start, end })) => {
                            if end >= start {
                                end - start
                            } else {
                                0
                            }
                        }
                        Some(_) | None => usize::MAX,
                    }
                } else {
                    usize::MAX
                };

                if self.threshold_object_size <= requested_size {
                    self.store(location).await?;
                    self.cache.get_opts(location, options).await
                } else {
                    self.backend.get_opts(location, options).await
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn get_ranges(
        &self,
        location: &Path,
        ranges: &[Range<usize>],
    ) -> ObjectStoreResult<Vec<Bytes>> {
        match self.cache.get_ranges(location, ranges).await {
            Ok(result) => Ok(result),
            Err(ObjectStoreError::NotFound { .. }) => {
                let requested_size = ranges
                    .into_iter()
                    .filter(|Range { start, end }| end >= start)
                    .map(|Range { start, end }| end - start)
                    .sum::<usize>();

                if self.threshold_object_size <= requested_size {
                    self.store(location).await?;
                    self.cache.get_ranges(location, ranges).await
                } else {
                    self.backend.get_ranges(location, ranges).await
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn head(&self, location: &Path) -> ObjectStoreResult<ObjectMeta> {
        match self.cache.head(location).await {
            Ok(result) => Ok(result),
            Err(ObjectStoreError::NotFound { .. }) => self.backend.head(location).await,
            Err(error) => Err(error),
        }
    }

    async fn delete(&self, location: &Path) -> ObjectStoreResult<()> {
        self.backend.delete(location).await
    }

    fn delete_stream<'a>(
        &'a self,
        locations: BoxStream<'a, ObjectStoreResult<Path>>,
    ) -> BoxStream<'a, ObjectStoreResult<Path>> {
        self.backend.delete_stream(locations)
    }

    fn list(&self, prefix: Option<&Path>) -> BoxStream<'_, ObjectStoreResult<ObjectMeta>> {
        self.backend.list(prefix)
    }

    fn list_with_offset(
        &self,
        prefix: Option<&Path>,
        offset: &Path,
    ) -> BoxStream<'_, ObjectStoreResult<ObjectMeta>> {
        self.backend.list_with_offset(prefix, offset)
    }

    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> ObjectStoreResult<ListResult> {
        self.backend.list_with_delimiter(prefix).await
    }

    async fn copy(&self, from: &Path, to: &Path) -> ObjectStoreResult<()> {
        self.backend.copy(from, to).await
    }

    async fn rename(&self, from: &Path, to: &Path) -> ObjectStoreResult<()> {
        self.backend.rename(from, to).await
    }

    async fn copy_if_not_exists(&self, from: &Path, to: &Path) -> ObjectStoreResult<()> {
        self.backend.copy_if_not_exists(from, to).await
    }

    async fn rename_if_not_exists(&self, from: &Path, to: &Path) -> ObjectStoreResult<()> {
        self.backend.rename_if_not_exists(from, to).await
    }
}

impl CachedObjectStoreBackend {
    async fn shrink(&self) -> ObjectStoreResult<()> {
        #[derive(PartialEq, Eq, PartialOrd, Ord)]
        struct CachedFileMetadata {
            is_large: bool,
            accessed: SystemTime,
            len: u64,
            path: PathBuf,
        }

        let pattern = format!("{}/**/*", &self.cache_dir);
        let list =
            glob(&pattern).map_err(|PatternError { pos: _, msg }| ObjectStoreError::Generic {
                store: NAME,
                source: msg.into(),
            })?;
        let mut metadata: BTreeSet<_> = stream::iter(list.filter_map(|path| path.ok()))
            .then(|path| {
                let threshold_object_size = self.threshold_object_size as _;
                async move {
                    let metadata = fs::metadata(&path).await?;
                    let accessed = metadata.accessed()?;
                    let len = metadata.len();
                    let is_large = len >= threshold_object_size;
                    Ok(CachedFileMetadata {
                        is_large,
                        accessed,
                        len,
                        path,
                    })
                }
            })
            .try_collect()
            .await
            .map_err(convert_std_io_err)?;

        let mut total_size = metadata.iter().map(|m| m.len).sum::<u64>();
        while total_size > self.threshold_total_size {
            let CachedFileMetadata { len, path, .. } = metadata.pop_last().unwrap();
            total_size -= len;
            fs::remove_file(path).await.map_err(convert_std_io_err)?;
        }
        Ok(())
    }

    async fn store(&self, location: &Path) -> ObjectStoreResult<PutResult> {
        self.shrink().await?;

        let payload = self.backend.get(location).await?.bytes().await?;
        self.cache.put(location, payload.into()).await
    }
}

#[inline]
fn convert_std_io_err(source: ::std::io::Error) -> ObjectStoreError {
    ObjectStoreError::Generic {
        store: NAME,
        source: Box::new(source),
    }
}
