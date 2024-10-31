use std::{collections::HashMap, fmt, ops, str::FromStr};

#[cfg(feature = "pyo3")]
use anyhow::Error;
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
use tracing::info;

#[derive(Clone, Debug, PartialEq, Parser)]
#[cfg_attr(feature = "pyo3", pyclass)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DatasetCatalog {
    /// A compression method applied when storing data in backend storage.
    #[arg(
        long,
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
        long,
        env = "CDL_MAX_BUFFER_SIZE",
        default_value_t = Self::default_max_buffer_size(),
    )]
    pub max_buffer_size: u64,

    /// Max chunk size for each file.
    /// A larger value allows more data to be stored in a row,
    /// but requires the same amount of data to be transmitted when modifying the data.
    #[arg(
        long,
        env = "CDL_MAX_CHUNK_SIZE",
        default_value_t = Self::default_max_chunk_size(),
    )]
    pub max_chunk_size: u64,

    /// S3 access key.
    #[arg(long, env = "AWS_ACCESS_KEY_ID")]
    pub s3_access_key: String,

    /// S3 region name.
    #[arg(
        long,
        env = "AWS_ENDPOINT_URL",
        default_value = Self::default_s3_host(),
    )]
    pub s3_endpoint: Url,

    /// S3 region name. Needed for AWS S3.
    #[arg(
        long,
        env = "AWS_REGION",
        default_value = Self::default_s3_region(),
    )]
    pub s3_region: String,

    /// S3 secret key.
    #[arg(long, env = "AWS_SECRET_ACCESS_KEY")]
    pub s3_secret_key: String,
}

impl DatasetCatalog {
    #[allow(clippy::identity_op)]
    #[inline]
    pub const fn default_max_buffer_size() -> u64 {
        1 * 1024 * 1024 * 1024 // 1 GB
    }

    #[allow(clippy::identity_op)]
    #[inline]
    pub const fn default_max_chunk_size() -> u64 {
        256 * 1024 // 256 KB
    }

    #[inline]
    pub const fn default_s3_host() -> &'static str {
        "http://object-storage"
    }

    #[inline]
    pub const fn default_s3_region() -> &'static str {
        "auto"
    }
}

impl DatasetCatalog {
    pub fn init(&self) {
        info!("Registering store: S3");
        ::deltalake::aws::register_handlers(Some(self.s3_endpoint.0.clone()))
    }

    pub fn storage_options(&self) -> HashMap<String, String> {
        let allow_http = self.s3_endpoint.scheme() == "http";

        let mut options = HashMap::default();
        options.insert("allow_http".into(), allow_http.to_string());
        options.insert("AWS_ACCESS_KEY_ID".into(), self.s3_access_key.clone());
        options.insert("AWS_ALLOW_HTTP".into(), allow_http.to_string());
        options.insert("AWS_EC2_METADATA_DISABLED".into(), true.to_string());
        options.insert("AWS_ENDPOINT_URL".into(), self.s3_endpoint.to_string());
        options.insert("AWS_REGION".into(), self.s3_region.clone());
        options.insert("AWS_SECRET_ACCESS_KEY".into(), self.s3_secret_key.clone());
        options.insert("conditional_put".into(), "etag".into());
        options
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
