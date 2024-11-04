use std::{collections::HashMap, fmt, ops, str::FromStr};

#[cfg(feature = "pyo3")]
use anyhow::Error;
use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use deltalake::{
    parquet::{basic, file::properties::EnabledStatistics},
    DeltaResult,
};
#[cfg(feature = "pyo3")]
use pyo3::{pyclass, pymethods, PyResult};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Parser)]
#[cfg_attr(feature = "pyo3", pyclass)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DatasetCatalog {
    /// Max directory size for cache directory.
    /// The value 0 disables the caching.
    #[arg(
        global=true, long,
        env = "CDL_CACHE_DIR",
        default_value_t = Self::default_cache_dir(),
    )]
    pub cache_dir: String,

    /// A compression method applied when storing data in backend storage.
    #[arg(
        global=true, long,
        env = "CDL_COMPRESSION",
        default_value_t = Compression::default(),
    )]
    pub compression: Compression,

    /// A compression level applied when storing data in backend storage.
    #[arg(long, env = "CDL_COMPRESSION_LEVEL")]
    pub compression_level: Option<u8>,

    /// Max file size for each parquet file.
    /// The larger the value, the faster the data transfer speed.
    /// It is recommended to use the largest possible value
    /// that is supported simultaneously by multiple backend storages.
    #[arg(
        global=true, long,
        env = "CDL_MAX_BUFFER_SIZE",
        default_value_t = Self::default_max_buffer_size(),
    )]
    pub max_buffer_size: u64,

    /// Max directory size for cache directory.
    /// The value 0 disables the caching.
    #[arg(
        global=true, long,
        env = "CDL_MAX_CACHE_SIZE",
        default_value_t = Self::default_max_cache_size(),
    )]
    pub max_cache_size: u64,

    /// Max chunk size for each file.
    /// A larger value allows more data to be stored in a row,
    /// but requires the same amount of data to be transmitted when modifying the data.
    /// The value 0 disables the chunking.
    #[arg(
        global=true, long,
        env = "CDL_MAX_CHUNK_SIZE",
        default_value_t = Self::default_max_chunk_size(),
    )]
    pub max_chunk_size: u64,

    /// Maximum number of threads for writing files.
    #[arg(
        global=true, long,
        env = "CDL_MAX_WRITE_THREADS",
        default_value_t = Self::default_max_write_threads(),
    )]
    pub max_write_threads: usize,

    /// Min object size for caching larege files.
    #[arg(
        global=true, long,
        env = "CDL_MIN_CACHE_OBJECT_SIZE",
        default_value_t = Self::default_min_cache_object_size(),
    )]
    pub min_cache_object_size: usize,

    /// S3 access key.
    #[arg(global = true, long, env = "AWS_ACCESS_KEY_ID")]
    pub s3_access_key: Option<String>,

    /// S3 region name.
    #[arg(
        global=true, long,
        env = "AWS_ENDPOINT_URL",
        default_value = Self::default_s3_endpoint(),
    )]
    pub s3_endpoint: Url,

    /// S3 region name. Needed for AWS S3.
    #[arg(
        global=true, long,
        env = "AWS_REGION",
        default_value = Self::default_s3_region(),
    )]
    pub s3_region: String,

    /// S3 secret key.
    #[arg(global = true, long, env = "AWS_SECRET_ACCESS_KEY")]
    pub s3_secret_key: Option<String>,
}

impl Default for DatasetCatalog {
    fn default() -> Self {
        Self {
            cache_dir: Self::default_cache_dir(),
            compression: Compression::default(),
            compression_level: None,
            max_buffer_size: Self::default_max_buffer_size(),
            max_cache_size: Self::default_max_cache_size(),
            max_chunk_size: Self::default_max_chunk_size(),
            max_write_threads: Self::default_max_write_threads(),
            min_cache_object_size: Self::default_min_cache_object_size(),
            s3_access_key: None,
            s3_endpoint: Self::default_s3_endpoint()
                .parse()
                .expect("Invalid fallback s3 endpoint"),
            s3_region: Self::default_s3_region().into(),
            s3_secret_key: None,
        }
    }
}

impl DatasetCatalog {
    pub fn default_cache_dir() -> String {
        "./cache".into()
    }

    #[allow(clippy::identity_op)]
    #[inline]
    pub const fn default_max_buffer_size() -> u64 {
        1 * 1024 * 1024 * 1024 // 1 GiB
    }

    #[allow(clippy::identity_op)]
    #[inline]
    pub const fn default_max_cache_size() -> u64 {
        32 * 1024 * 1024 * 1024 // 32 GiB
    }

    #[allow(clippy::identity_op)]
    #[inline]
    pub const fn default_max_chunk_size() -> u64 {
        256 * 1024 // 256 KiB
    }

