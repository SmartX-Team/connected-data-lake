use std::{collections::HashMap, fmt};

use clap::{Parser, ValueEnum};
use deltalake::{
    parquet::{basic, file::properties::EnabledStatistics},
    DeltaResult,
};
use tracing::info;
use url::Url;

#[derive(Clone, Debug, PartialEq, Parser)]
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
    #[inline]
    pub const fn default_max_buffer_size() -> u64 {
        1 * 1024 * 1024 * 1024 // 1 GB
    }

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
        ::deltalake::aws::register_handlers(Some(self.s3_endpoint.clone()))
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

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, ValueEnum)]
#[allow(non_camel_case_types)]
pub enum Compression {
    BROTLI,
    GZIP,
    LZO,
    LZ4,
    LZ4_RAW,
    #[default]
    SNAPPY,
    UNCOMPRESSED,
    ZSTD,
}

impl fmt::Display for Compression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BROTLI => "brotli".fmt(f),
            Self::GZIP => "gzip".fmt(f),
            Self::LZO => "lzo".fmt(f),
            Self::LZ4 => "lz4".fmt(f),
            Self::LZ4_RAW => "lz4-raw".fmt(f),
            Self::SNAPPY => "snappy".fmt(f),
            Self::UNCOMPRESSED => "none".fmt(f),
            Self::ZSTD => "zstd".fmt(f),
        }
    }
}
