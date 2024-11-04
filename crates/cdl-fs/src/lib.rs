mod functions;

use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Error, Result};
use cdl_catalog::DatasetCatalog;
pub use cdl_store::register_handlers;
use chrono::{DateTime, Timelike, Utc};
use deltalake::{
    arrow::{
        array::{self, ArrayBuilder, ArrayRef, AsArray, RecordBatch},
        datatypes::SchemaRef,
    },
    datafusion::{
        execution::SendableRecordBatchStream,
        prelude::{DataFrame, SessionConfig, SessionContext},
    },
    kernel::{Action, DataType, PrimitiveType, StructField},
    operations::transaction::CommitBuilder,
    parquet::file::properties::WriterProperties,
    protocol::{DeltaOperation, SaveMode},
    writer::{DeltaWriter, RecordBatchWriter},
    DeltaOps, DeltaTable,
};
use filetime::FileTime;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use glob::glob;
use itertools::Itertools;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    spawn, sync,
    time::sleep,
};
use tracing::{debug, info, instrument, Level};

pub struct CdlFS {
    catalog: DatasetCatalog,
    ctx: SessionContext,
    path: GlobalPath,
}

impl CdlFS {
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
    pub async fn read_files(
        &self,
        files: &RecordBatch,
    ) -> Result<impl '_ + Send + Stream<Item = Result<FileRecord>>> {
        let records = FileRecordBatch::try_from(files)?.to_vec()?;
        let condition_name = records
            .iter()
            .map(|FileRecord { name, parent, .. }| {
                format!("name = '{name}' AND parent = '{parent}'")
            })
            .join(" OR ");
        // let condition =
        //     format!("WHERE {condition_name} ORDER BY parent ASC, name ASC, chunk_id ASC");
        // let condition = format!("ORDER BY parent ASC, name ASC, chunk_id ASC");
        // let condition = "";
        let condition = format!("WHERE {condition_name} ORDER BY RANDOM()");

        info!("{}", 1);
        let stream = self.load_by(&condition).await?;
        info!("{}", 2);
        // let files: Vec<_> = stream
        //     .map_ok(|d| {
        //         info!("{}", 2.1);
        //         d
        //     })
        //     .try_collect()
        //     .await?;
        // info!("{}", 3);
        // info!("{}", files[0].num_rows());
        let mut stream = stream;
        while let Some(d) = stream.try_next().await? {
            info!("{}", 2.1);
        }
        todo!();
        Ok(stream::empty())
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

    async fn load_all(
        &self,
    ) -> Result<Pin<Box<dyn '_ + Send + Stream<Item = Result<FileRecord>>>>> {
        let Self {
            catalog,
            ctx: _,
            path: GlobalPath { dataset, rel: root },
        } = self;

