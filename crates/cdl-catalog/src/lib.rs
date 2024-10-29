use std::collections::HashMap;

use clap::Parser;
use tracing::info;
use url::Url;

#[derive(Clone, Debug, PartialEq, Parser)]
pub struct DatasetCatalog {
    #[arg(
        long,
        env = "CDL_MAX_CHUNK_SIZE",
        default_value_t = Self::default_max_chunk_size(),
    )]
    pub max_chunk_size: u64,

    #[arg(long, env = "AWS_ACCESS_KEY_ID")]
    pub s3_access_key: String,

    #[arg(
        long,
        env = "AWS_ENDPOINT_URL",
        default_value = Self::default_s3_host(),
    )]
    pub s3_endpoint: Url,

    #[arg(
        long,
        env = "AWS_REGION",
        default_value = Self::default_s3_region(),
    )]
    pub s3_region: String,

    #[arg(long, env = "AWS_SECRET_ACCESS_KEY")]
    pub s3_secret_key: String,
}

impl DatasetCatalog {
    #[inline]
    pub const fn default_max_chunk_size() -> u64 {
        8 * 1024 * 1024 // 8 MB
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
}
