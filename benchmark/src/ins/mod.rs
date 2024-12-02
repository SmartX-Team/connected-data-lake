pub mod create_datasets;
pub mod create_ponds;
pub mod elapsed_time;
pub mod static_metric;

use anyhow::Result;
use async_trait::async_trait;
use kube::Client;
use serde_json::{Map, Value};

use crate::args::CommonArgs;

#[derive(Debug, Default)]
pub struct Metrics {
    inner: Map<String, Value>,
}

impl Metrics {
    pub fn write(&mut self, key: String, value: impl Into<Value>) {
        self.inner.insert(key, value.into());
    }
}

#[async_trait]
pub trait Instruction
where
    Self: Send,
{
    async fn apply(&self, kube: &Client, args: &CommonArgs, metrics: &mut Metrics) -> Result<()>;

    async fn delete(&self, kube: &Client, args: &CommonArgs, metrics: &mut Metrics) -> Result<()>;
}

pub struct InstructionStack {
    args: CommonArgs,
    inner: Vec<Box<dyn Instruction>>,
    kube: Client,
    metrics: Metrics,
}

impl InstructionStack {
    pub async fn try_new(args: CommonArgs) -> Result<Self> {
        Ok(Self {
            args,
            inner: Vec::default(),
            kube: Client::try_default().await?,
            metrics: Metrics::default(),
        })
    }

    pub async fn push(&mut self, ins: Box<dyn Instruction>) -> Result<()> {
        self.inner.push(ins);
        self.inner
            .last()
            .unwrap()
            .apply(&self.kube, &self.args, &mut self.metrics)
            .await?;
        Ok(())
    }

    pub async fn cleanup(mut self) -> Result<Value> {
        for ins in self.inner.into_iter().rev() {
            ins.delete(&self.kube, &self.args, &mut self.metrics)
                .await?;
        }
        Ok(Value::Object(self.metrics.inner))
    }
}