        match dataset.scheme {
            Scheme::Local => Ok(Box::pin(
                FileRecord::<Vec<u8>>::load_all(catalog, root).await?,
            )),
            Scheme::S3A => {
                let (_, stream) = open_table(catalog, dataset).await?;
                // TODO: filter root path
                let stream = stream
                    .map(|batch| {
                        batch
                            .map_err(Into::into)
                            .and_then(|ref batch| FileRecordBatch::try_from(batch))
                            .and_then(FileRecordBatch::to_vec)
                            .map(|records| stream::iter(records.into_iter().map(Ok)))
                    })
                    .try_flatten();
                Ok(Box::pin(stream))
            }
        }
    }

    #[inline]
    async fn load_by(&self, condition: &str) -> Result<SendableRecordBatchStream> {
        // let sql = format!("SELECT parent, name, COUNT(data) AS combined_data FROM {DIR_ROOTFS} GROUP BY parent, name ORDER BY RANDOM()");
        let sql = format!("SELECT parent, name, COUNT(data) AS combined_data FROM {DIR_ROOTFS} GROUP BY parent, name");
        info!("Querying LOAD: {sql}");

        let df = self.query(&sql).await?;
        let mut stream = df
            .execute_stream()
            .await
            .context("Failed to execute the dataframe")?;
        while let Some(d) = stream.try_next().await? {
            info!("Incoming: {}", d.num_rows());
        }
        info!("Completed");

        todo!();

        let sql = format!("SELECT * FROM {DIR_ROOTFS} {condition}");
        info!("Querying LOAD: {sql}");

        let df = self.query(&sql).await?;
        df.execute_stream()
            .await
            .context("Failed to execute the dataframe")
    }

    async fn table(&self) -> Result<DeltaTable> {
        let Self {
            catalog,
            ctx: _,
            path: GlobalPath { dataset, .. },
        } = self;

        match dataset.scheme {
            Scheme::Local => bail!("Local filesystem does not support CDL rootfs table"),
            Scheme::S3A => open_table(catalog, dataset).await.map(|(table, _)| table),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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

        let rel = slice
            .next()
            .map(|path| path.trim().parse())
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            dataset: DatasetPath {
                scheme,
                name: name.into(),
            },
            rel,
        })
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
        ctx.register_udf(crate::functions::len::Udf::new());

        Ok(CdlFS {
            catalog,
            ctx,
            path: self,
        })
    }

    async fn dump_all(
        &self,
        catalog: &DatasetCatalog,
        stream: impl Stream<Item = Result<FileRecord>>,
    ) -> Result<()> {
        match self.dataset.scheme {
            Scheme::Local => FileRecord::dump_all(&self.rel, stream).await,
            Scheme::S3A => {
                let table = create_table(catalog, &self.dataset).await?;
                let schema: SchemaRef = Arc::new(table.get_schema()?.try_into()?);

                let writer_properties = WriterProperties::builder()
                    .set_compression(catalog.compression()?)
                    .set_statistics_enabled(catalog.enabled_statistics())
                    .build();

                let writer = RecordBatchWriter::for_table(&table)
                    .expect("Failed to make RecordBatchWriter")
                    .with_writer_properties(writer_properties);

                struct Writer {
                    actions: Vec<Action>,
                    count: usize,
                    inner: RecordBatchWriter,
                }

                impl Writer {
                    async fn write(&mut self, batch: RecordBatch) -> Result<()> {
                        self.count += batch.num_rows();
                        self.inner.write(batch).await?;
                        self.actions
                            .extend(&mut self.inner.flush().await?.into_iter().map(Action::Add));
                        Ok(())
                    }
                }

                let mut builder = FileRecordBuilder::default();
                let num_running_tasks = Arc::new(AtomicUsize::default());
                let mut stream = Box::pin(stream);
                let mut tasks = Vec::default();
                let writer = Arc::new(sync::Mutex::new(Writer {
                    actions: Vec::default(),
                    count: 0,
                    inner: writer,
                }));

                let wait_for_commit = || async {
                    while num_running_tasks.load(Ordering::SeqCst) >= catalog.max_write_threads {
                        sleep(Duration::from_millis(10)).await;
                    }
                    num_running_tasks.fetch_add(1, Ordering::SeqCst);
                };
                let mut commit = |batch| {
                    let num_running_tasks = num_running_tasks.clone();
                    let writer = writer.clone();
                    tasks.push(spawn(async move {
                        let result = writer.lock().await.write(batch).await;
                        num_running_tasks.fetch_sub(1, Ordering::SeqCst);
                        result
                    }));
                };

                while let Some(file) = stream.try_next().await? {
                    if let Some(batch) = builder.push(catalog, &schema, file)? {
                        wait_for_commit().await;
                        commit(batch);
                    }
                }
                if let Some(batch) = builder.flush(&schema)? {
                    wait_for_commit().await;
                    commit(batch);
                }
                let () = tasks
                    .into_iter()
                    .collect::<stream::FuturesOrdered<_>>()
                    .map(|result| {
                        result
                            .map_err(Error::from)
                            .and_then(|result| result.map_err(Error::from))
                    })
                    .try_collect()
                    .await?;

                let Writer {
                    actions,
                    count,
                    inner: _,
                } = sync::Mutex::into_inner(Arc::into_inner(writer).expect("Writer is poisoned"));
                if !actions.is_empty() {
                    let snapshot = table.snapshot()?;
                    let operation = DeltaOperation::Write {
                        mode: SaveMode::Append,
                        partition_by: {
                            let partition_cols = snapshot.metadata().partition_columns.clone();
                            if !partition_cols.is_empty() {
                                Some(partition_cols)
                            } else {
                                None
                            }
                        },
                        predicate: None,
                    };

                    let log_store = table.log_store();
                    let version = CommitBuilder::default()
                        .with_actions(actions)
                        .build(Some(snapshot), log_store, operation)
                        .await?
                        .version();
                    info!("{count} files are dumped on version {version}");
                } else {
                    info!("No files are dumped");
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DatasetPath {
    pub scheme: Scheme,
    pub name: String,
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
            Scheme::S3A => {
                let name = &self.name;
                let rel = trim_rel_path(rel);
                format!("s3a://{name}/{rel}")
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Scheme {
    Local,
    S3A,
}

impl FromStr for Scheme {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "s3" | "s3a" => Ok(Self::S3A),
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
    pub mode: array::Int32Array,
    pub size: array::Int64Array,
    pub chunk_id: array::Int64Array,
    pub chunk_offset: array::Int64Array,
    pub chunk_size: array::Int64Array,
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
    fn to_vec(self) -> Result<Vec<FileRecord>> {
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
    pub mode: array::Int32Builder,
    pub size: array::Int64Builder,
    pub total_size: u64,
    pub chunk_id: array::Int64Builder,
    pub chunk_offset: array::Int64Builder,
    pub chunk_size: array::Int64Builder,
    pub data: array::BinaryBuilder,
}

impl FileRecordBuilder {
    fn push(
        &mut self,
        catalog: &DatasetCatalog,
        schema: &SchemaRef,
        file: FileRecord,
    ) -> Result<Option<RecordBatch>> {
        match self.total_size.checked_add(file.chunk_size) {
            Some(total_size) => {
                let batch = if total_size > catalog.max_buffer_size {
                    let batch = self.flush(schema)?;
                    self.total_size = file.chunk_size;
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

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
    async fn load<'a>(
        catalog: &'a DatasetCatalog,
        root: PathBuf,
        path: &Path,
    ) -> impl 'a + Stream<Item = Result<Self>> {
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
    async fn load_all<'a>(
        catalog: &'a DatasetCatalog,
        root: &'a Path,
    ) -> Result<impl 'a + Stream<Item = Result<Self>>> {
        let root = fs::canonicalize(root).await?;
        let pattern = format!("{}/**/*", root.to_string_lossy());
        let list = glob(&pattern).with_context(|| format!("Failed to list files on {root:?}"))?;
        Ok(stream::iter(list.filter_map(|path| path.ok()))
            .then(move |path| {
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

    fn columns() -> Vec<StructField> {
        vec![
            StructField::new(
                "name".to_string(),
                DataType::Primitive(PrimitiveType::String),
                false,
            ),
            StructField::new(
                "parent".to_string(),
                DataType::Primitive(PrimitiveType::String),
                false,
            ),
            StructField::new(
                "atime".to_string(),
                DataType::Primitive(PrimitiveType::TimestampNtz),
                true,
            ),
            StructField::new(
                "ctime".to_string(),
                DataType::Primitive(PrimitiveType::TimestampNtz),
                true,
            ),
            StructField::new(
                "mtime".to_string(),
                DataType::Primitive(PrimitiveType::TimestampNtz),
                true,
            ),
            StructField::new(
                "mode".to_string(),
                DataType::Primitive(PrimitiveType::Integer),
                true,
            ),
            StructField::new(
                "size".to_string(),
                DataType::Primitive(PrimitiveType::Long),
                true,
            ),
            StructField::new(
                "chunk_id".to_string(),
                DataType::Primitive(PrimitiveType::Long),
                false,
            ),
            StructField::new(
                "chunk_offset".to_string(),
                DataType::Primitive(PrimitiveType::Long),
                false,
            ),
            StructField::new(
                "chunk_size".to_string(),
                DataType::Primitive(PrimitiveType::Long),
                false,
            ),
            StructField::new(
                "data".to_string(),
                DataType::Primitive(PrimitiveType::Binary),
                true,
            ),
        ]
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FileMetadataRecord {
    pub atime: DateTime<Utc>,
    pub ctime: DateTime<Utc>,
    pub mtime: DateTime<Utc>,
    pub mode: u32,
    pub size: u64,
}

#[instrument(skip_all)]
async fn create_table(catalog: &DatasetCatalog, dataset: &DatasetPath) -> Result<DeltaTable> {
    let uri = dataset.to_uri(DIR_ROOTFS);
    let ops = create_delta_ops(catalog, &uri).await?;
    match &ops.0.state {
        Some(_) => ops
            .load()
            .await
            .map(|(table, _)| table)
            .with_context(|| format!("Cannot load a rootfs table on {uri:?}")),
        None => ops
            .create()
            .with_columns(FileRecord::columns())
            .await
            .with_context(|| format!("Cannot create a rootfs table on {uri:?}")),
    }
}

#[instrument(skip_all)]
async fn open_table(
    catalog: &DatasetCatalog,
    dataset: &DatasetPath,
) -> Result<(DeltaTable, SendableRecordBatchStream)> {
    let uri = dataset.to_uri(DIR_ROOTFS);
    let ops = create_delta_ops(catalog, &uri).await?;
    if ops.0.state.is_none() {
        bail!("Empty storage")
    }

    ops.load()
        .await
        .with_context(|| format!("Cannot open a rootfs table on {uri:?}"))
}

#[instrument(skip(catalog))]
async fn create_delta_ops(catalog: &DatasetCatalog, uri: &str) -> Result<DeltaOps> {
    DeltaOps::try_from_uri_with_storage_options(uri, catalog.storage_options()?)
        .await
        .with_context(|| format!("Cannot init a rootfs table on {uri:?}"))
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
