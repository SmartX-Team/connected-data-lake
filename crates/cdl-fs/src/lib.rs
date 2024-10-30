use std::{
    io::SeekFrom,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Error, Result};
use cdl_catalog::DatasetCatalog;
use chrono::{DateTime, Timelike, Utc};
use deltalake::{
    arrow::{
        array::{self, ArrayBuilder, ArrayRef, AsArray, RecordBatch},
        datatypes::SchemaRef,
    },
    datafusion::execution::SendableRecordBatchStream,
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
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    spawn, sync,
};
use tracing::{info, instrument, Level};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
            dataset: DatasetPath {
                scheme: Scheme::Local,
                name: "localhost".into(),
            },
            rel,
        }
    }

    pub async fn open_table(
        &self,
        catalog: &DatasetCatalog,
    ) -> Result<(DeltaTable, SendableRecordBatchStream)> {
        match self.dataset.scheme {
            Scheme::Local => bail!("Local filesystem does not support CDL rootfs table"),
            Scheme::S3A => open_table(catalog, &self.dataset).await,
        }
    }

    pub async fn copy_all(&self, catalog: &DatasetCatalog, dst: &Self) -> Result<()> {
        let stream = self.load_all_as_stream(catalog).await?;
        dst.dump_all(catalog, stream).await
    }

    async fn load_all_as_stream<'a>(
        &'a self,
        catalog: &'a DatasetCatalog,
    ) -> Result<Pin<Box<dyn 'a + Stream<Item = Result<FileRecord>>>>> {
        match self.dataset.scheme {
            Scheme::Local => Ok(Box::pin(FileRecord::load_all(catalog, &self.rel).await?)),
            Scheme::S3A => {
                let (_, stream) = open_table(&catalog, &self.dataset).await?;
                let stream = stream
                    .map(|batch| {
                        batch
                            .map_err(Into::into)
                            .and_then(FileRecordBatch::try_from)
                            .map(FileRecordBatch::into_stream)
                    })
                    .try_flatten();
                Ok(Box::pin(stream))
            }
        }
    }

    async fn dump_all(
        &self,
        catalog: &DatasetCatalog,
        stream: impl Stream<Item = Result<FileRecord>>,
    ) -> Result<()> {
        match self.dataset.scheme {
            Scheme::Local => FileRecord::dump_all(&self.rel, stream).await,
            Scheme::S3A => {
                let table = create_table(&catalog, &self.dataset).await?;
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
                let mut stream = Box::pin(stream);
                let mut tasks = Vec::default();
                let writer = Arc::new(sync::Mutex::new(Writer {
                    actions: Vec::default(),
                    count: 0,
                    inner: writer,
                }));
                while let Some(file) = stream.try_next().await? {
                    if let Some(batch) = builder.push(catalog, &schema, file)? {
                        let writer = writer.clone();
                        tasks.push(spawn(async move { writer.lock().await.write(batch).await }));
                    }
                }
                if let Some(batch) = builder.flush(&schema)? {
                    let writer = writer.clone();
                    tasks.push(spawn(async move { writer.lock().await.write(batch).await }));
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
pub struct DatasetPath {
    pub scheme: Scheme,
    pub name: String,
}

impl DatasetPath {
    pub fn to_uri(&self, mut rel: &str) -> String {
        match self.scheme {
            Scheme::Local => rel.into(),
            Scheme::S3A => {
                let name = &self.name;
                while rel.starts_with("/") {
                    rel = &rel[1..];
                }
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
            "s3a" => Ok(Self::S3A),
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

impl TryFrom<RecordBatch> for FileRecordBatch {
    type Error = Error;

    fn try_from(batch: RecordBatch) -> Result<Self, Self::Error> {
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
            name: get_column(&batch, "name", |c| c.as_string_opt())?,
            parent: get_column(&batch, "parent", |c| c.as_string_opt())?,
            atime: get_column(&batch, "atime", |c| c.as_primitive_opt())?,
            ctime: get_column(&batch, "ctime", |c| c.as_primitive_opt())?,
            mtime: get_column(&batch, "mtime", |c| c.as_primitive_opt())?,
            mode: get_column(&batch, "mode", |c| c.as_primitive_opt())?,
            size: get_column(&batch, "size", |c| c.as_primitive_opt())?,
            chunk_id: get_column(&batch, "chunk_id", |c| c.as_primitive_opt())?,
            chunk_offset: get_column(&batch, "chunk_offset", |c| c.as_primitive_opt())?,
            chunk_size: get_column(&batch, "chunk_size", |c| c.as_primitive_opt())?,
            data: get_column(&batch, "data", |c| c.as_binary_opt())?,
        })
    }
}

impl FileRecordBatch {
    fn into_stream(self) -> impl Stream<Item = Result<FileRecord>> {
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

        stream::iter(
            data.filter_map(|data| {
                Some(Ok(FileRecord {
                    name: name.next()??.into(),
                    parent: parent.next()?.map(Into::into),
                    metadata: get_metadata(),
                    chunk_id: chunk_id.next()?? as _,
                    chunk_offset: chunk_offset.next()?? as _,
                    chunk_size: chunk_size.next()?? as _,
                    data: data?.to_vec(),
                }))
            })
            .collect::<Vec<_>>(),
        )
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
                self.parent.append_option(file.parent);
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
pub struct FileRecord<T = Vec<u8>> {
    pub name: String,
    pub parent: Option<String>,
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
        let parent = path
            .parent()
            .filter(|path| path.starts_with(&root))
            .map(|path| path.to_string_lossy()[root.to_string_lossy().len()..].to_string());

        let atime = DateTime::from_timestamp_nanos(metadata.atime_nsec());
        let ctime = DateTime::from_timestamp_nanos(metadata.ctime_nsec());
        let mtime = DateTime::from_timestamp_nanos(metadata.mtime_nsec());
        let mode = metadata.mode();
        let size = metadata.size();

        let mut metadata = Some(FileMetadataRecord {
            atime,
            ctime,
            mtime,
            mode,
            size,
        });

        if catalog.max_chunk_size == 0 {
            return bail(anyhow!("Max chunk size should be positive"));
        }
        let chunk_ids = match size {
            0 => 0..=0,
            size => 0..=((size - 1) / catalog.max_chunk_size),
        };

        let mut files = Vec::with_capacity(*chunk_ids.end() as _);
        for chunk_id in chunk_ids {
            let chunk_offset = chunk_id * catalog.max_chunk_size;
            let chunk_size = size.min((chunk_id + 1) * catalog.max_chunk_size) - chunk_offset;
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

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn load_all<'a>(
        catalog: &'a DatasetCatalog,
        root: &'a Path,
    ) -> Result<impl 'a + Stream<Item = Result<Self>>> {
        let root = fs::canonicalize(root).await?;
        let pattern = format!("{}/**/*", root.to_string_lossy());
        let list = glob(&pattern).with_context(|| format!("Failed to list files on {root:?}"))?;
        Ok(list
            .into_iter()
            .filter_map(|path| path.ok())
            .map(|path| {
                let root = root.clone();
                async move { Self::load(catalog, root, &path).await }
            })
            .collect::<stream::FuturesOrdered<_>>()
            .flatten())
    }

    #[instrument(
        skip(self),
        fields(name = %self.name, parent = &self.parent),
        err(level = Level::ERROR),
    )]
    async fn dump(&self, root: &Path) -> Result<()> {
        let path = match self.parent.as_ref() {
            Some(parent) => {
                let mut parent = parent.as_str();
                while parent.starts_with("/") {
                    parent = &parent[1..];
                }
                let base_dir = root.join(parent);
                fs::create_dir_all(&base_dir).await?;
                base_dir.join(&self.name)
            }
            None => root.join(&self.name),
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
            let mut perm = metadata.permissions();
            perm.set_mode(record.mode);
            file.set_permissions(perm).await?;
            file.set_len(record.size).await?;
        }
        Ok(())
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
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
                true,
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
pub struct FileMetadataRecord {
    pub atime: DateTime<Utc>,
    pub ctime: DateTime<Utc>,
    pub mtime: DateTime<Utc>,
    pub mode: u32,
    pub size: u64,
}

#[instrument(skip_all, err(level = Level::ERROR))]
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

#[instrument(skip_all, err(level = Level::ERROR))]
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

#[instrument(skip(catalog), err(level = Level::ERROR))]
async fn create_delta_ops(catalog: &DatasetCatalog, uri: &str) -> Result<DeltaOps> {
    DeltaOps::try_from_uri_with_storage_options(uri, catalog.storage_options())
        .await
        .with_context(|| format!("Cannot init a rootfs table on {uri:?}"))
}

const DIR_ROOTFS: &str = "rootfs";