    #[inline]
    pub const fn default_max_write_threads() -> usize {
        2
    }

    #[allow(clippy::identity_op)]
    #[inline]
    pub const fn default_min_cache_object_size() -> usize {
        64 * 1024 * 1024 // 64 MiB
    }

    #[inline]
    pub const fn default_s3_endpoint() -> &'static str {
        "http://object-storage"
    }

    #[inline]
    pub const fn default_s3_region() -> &'static str {
        "auto"
    }
}

impl DatasetCatalog {
    pub const KEY_CACHE_DIR: &'static str = "CDL_CACHE_DIR";
    pub const KEY_MAX_CACHE_SIZE: &'static str = "CDL_MAX_CACHE_SIZE";
    pub const KEY_MIN_CACHE_OBJECT_SIZE: &'static str = "CDL_MIN_CACHE_OBJECT_SIZE";

    pub fn storage_options(&self) -> Result<HashMap<String, String>> {
        let allow_http = self.s3_endpoint.scheme() == "http";

        fn get_arg<T>(arg: Option<&T>, name: &'static str) -> Result<T>
        where
            T: Clone,
        {
            arg.cloned()
                .with_context(|| format!("Missing catalog config: {name}"))
        }

        macro_rules! get_arg {
            ( $name:ident ) => {{
                get_arg(self.$name.as_ref(), stringify!($name))
            }};
        }

        let mut options = HashMap::default();
        // Cache
        options.insert(Self::KEY_CACHE_DIR.into(), self.cache_dir.clone());
        options.insert(
            Self::KEY_MAX_CACHE_SIZE.into(),
            self.max_cache_size.to_string(),
        );
        options.insert(
            Self::KEY_MIN_CACHE_OBJECT_SIZE.into(),
            self.min_cache_object_size.to_string(),
        );
        // S3
        options.insert("allow_http".into(), allow_http.to_string());
        options.insert("AWS_ACCESS_KEY_ID".into(), get_arg!(s3_access_key)?);
        options.insert("AWS_ALLOW_HTTP".into(), allow_http.to_string());
        options.insert("AWS_EC2_METADATA_DISABLED".into(), true.to_string());
        options.insert("AWS_ENDPOINT_URL".into(), self.s3_endpoint.to_string());
        options.insert("AWS_REGION".into(), self.s3_region.clone());
        options.insert("AWS_SECRET_ACCESS_KEY".into(), get_arg!(s3_secret_key)?);
        options.insert("conditional_put".into(), "etag".into());
        Ok(options)
    }

    pub fn compression(&self) -> DeltaResult<basic::Compression> {
        Ok(match self.compression {
            Compression::BROTLI => basic::Compression::BROTLI(match self.compression_level {
                Some(level) => basic::BrotliLevel::try_new(level as _)?,
                None => basic::BrotliLevel::default(),
            }),
            Compression::GZIP => basic::Compression::GZIP(match self.compression_level {
                Some(level) => basic::GzipLevel::try_new(level as _)?,
                None => basic::GzipLevel::default(),
            }),
            Compression::LZO => basic::Compression::LZO,
            Compression::LZ4 => basic::Compression::LZ4,
            Compression::LZ4_RAW => basic::Compression::LZ4_RAW,
            Compression::SNAPPY => basic::Compression::SNAPPY,
            Compression::UNCOMPRESSED => basic::Compression::UNCOMPRESSED,
            Compression::ZSTD => basic::Compression::ZSTD(match self.compression_level {
                Some(level) => basic::ZstdLevel::try_new(level as _)?,
                None => basic::ZstdLevel::default(),
            }),
        })
    }

    #[inline]
    pub const fn enabled_statistics(&self) -> EnabledStatistics {
        EnabledStatistics::None
    }
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, Display, Default, PartialEq, Eq, Hash, EnumString, ValueEnum)]
#[cfg_attr(feature = "pyo3", pyclass)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[strum(serialize_all = "kebab-case")]
pub enum Compression {
    BROTLI = 0,
    GZIP = 1,
    LZO = 2,
    LZ4 = 3,
    LZ4_RAW = 4,
    #[default]
    SNAPPY = 5,
    UNCOMPRESSED = 6,
    ZSTD = 7,
}

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "pyo3", pyclass)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[repr(transparent)]
pub struct Url(::url::Url);

impl FromStr for Url {
    type Err = ::url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ::url::Url::from_str(s).map(Self)
    }
}

impl fmt::Display for Url {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ops::Deref for Url {
    type Target = ::url::Url;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ops::DerefMut for Url {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl Url {
    #[new]
    #[pyo3(signature = (
        url,
        /,
    ))]
    fn new(url: &str) -> PyResult<Self> {
        url.parse().map_err(|error| Error::from(error).into())
    }
}
