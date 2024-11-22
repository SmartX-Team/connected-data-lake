mod functions;

use core::fmt;
use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Error, Result};
use arrow::{
    array::{self, ArrayBuilder, ArrayRef, AsArray, RecordBatch},
    datatypes::{
        DataType as ArrowDataType, Field as ArrowField, Schema as ArrowSchema, SchemaRef,
        TimeUnit as ArrowTimeUnit,
    },
};
use cdl_catalog::DatasetCatalog;
use cdl_store::build_registry;
pub use cdl_store::CachedObjectStoreProvider;
use chrono::{DateTime, Timelike, Utc};
use datafusion::{
    error::DataFusionError,
    execution::SendableRecordBatchStream,
    physical_plan::stream::RecordBatchStreamAdapter,
    prelude::{DataFrame, SessionConfig, SessionContext},
};
use filetime::FileTime;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use glob::glob;
use itertools::Itertools;
use lance::{
    dataset::{builder::DatasetBuilder, InsertBuilder, WriteDestination, WriteMode, WriteParams},
    Dataset, Error as LanceError,
};
use lance_encoding::version::LanceFileVersion;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use strum::Display;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    spawn,
    sync::mpsc,
};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, instrument, Level};

pub struct CdlFS {
    catalog: DatasetCatalog,
    ctx: SessionContext,
    path: GlobalPath,
}

