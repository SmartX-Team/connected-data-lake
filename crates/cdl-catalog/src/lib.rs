use std::{collections::HashMap, fmt, ops, str::FromStr, sync::Arc};

use anyhow::{bail, Context, Result};
use clap::Parser;
use lance::{
    dataset::progress::{NoopFragmentWriteProgress, WriteFragmentProgress},
    io::ObjectStoreParams,
};
use lance_table::io::commit::{CommitHandler, UnsafeCommitHandler};
use object_store::{
    aws::{AwsCredential, AwsCredentialProvider},
    StaticCredentialProvider,
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

macro_rules! get_arg {
    ( $catalog:tt, $name:ident ) => {{
        get_arg($catalog.$name.as_ref(), stringify!($name))
    }};
}

fn get_arg<T>(arg: Option<&T>, name: &'static str) -> Result<T>
where
    T: Clone,
{
    arg.cloned()
        .with_context(|| format!("Missing catalog config: {name}"))
}

#[derive(Clone, Debug, PartialEq, Parser)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub struct DatasetCatalog {
    /// Max directory size for cache directory.
    #[arg(
        global=true, long,
        env = "CDL_CACHE_DIR",
        default_value_t = Self::default_cache_dir(),
    )]
    #[cfg_attr(
        feature = "serde",
        serde(default = "DatasetCatalog::default_cache_dir")
    )]
    pub cache_dir: String,

    /// Max file size for each batch file.
    /// The larger the value, the faster the data transfer speed.
    /// It is recommended to use the largest possible value
    /// that is supported simultaneously by multiple backend storages.
    #[arg(
        global=true, long,
        env = "CDL_MAX_BUFFER_SIZE",
        default_value_t = Self::default_max_buffer_size(),
    )]
    #[cfg_attr(
        feature = "serde",
        serde(default = "DatasetCatalog::default_max_buffer_size")
    )]
    pub max_buffer_size: usize,

    /// Max directory size for cache directory.
    /// The value 0 disables the caching.
    #[arg(
        global=true, long,
        env = "CDL_MAX_CACHE_SIZE",
        default_value_t = Self::default_max_cache_size(),
    )]
    #[cfg_attr(
        feature = "serde",
        serde(default = "DatasetCatalog::default_max_cache_size")
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
    #[cfg_attr(
        feature = "serde",
        serde(default = "DatasetCatalog::default_max_chunk_size")
    )]
    pub max_chunk_size: u64,

    /// Maximum number of threads for writing files.
    #[arg(
        global=true, long,
        env = "CDL_MAX_WRITE_THREADS",
        default_value_t = Self::default_max_write_threads(),
    )]
    #[cfg_attr(
        feature = "serde",
        serde(default = "DatasetCatalog::default_max_write_threads")
    )]
    pub max_write_threads: usize,

    /// Min object size for caching larege files.
    #[arg(
        global=true, long,
        env = "CDL_MIN_CACHE_OBJECT_SIZE",
        default_value_t = Self::default_min_cache_object_size(),
    )]
    #[cfg_attr(
        feature = "serde",
        serde(default = "DatasetCatalog::default_min_cache_object_size")
    )]
    pub min_cache_object_size: usize,

    /// S3 access key.
    #[arg(global = true, long, env = "AWS_ACCESS_KEY_ID")]
    #[cfg_attr(feature = "serde", serde(default))]
    pub s3_access_key: Option<String>,

    /// S3 region name.
    #[arg(
        global=true, long,
        env = "AWS_ENDPOINT_URL",
        default_value_t = Self::default_s3_endpoint(),
    )]
    #[cfg_attr(
        feature = "serde",
        serde(default = "DatasetCatalog::default_s3_endpoint")
    )]
    pub s3_endpoint: Url,

    /// S3 region name. Needed for AWS S3.
    #[arg(
        global=true, long,
        env = "AWS_REGION",
        default_value_t = Self::default_s3_region(),
    )]
    #[cfg_attr(
        feature = "serde",
        serde(default = "DatasetCatalog::default_s3_region")
    )]
    pub s3_region: String,

    /// S3 secret key.
    #[arg(global = true, long, env = "AWS_SECRET_ACCESS_KEY")]
    #[cfg_attr(feature = "serde", serde(default))]
    pub s3_secret_key: Option<String>,
}

