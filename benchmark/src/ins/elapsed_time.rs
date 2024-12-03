use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use kube::Client;
use tracing::{info, instrument, Level};

use crate::args::CommonArgs;

use super::Metrics;

#[derive(Clone, Debug)]
pub struct Instruction {
    pub label: &'static str,
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, _kube: &Client, _args: &CommonArgs, metrics: &mut Metrics) -> Result<()> {
        let Self { label } = self;
        info!("elapsed_time: {label}_timestamp_begin");
        metrics.write(
            format!("{label}_timestamp_begin"),
            Utc::now().timestamp_micros(),
        );
        Ok(())
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn delete(
        &self,
        _kube: &Client,
        _args: &CommonArgs,
        metrics: &mut Metrics,
    ) -> Result<()> {
        let Self { label } = self;
        info!("elapsed_time: {label}_timestamp_end");
        metrics.write(
            format!("{label}_timestamp_end"),
            Utc::now().timestamp_micros(),
        );
        Ok(())
    }
}