impl CdlFS {
    #[inline]
    pub fn path(&self) -> String {
        self.path.to_string()
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    pub async fn copy_to(&self, dst: &GlobalPath) -> Result<()> {
        let stream = self.load_all().await?;
        dst.dump_all(&self.catalog, stream).await
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    pub async fn query(&self, sql: &str) -> Result<DataFrame> {
        self.ctx().await?.sql(sql).await.map_err(Into::into)
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    pub async fn read_dir(&self, path: impl AsRef<Path>) -> Result<SendableRecordBatchStream> {
        let parent = trim_rel_suffix(path.as_ref().to_str().context("Invalid path")?);
        let condition =
            format!("WHERE parent LIKE '{parent}' AND size IS NOT NULL ORDER BY name ASC");
        self.list_by(&condition).await
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    pub async fn read_dir_all(&self) -> Result<SendableRecordBatchStream> {
        let condition = "WHERE size IS NOT NULL ORDER BY parent ASC, name ASC";
        self.list_by(condition).await
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    pub async fn read_files_by_condition(
        &self,
        condition: &str,
    ) -> Result<impl '_ + Send + Stream<Item = Result<Vec<FileRecord>>>> {
        let stream = self.load_by(condition).await?;
        let file_stream = stream.map_err(Error::from).and_then(|batch| async move {
            let batch = FileRecordBatch::try_from(&batch)?;
            batch.into_vec()
        });
        Ok(file_stream)
    }
}

impl CdlFS {
    async fn ctx(&self) -> Result<&SessionContext> {
        if !self.ctx.table_exist(DIR_ROOTFS)? {
            let table = Arc::new(self.table().await?);
            self.ctx.register_table(DIR_ROOTFS, table)?;
        }
        Ok(&self.ctx)
    }

    #[inline]
    async fn list_by(&self, condition: &str) -> Result<SendableRecordBatchStream> {
        let sql = format!(
            "SELECT name, parent, atime, ctime, mtime, mode, size, chunk_id, chunk_offset, chunk_size, x'' AS data FROM {DIR_ROOTFS} {condition}"
        );
        debug!("Querying LIST: {sql}");

        let df = self.query(&sql).await?;
        df.execute_stream()
            .await
            .context("Failed to execute the dataframe")
    }

    async fn load_all(&self) -> Result<FileRecordStream> {
        let Self {
            catalog,
            ctx: _,
            path: GlobalPath { dataset, rel: root },
        } = self;

        match dataset.scheme {
            Scheme::Local => Ok(Box::pin(
                FileRecord::<Vec<u8>>::load_all(catalog.clone(), root).await?,
            )),
            Scheme::S3 => {
                let dataset = open_table(catalog, dataset).await?;
                let stream = dataset
                    .scan()
                    // TODO: filter root path
                    // .filter()
                    .scan_in_order(true)
                    .use_stats(self.catalog.enable_statistics())
                    .try_into_stream()
                    .await?
                    .map(|batch| {
                        batch
                            .map_err(Into::into)
                            .and_then(|ref batch| FileRecordBatch::try_from(batch))
                            .and_then(FileRecordBatch::into_vec)
                            .map(|records| stream::iter(records.into_iter().map(Ok)))
                    })
                    .try_flatten();
                Ok(Box::pin(stream))
            }
        }
    }

    #[inline]
    async fn load_by(&self, condition: &str) -> Result<SendableRecordBatchStream> {
        let sql = format!("SELECT * FROM {DIR_ROOTFS} WHERE {condition}");
        info!("Querying LOAD: {sql}");

        let df = self.query(&sql).await?;
        df.execute_stream()
            .await
            .context("Failed to execute the dataframe")
    }

    async fn table(&self) -> Result<Dataset> {
        let Self {
            catalog,
            ctx: _,
            path: GlobalPath { dataset, .. },
        } = self;

        match dataset.scheme {
            Scheme::Local => bail!("Local filesystem does not support CDL rootfs table"),
            Scheme::S3 => open_table(catalog, dataset).await,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub struct GlobalPath {
    pub dataset: DatasetPath,
    pub rel: PathBuf,
}

impl FromStr for GlobalPath {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let mut slice = s.split("://");
        let scheme = slice.next().unwrap();
        let next = match slice.next() {
            Some(next) => next,
            None => return Ok(Self::from_local(s.parse()?)),
        };
        let scheme = Scheme::from_str(scheme)?;

        let mut slice = next.split("/");
        let name = slice.next().unwrap().trim();
        if name.is_empty() {
            bail!("Empty dataset name: {s}")
        }

        let rel = slice.join("/").trim().parse()?;

        Ok(Self {
            dataset: DatasetPath {
                scheme,
                name: name.into(),
            },
            rel,
        })
    }
}

impl fmt::Display for GlobalPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { dataset, rel } = self;
        let rel = rel.display();
        match dataset.scheme {
            Scheme::Local => rel.fmt(f),
            _ => write!(f, "{dataset}/{rel}"),
        }
    }
}

impl GlobalPath {
    pub fn from_local(rel: PathBuf) -> Self {
        Self {
            dataset: DatasetPath::local(),
            rel,
        }
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    pub async fn open(self, catalog: DatasetCatalog) -> Result<CdlFS> {
        let mut config = SessionConfig::new();
        {
            let options = config.options_mut();
            options.execution.parquet.metadata_size_hint = Some(catalog.max_buffer_size as _);
            options.execution.parquet.pushdown_filters = true;
            options.execution.parquet.reorder_filters = true;
            options.execution.target_partitions = 1;
        }
        let ctx: SessionContext = SessionContext::new_with_config(config);
        ctx.register_udf(crate::functions::len::Udf::build());

        Ok(CdlFS {
            catalog,
            ctx,
            path: self,
        })
    }

    async fn dump_all(&self, catalog: &DatasetCatalog, stream: FileRecordStream) -> Result<()> {
        match self.dataset.scheme {
            Scheme::Local => FileRecord::dump_all(&self.rel, stream).await,
            Scheme::S3 => self.dump_all_to_s3(catalog, stream).await,
        }
    }

    async fn dump_all_to_s3(
        &self,
        catalog: &DatasetCatalog,
        stream: FileRecordStream,
    ) -> Result<()> {
        let stream = file_stream_to_batch_stream(catalog, stream).await?;
        commit_table(catalog, &self.dataset, stream).await?;
        Ok(())
        // let Writer {
        //     actions,
        //     count,
        //     inner: _,
        // } = sync::Mutex::into_inner(Arc::into_inner(writer).expect("Writer is poisoned"));
        // if !actions.is_empty() {
        //     let snapshot = table.snapshot()?;
        //     let operation = DeltaOperation::Write {
        //         mode: SaveMode::Append,
        //         partition_by: {
        //             let partition_cols = snapshot.metadata().partition_columns.clone();
        //             if !partition_cols.is_empty() {
        //                 Some(partition_cols)
        //             } else {
        //                 None
        //             }
        //         },
        //         predicate: None,
        //     };

        //     let log_store = table.log_store();
        //     let version = CommitBuilder::default()
        //         .with_actions(actions)
        //         .build(Some(snapshot), log_store, operation)
        //         .await?
        //         .version();
        //     info!("{count} files are dumped on version {version}");
        // } else {
        //     info!("No files are dumped");
        // }
        // Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub struct DatasetPath {
    pub scheme: Scheme,
    pub name: String,
}

impl fmt::Display for DatasetPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { scheme, name } = self;
        write!(f, "{scheme}://{name}")
    }
}

impl DatasetPath {
    #[inline]
    fn local() -> Self {
        DatasetPath {
            scheme: Scheme::Local,
            name: "localhost".into(),
        }
    }

    pub fn to_uri(&self, rel: &str) -> String {
        match self.scheme {
            Scheme::Local => rel.into(),
            Scheme::S3 => {
                let name = &self.name;
                let rel = trim_rel_path(rel);
                format!("s3://{name}/{rel}")
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Display, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[strum(serialize_all = "kebab-case")]
pub enum Scheme {
    Local,
    S3,
}

impl FromStr for Scheme {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "s3" | "s3a" => Ok(Self::S3),
            _ => bail!("Unknown scheme: {s:?}"),
        }
    }
}

#[derive(Debug)]
struct FileRecordBatch {
    pub name: array::StringArray,
    pub parent: array::StringArray,
    pub atime: array::TimestampMicrosecondArray,
    pub ctime: array::TimestampMicrosecondArray,
    pub mtime: array::TimestampMicrosecondArray,
    pub mode: array::UInt32Array,
    pub size: array::UInt64Array,
    pub chunk_id: array::UInt64Array,
    pub chunk_offset: array::UInt64Array,
    pub chunk_size: array::UInt64Array,
    pub data: array::BinaryArray,
}

impl TryFrom<&RecordBatch> for FileRecordBatch {
    type Error = Error;

    fn try_from(batch: &RecordBatch) -> Result<Self, Self::Error> {
        fn get_column<T>(
            batch: &RecordBatch,
            name: &str,
            cast: impl FnOnce(&ArrayRef) -> Option<&T>,
        ) -> Result<T>
        where
            T: Clone,
        {
            batch
                .column_by_name(name)
                .with_context(|| format!("No such column in the rootfs table: {name:?}"))
                .and_then(|column| {
                    cast(column).cloned().with_context(|| {
                        format!("Invalid column type in the rootfs table: {name:?}")
                    })
                })
        }

        Ok(Self {
            name: get_column(batch, "name", |c| c.as_string_opt())?,
            parent: get_column(batch, "parent", |c| c.as_string_opt())?,
            atime: get_column(batch, "atime", |c| c.as_primitive_opt())?,
            ctime: get_column(batch, "ctime", |c| c.as_primitive_opt())?,
            mtime: get_column(batch, "mtime", |c| c.as_primitive_opt())?,
            mode: get_column(batch, "mode", |c| c.as_primitive_opt())?,
            size: get_column(batch, "size", |c| c.as_primitive_opt())?,
            chunk_id: get_column(batch, "chunk_id", |c| c.as_primitive_opt())?,
            chunk_offset: get_column(batch, "chunk_offset", |c| c.as_primitive_opt())?,
            chunk_size: get_column(batch, "chunk_size", |c| c.as_primitive_opt())?,
            data: get_column(batch, "data", |c| c.as_binary_opt())?,
        })
    }
}

impl FileRecordBatch {
    fn into_vec(self) -> Result<Vec<FileRecord>> {
        let Self {
            name,
            parent,
            atime,
            ctime,
            mtime,
            mode,
            size,
            chunk_id,
            chunk_offset,
            chunk_size,
            data,
        } = self;

        let mut name = name.into_iter();
        let mut parent = parent.into_iter();
        let mut atime = atime.into_iter();
        let mut ctime = ctime.into_iter();
        let mut mtime = mtime.into_iter();
        let mut mode = mode.into_iter();
        let mut size = size.into_iter();
        let mut chunk_id = chunk_id.into_iter();
        let mut chunk_offset = chunk_offset.into_iter();
        let mut chunk_size = chunk_size.into_iter();
        let data = data.into_iter();

        let mut get_metadata = || {
            Some(FileMetadataRecord {
                atime: DateTime::from_timestamp_micros(atime.next()??)?,
                ctime: DateTime::from_timestamp_micros(ctime.next()??)?,
                mtime: DateTime::from_timestamp_micros(mtime.next()??)?,
                mode: mode.next()?? as _,
                size: size.next()?? as _,
            })
        };

        Ok(data
            .filter_map(|data| {
                Some(FileRecord {
                    name: name.next()??.into(),
                    parent: parent.next()??.into(),
                    metadata: get_metadata(),
                    chunk_id: chunk_id.next()?? as _,
                    chunk_offset: chunk_offset.next()?? as _,
                    chunk_size: chunk_size.next()?? as _,
                    data: data.unwrap_or_default().to_vec(),
                })
            })
            .collect())
    }
}

#[derive(Debug, Default)]
struct FileRecordBuilder {
    pub name: array::StringBuilder,
    pub parent: array::StringBuilder,
    pub atime: array::TimestampMicrosecondBuilder,
    pub ctime: array::TimestampMicrosecondBuilder,
    pub mtime: array::TimestampMicrosecondBuilder,
    pub mode: array::UInt32Builder,
    pub size: array::UInt64Builder,
    pub total_size: usize,
    pub chunk_id: array::UInt64Builder,
    pub chunk_offset: array::UInt64Builder,
    pub chunk_size: array::UInt64Builder,
    pub data: array::BinaryBuilder,
}

impl FileRecordBuilder {
    fn push(
        &mut self,
        catalog: &DatasetCatalog,
        schema: &SchemaRef,
        file: FileRecord,
    ) -> Result<Option<RecordBatch>> {
        let chunk_size = file.chunk_size as _;
        match self.total_size.checked_add(chunk_size) {
            Some(total_size) => {
                let batch = if total_size > catalog.max_buffer_size {
                    let batch = self.flush(schema)?;
                    self.total_size = chunk_size;
                    batch
                } else {
                    self.total_size = total_size;
                    None
                };

                self.name.append_value(file.name);
                self.parent.append_value(file.parent);
                self.atime
                    .append_option(file.metadata.as_ref().map(|m| m.atime.timestamp_micros()));
                self.ctime
                    .append_option(file.metadata.as_ref().map(|m| m.ctime.timestamp_micros()));
                self.mtime
                    .append_option(file.metadata.as_ref().map(|m| m.mtime.timestamp_micros()));
                self.mode
                    .append_option(file.metadata.as_ref().map(|m| m.mode as _));
                self.size
                    .append_option(file.metadata.as_ref().map(|m| m.size as _));
                self.chunk_id.append_value(file.chunk_id as _);
                self.chunk_offset.append_value(file.chunk_offset as _);
                self.chunk_size.append_value(file.chunk_size as _);
                self.data.append_value(file.data);
                Ok(batch)
            }
            None => bail!("File too large: {}", &file.name),
        }
    }

    fn flush(&mut self, schema: &SchemaRef) -> Result<Option<RecordBatch>> {
        if self.data.is_empty() {
            return Ok(None);
        }

        let Self {
            name,
            parent,
            atime,
            ctime,
            mtime,
            mode,
            size,
            total_size,
            chunk_id,
            chunk_offset,
            chunk_size,
            data,
        } = self;

        *total_size = 0;
        let arrow_array: Vec<ArrayRef> = vec![
            Arc::new(name.finish()),
            Arc::new(parent.finish()),
            Arc::new(atime.finish()),
            Arc::new(ctime.finish()),
            Arc::new(mtime.finish()),
            Arc::new(mode.finish()),
            Arc::new(size.finish()),
            Arc::new(chunk_id.finish()),
            Arc::new(chunk_offset.finish()),
            Arc::new(chunk_size.finish()),
            Arc::new(data.finish()),
        ];
        let batch = RecordBatch::try_new(schema.clone(), arrow_array)?;
        Ok(Some(batch))
    }
}

type FileRecordStream = Pin<Box<dyn Send + Stream<Item = Result<FileRecord>>>>;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub struct FileRecord<T = Vec<u8>> {
    pub name: String,
    pub parent: String,
    pub metadata: Option<FileMetadataRecord>,
    pub chunk_id: u64,
    pub chunk_offset: u64,
    pub chunk_size: u64,
    pub data: T,
}

impl FileRecord {
    #[instrument(skip(catalog))]
    async fn load(
        catalog: DatasetCatalog,
        root: PathBuf,
        path: &Path,
    ) -> impl Stream<Item = Result<Self>> {
        let bail = |error: Error| stream::iter(vec![Err(error)]);

        let mut file = match fs::File::open(path).await {
            Ok(file) => file,
            Err(error) => return bail(error.into()),
        };
        let metadata = match file.metadata().await {
            Ok(metadata) => metadata,
            Err(error) => return bail(error.into()),
        };

        if metadata.is_symlink() || !metadata.is_file() {
            return stream::iter(Vec::default());
        }

        let name = match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => return bail(anyhow!("Empty file name: {path:?}")),
        };
        let parent = match path
            .parent()
            .filter(|path| path.starts_with(&root))
            .map(|path| path.to_string_lossy()[root.to_string_lossy().len()..].to_string())
        {
            Some(parent) => parent,
            None => return bail(anyhow!("Cannot find the parent directory: {path:?}")),
        };

        #[cfg(unix)]
        let mut metadata = Some({
            use std::os::unix::fs::MetadataExt;

            let atime = DateTime::from_timestamp_nanos(metadata.atime_nsec());
            let ctime = DateTime::from_timestamp_nanos(metadata.ctime_nsec());
            let mtime = DateTime::from_timestamp_nanos(metadata.mtime_nsec());
            let mode = metadata.mode();
            let size = metadata.size();

            FileMetadataRecord {
                atime,
                ctime,
                mtime,
                mode,
                size,
            }
        });

        #[cfg(windows)]
        let mut metadata = Some({
            use std::os::windows::fs::MetadataExt;

            let atime = DateTime::from_timestamp_nanos(100 * metadata.last_access_time() as i64);
            let ctime = DateTime::from_timestamp_nanos(100 * metadata.creation_time() as i64);
            let mtime = DateTime::from_timestamp_nanos(100 * metadata.last_write_time() as i64);
            let mode = 0o777;
            let size = metadata.file_size();

            FileMetadataRecord {
                atime,
                ctime,
                mtime,
                mode,
                size,
            }
        });

        let size = metadata.as_ref().map(|m| m.size).unwrap();
        let chunk_ids = match catalog.max_chunk_size {
            0 => 0..=0,
            max_chunk_size => match size {
                0 => 0..=0,
                size => 0..=((size - 1) / max_chunk_size),
            },
        };

        let mut files = Vec::with_capacity(*chunk_ids.end() as _);
        for chunk_id in chunk_ids {
            let chunk_offset = chunk_id * catalog.max_chunk_size;
            let chunk_size = match catalog.max_chunk_size {
                0 => size,
                max_chunk_size => size.min((chunk_id + 1) * max_chunk_size) - chunk_offset,
            };
            let mut data = vec![0; chunk_size as _];
            let maybe_file = match file.read_exact(&mut data).await {
                Ok(_) => Ok(Self {
                    name: name.clone(),
                    parent: parent.clone(),
                    metadata: metadata.take(),
                    chunk_id,
                    chunk_offset,
                    chunk_size,
                    data,
                }),
                Err(error) => Err(error.into()),
            };
            files.push(maybe_file)
        }
        stream::iter(files)
    }

    #[instrument(skip_all)]
    async fn load_all(
        catalog: DatasetCatalog,
        root: &Path,
    ) -> Result<impl 'static + Stream<Item = Result<Self>>> {
        let root = fs::canonicalize(root).await?;
        let pattern = format!("{}/**/*", root.to_string_lossy());
        let list = glob(&pattern).with_context(|| format!("Failed to list files on {root:?}"))?;
        Ok(stream::iter(list.filter_map(|path| path.ok()))
            .then(move |path| {
                let catalog = catalog.clone();
                let root = root.clone();
                async move { Self::load(catalog, root, &path).await }
            })
            .flatten())
    }

    #[instrument(
        skip(self),
        fields(name = %self.name, parent = &self.parent),
    )]
    async fn dump(&self, root: &Path) -> Result<()> {
        let path = {
            let parent = trim_rel_path(self.parent.as_str());
            let base_dir = root.join(parent);
            fs::create_dir_all(&base_dir).await?;
            base_dir.join(&self.name)
        };

        let mut options = fs::File::options();
        let mut file = options.create(true).append(true).open(&path).await?;

        if self.chunk_offset > 0 {
            file.seek(SeekFrom::Start(self.chunk_offset)).await?;
        }
        file.write_all(&self.data).await?;

        if let Some(record) = self.metadata.as_ref() {
            ::filetime::set_file_atime(
                &path,
                FileTime::from_unix_time(record.atime.timestamp(), record.atime.nanosecond()),
            )?;
            ::filetime::set_file_mtime(
                &path,
                FileTime::from_unix_time(record.mtime.timestamp(), record.mtime.nanosecond()),
            )?;

            let metadata = file.metadata().await?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                let mut perm = metadata.permissions();
                perm.set_mode(record.mode);
                file.set_permissions(perm).await?;
            }
            file.set_len(record.size).await?;
        }
        Ok(())
    }

    #[instrument(skip_all)]
    async fn dump_all(root: &Path, stream: impl Stream<Item = Result<FileRecord>>) -> Result<()> {
        fs::create_dir_all(&root)
            .await
            .with_context(|| format!("Failed to create directory: {root:?}"))?;

        stream
            .try_for_each(|file| async move { file.dump(root).await })
            .await
    }

    fn columns_arrow() -> Vec<ArrowField> {
        let timestamp_micros = || ArrowDataType::Timestamp(ArrowTimeUnit::Microsecond, None);
        vec![
            ArrowField::new("name", ArrowDataType::Utf8, false),
            ArrowField::new("parent", ArrowDataType::Utf8, false),
            ArrowField::new("atime", timestamp_micros(), true),
            ArrowField::new("ctime", timestamp_micros(), true),
            ArrowField::new("mtime", timestamp_micros(), true),
            ArrowField::new("mode", ArrowDataType::UInt32, true),
            ArrowField::new("size", ArrowDataType::UInt64, true),
            ArrowField::new("chunk_id", ArrowDataType::UInt64, false),
            ArrowField::new("chunk_offset", ArrowDataType::UInt64, false),
            ArrowField::new("chunk_size", ArrowDataType::UInt64, false),
            ArrowField::new("data", ArrowDataType::Binary, true),
        ]
    }

    fn schema_arrow() -> ArrowSchema {
        ArrowSchema::new(Self::columns_arrow())
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub struct FileMetadataRecord {
    pub atime: DateTime<Utc>,
    pub ctime: DateTime<Utc>,
    pub mtime: DateTime<Utc>,
    pub mode: u32,
    pub size: u64,
}

#[instrument(skip_all)]
async fn open_table(catalog: &DatasetCatalog, dataset: &DatasetPath) -> Result<Dataset> {
    let uri = dataset.to_uri(DIR_ROOTFS);
    match DatasetBuilder::from_uri(&uri)
        .with_aws_credentials_provider(catalog.s3_credential_provider()?)
        .with_commit_handler(catalog.commit_handler())
        .with_object_store_registry(build_registry())
        .with_storage_options(catalog.storage_options(false)?)
        .load()
        .await
    {
        Ok(dataset) => Ok(dataset),
        Err(LanceError::DatasetNotFound { .. }) => bail!("Empty storage"),
        Err(error) => Err(error).with_context(|| format!("Cannot open a rootfs table on {uri:?}")),
    }
}

#[instrument(skip_all)]
async fn commit_table(
    catalog: &DatasetCatalog,
    dataset: &DatasetPath,
    stream: SendableRecordBatchStream,
) -> Result<Dataset> {
    let uri = dataset.to_uri(DIR_ROOTFS);
    let (dest, mode) = {
        let dest = WriteDestination::Uri(&uri);
        let mode = WriteMode::Append;
        (dest, mode)
    };

    let write_params = WriteParams {
        commit_handler: Some(catalog.commit_handler()),
        data_storage_version: Some(LanceFileVersion::Stable),
        enable_move_stable_row_ids: false,
        enable_v2_manifest_paths: true,
        max_bytes_per_file: catalog.max_buffer_size,
        mode,
        object_store_registry: build_registry(),
        progress: catalog.fragment_process(),
        store_params: Some(catalog.storage_parameters()?),
        ..Default::default()
    };

    InsertBuilder::new(dest)
        .with_params(&write_params)
        .execute_stream(stream)
        .await
        .with_context(|| format!("Failed to commit stream to {dataset}"))
}

async fn file_stream_to_batch_stream(
    catalog: &DatasetCatalog,
    mut stream: FileRecordStream,
) -> Result<SendableRecordBatchStream> {
    let schema = Arc::new(FileRecord::schema_arrow());

    let (tx, rx) = mpsc::channel(catalog.max_write_threads);

    spawn({
        let arrow_schema = schema.clone();
        let catalog = catalog.clone();
        async move {
            let mut builder = FileRecordBuilder::default();
            while let Some(file) = stream.try_next().await? {
                if let Some(batch) = builder.push(&catalog, &arrow_schema, file)? {
                    tx.send(Result::<_, DataFusionError>::Ok(batch)).await?;
                }
            }
            if let Some(batch) = builder.flush(&arrow_schema)? {
                tx.send(Result::<_, DataFusionError>::Ok(batch)).await?;
            }
            Result::<_, Error>::Ok(())
        }
    });

    let stream = RecordBatchStreamAdapter::new(schema, ReceiverStream::new(rx));
    Ok(Box::pin(stream))
}

fn trim_rel_path(mut path: &str) -> &str {
    while path.starts_with('/') {
        path = &path[1..];
    }
    trim_rel_suffix(path)
}

fn trim_rel_suffix(mut path: &str) -> &str {
    while path.ends_with('/') {
        path = &path[..path.len() - 1];
    }
    path
}

const DIR_ROOTFS: &str = "rootfs";
