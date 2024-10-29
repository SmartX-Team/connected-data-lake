use std::{
    io::SeekFrom,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
};

use anyhow::{anyhow, bail, Context, Error, Result};
use cdl_catalog::DatasetCatalog;
use chrono::{DateTime, Timelike, Utc};
use deltalake::{
    kernel::{DataType, PrimitiveType, StructField},
    parquet::{
        basic::{Compression, ZstdLevel},
        file::properties::WriterProperties,
    },
    writer::RecordBatchWriter,
    DeltaOps, DeltaTable, DeltaTableError,
};
use filetime::FileTime;
use futures::{
    stream::{self, FuturesOrdered},
    Stream, StreamExt,
};
use glob::glob;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};
use tracing::{info, instrument, Level};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GlobalPath {
    pub site: DatasetPath,
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
            None => {
                return Ok(Self {
                    site: DatasetPath {
                        scheme: Scheme::Local,
                        name: "localhost".into(),
                    },
                    rel: s.parse()?,
                })
            }
        };
        let scheme = Scheme::from_str(scheme)?;

        let mut slice = next.split("/");
        let name = slice.next().unwrap().trim();
        if name.is_empty() {
            bail!("Empty site name: {s}")
        }

        let rel = slice
            .next()
            .map(|path| path.trim().parse())
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            site: DatasetPath {
                scheme,
                name: name.into(),
            },
            rel,
        })
    }
}

impl GlobalPath {
    pub async fn copy_all(&self, catalog: &DatasetCatalog, dst: &Self) -> Result<()> {
        let stream = self.load_all(catalog).await?;
        dst.dump_all(catalog, stream).await
    }

