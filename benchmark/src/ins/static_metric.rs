use anyhow::Result;
use async_trait::async_trait;
use kube::Client;
use serde_json::Value;
use tracing::{instrument, Level};

use crate::args::CommonArgs;

use super::Metrics;

#[derive(Clone, Debug)]
pub struct Instruction {
    pub key: &'static str,
    pub value: Value,
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, _kube: &Client, _args: &CommonArgs, metrics: &mut Metrics) -> Result<()> {
        let Self { key, value } = self.clone();
        metrics.write(key.into(), value);
        Ok(())
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn delete(
        &self,
        _kube: &Client,
        _args: &CommonArgs,
        _metrics: &mut Metrics,
    ) -> Result<()> {
        Ok(())
    }
}