impl Default for DatasetCatalog {
    fn default() -> Self {
        Self {
            cache_dir: Self::default_cache_dir(),
            max_buffer_size: Self::default_max_buffer_size(),
            max_cache_size: Self::default_max_cache_size(),
            max_chunk_size: Self::default_max_chunk_size(),
            max_write_threads: Self::default_max_write_threads(),
            min_cache_object_size: Self::default_min_cache_object_size(),
            s3_access_key: None,
            s3_endpoint: Self::default_s3_endpoint(),
            s3_region: Self::default_s3_region(),
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
    pub const fn default_max_buffer_size() -> usize {
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
        // 256 * 1024 // 256 KiB
        0
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

    pub fn default_s3_endpoint() -> Url {
        "http://object-storage"
            .parse()
            .expect("Invalid fallback s3 endpoint")
    }

    pub fn default_s3_region() -> String {
        "auto".into()
    }

    pub fn merge(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "cache_dir" => self.cache_dir = value.into(),
            "max_buffer_size" => self.max_buffer_size = value.parse()?,
            "max_cache_size" => self.max_cache_size = value.parse()?,
            "max_chunk_size" => self.max_chunk_size = value.parse()?,
            "max_write_threads" => self.max_write_threads = value.parse()?,
            "min_cache_object_size" => self.min_cache_object_size = value.parse()?,
            "s3_access_key" => self.s3_access_key = Some(value.into()),
            "s3_endpoint" => self.s3_endpoint = value.parse()?,
            "s3_region" => self.s3_region = value.into(),
            "s3_secret_key" => self.s3_secret_key = Some(value.into()),
            _ => bail!("Invalid key: {key:?}"),
        }
        Ok(())
    }

    pub fn merge_iter<'a>(
        &mut self,
        mut iter: impl Iterator<Item = (&'a str, &'a str)>,
    ) -> Result<()> {
        iter.try_for_each(|(key, value)| self.merge(key, value))
    }
}

impl DatasetCatalog {
    pub const KEY_CACHE_DIR: &'static str = "CDL_CACHE_DIR";
    pub const KEY_MAX_CACHE_SIZE: &'static str = "CDL_MAX_CACHE_SIZE";
    pub const KEY_MIN_CACHE_OBJECT_SIZE: &'static str = "CDL_MIN_CACHE_OBJECT_SIZE";

    pub fn commit_handler(&self) -> Arc<dyn CommitHandler> {
        Arc::new(UnsafeCommitHandler)
    }

    pub fn fragment_process(&self) -> Arc<dyn WriteFragmentProgress> {
        Arc::new(NoopFragmentWriteProgress::default())
    }

    pub fn s3_credential_provider(&self) -> Result<AwsCredentialProvider> {
        Ok(Arc::new(StaticCredentialProvider::new(AwsCredential {
            key_id: get_arg!(self, s3_access_key)?,
            secret_key: get_arg!(self, s3_secret_key)?,
            token: None,
        })))
    }

    pub fn storage_options(&self, append_credentials: bool) -> Result<HashMap<String, String>> {
        let allow_http = self.s3_endpoint.scheme() == "http";
        let mut endpoint = self.s3_endpoint.to_string();
        while endpoint.ends_with("/") {
            endpoint = endpoint[..endpoint.len() - 1].to_string();
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
        if append_credentials {
            options.insert("AWS_ACCESS_KEY_ID".into(), get_arg!(self, s3_access_key)?);
        }
        options.insert("AWS_ALLOW_HTTP".into(), allow_http.to_string());
        options.insert("AWS_EC2_METADATA_DISABLED".into(), true.to_string());
        options.insert("AWS_ENDPOINT_URL".into(), endpoint);
        options.insert("AWS_REGION".into(), self.s3_region.clone());
        if append_credentials {
            options.insert(
                "AWS_SECRET_ACCESS_KEY".into(),
                get_arg!(self, s3_secret_key)?,
            );
        }
        options.insert("AWS_VIRTUAL_HOSTED_STYLE_REQUEST".into(), "false".into());
        options.insert("conditional_put".into(), "etag".into());
        Ok(options)
    }

    pub fn storage_parameters(&self) -> Result<ObjectStoreParams> {
        Ok(ObjectStoreParams {
            aws_credentials: Some(self.s3_credential_provider()?),
            list_is_lexically_ordered: Some(true),
            storage_options: Some(self.storage_options(false)?),
            use_constant_size_upload_parts: false,
            ..Default::default()
        })
    }

    #[inline]
    pub const fn enable_statistics(&self) -> bool {
        false
    }
}

#[allow(non_camel_case_types)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