    async fn load_all<'a>(
        &'a self,
        catalog: &'a DatasetCatalog,
    ) -> Result<Pin<Box<dyn 'a + Stream<Item = Result<FileRecord>>>>> {
        match self.site.scheme {
            Scheme::Local => Ok(Box::pin(FileRecord::load_all(catalog, &self.rel)?)),
            Scheme::S3A => {
                let table = open_table(&catalog, &self.site).await?;

                let writer_properties = WriterProperties::builder()
                    .set_compression(Compression::ZSTD(ZstdLevel::default()))
                    .build();

                let mut writer = RecordBatchWriter::for_table(&table)
                    .expect("Failed to make RecordBatchWriter")
                    .with_writer_properties(writer_properties);

                Ok(todo!())
            }
        }
    }

    async fn dump_all(
        &self,
        catalog: &DatasetCatalog,
        stream: impl Stream<Item = Result<FileRecord>>,
    ) -> Result<()> {
        let files: Vec<_> = stream.collect().await;
        dbg!(files.len());
        dbg!(files
            .iter()
            .filter_map(|f| f.as_ref().ok())
            .map(|f| f.chunk_size)
            .sum::<u64>());
        todo!()
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

#[derive(Clone, Debug, Default)]
struct FileRecordBatch<T = Vec<u8>> {
    pub name: Vec<String>,
    pub parent: Vec<Option<String>>,
    pub atime: Vec<Option<DateTime<Utc>>>,
    pub ctime: Vec<Option<DateTime<Utc>>>,
    pub mtime: Vec<Option<DateTime<Utc>>>,
    pub size: Vec<Option<u64>>,
    pub total_size: u64,
    pub chunk_id: Vec<u64>,
    pub chunk_offset: Vec<u64>,
    pub chunk_size: Vec<u64>,
    pub data: Vec<T>,
}

impl<T> FileRecordBatch<T> {
    fn push(&mut self, item: FileRecord<T>) -> bool {
        match self.total_size.checked_add(item.chunk_size) {
            Some(total_size) => {
                self.name.push(item.name);
                self.parent.push(item.parent);
                self.atime.push(item.metadata.as_ref().map(|m| m.atime));
                self.ctime.push(item.metadata.as_ref().map(|m| m.ctime));
                self.mtime.push(item.metadata.as_ref().map(|m| m.mtime));
                self.size.push(item.metadata.as_ref().map(|m| m.size));
                self.total_size = total_size;
                self.chunk_id.push(item.chunk_id);
                self.chunk_offset.push(item.chunk_offset);
                self.chunk_size.push(item.chunk_size);
                self.data.push(item.data);
                true
            }
            None => false,
        }
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
        root: &'a Path,
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
            .filter(|path| path.starts_with(root))
            .map(|path| path.to_string_lossy()[root.to_string_lossy().len()..].to_string());

        let atime = match metadata.accessed() {
            Ok(time) => DateTime::from(time),
            Err(error) => return bail(error.into()),
        };
        let ctime = match metadata.created() {
            Ok(time) => DateTime::from(time),
            Err(error) => return bail(error.into()),
        };
        let mtime = match metadata.modified() {
            Ok(time) => DateTime::from(time),
            Err(error) => return bail(error.into()),
        };
        let size = metadata.size();

        let mut metadata = Some(FileMetadataRecord {
            atime,
            ctime,
            mtime,
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

    #[instrument(err(level = Level::ERROR))]
    fn load_all<'a>(
        catalog: &'a DatasetCatalog,
        root: &'a Path,
    ) -> Result<impl 'a + Stream<Item = Result<Self>>> {
        let pattern = format!("{}/**/*", root.to_string_lossy());
        let list = glob(&pattern).with_context(|| format!("Failed to list files on {root:?}"))?;
        Ok(list
            .into_iter()
            .filter_map(|path| path.ok())
            .map(|path| async move { Self::load(catalog, root, &path).await })
            .collect::<FuturesOrdered<_>>()
            .flatten())
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    async fn dump(&self, root: &Path) -> Result<()> {
        let path = match self.parent.as_ref() {
            Some(parent) => {
                let mut parent = parent.as_str();
                while parent.starts_with("/") {
                    parent = &parent[1..];
                }
                root.join(parent).join(&self.name)
            }
            None => root.join(&self.name),
        };

        let mut options = fs::File::options();
        if self.metadata.is_some() {
            options.create(true).truncate(true);
        } else {
            options.append(true);
        };
        let mut file = options.write(true).open(&path).await?;

        if self.chunk_offset > 0 {
            file.seek(SeekFrom::Start(self.chunk_offset)).await?;
        }
        file.write_all(&self.data).await?;

        if let Some(metadata) = self.metadata.as_ref() {
            ::filetime::set_file_atime(
                &path,
                FileTime::from_unix_time(metadata.atime.timestamp(), metadata.atime.nanosecond()),
            )?;
            ::filetime::set_file_mtime(
                &path,
                FileTime::from_unix_time(metadata.mtime.timestamp(), metadata.mtime.nanosecond()),
            )?;
        }
        Ok(())
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
                "accessed_at".to_string(),
                DataType::Primitive(PrimitiveType::TimestampNtz),
                true,
            ),
            StructField::new(
                "created_at".to_string(),
                DataType::Primitive(PrimitiveType::TimestampNtz),
                true,
            ),
            StructField::new(
                "modified_at".to_string(),
                DataType::Primitive(PrimitiveType::TimestampNtz),
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
    pub size: u64,
}

#[instrument(skip_all, err(level = Level::ERROR))]
async fn open_table(catalog: &DatasetCatalog, site: &DatasetPath) -> Result<DeltaTable> {
    let uri = site.to_uri("rootfs");
    match ::deltalake::open_table_with_storage_options(&uri, catalog.storage_options()).await {
        Ok(table) => Ok(table),
        Err(DeltaTableError::NotATable(_)) => {
            info!("Cannot find a rootfs table; creating");
            create_delta_ops(catalog, &uri)
                .await?
                .create()
                .with_columns(FileRecord::columns())
                .await
                .map_err(Into::into)
        }
        Err(error) => Err(error).with_context(|| format!("Cannot load a rootfs table on {uri:?}")),
    }
}

#[instrument(skip(catalog), err(level = Level::ERROR))]
async fn create_delta_ops(catalog: &DatasetCatalog, uri: &str) -> Result<DeltaOps> {
    DeltaOps::try_from_uri_with_storage_options(uri, catalog.storage_options())
        .await
        .with_context(|| format!("Cannot init a rootfs table on {uri:?}"))
}
