// pub mod branch;
pub mod checkout_context;
pub mod create_datasets;
pub mod create_ponds;
pub mod create_syncs;
pub mod elapsed_time;
pub mod static_metric;

use std::{mem, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use kube::Client;
use serde_json::{Map, Value};
use tokio::sync::Mutex;

use crate::args::CommonArgs;

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    inner: Arc<Mutex<Map<String, Value>>>,
}

impl Metrics {
    pub async fn write(&self, key: String, value: impl Into<Value>) {
        self.inner.lock().await.insert(key, value.into());
    }
}

#[async_trait]
pub trait Instruction
where
    Self: Send + Sync,
{
    async fn apply(&self, stack: &mut InstructionStack) -> Result<()>;

    async fn delete(&self, stack: &mut InstructionStack) -> Result<()>;
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

    // async fn branch(&self) -> Self {
    //     Self {
    //         args: self.args.clone(),
    //         inner: Vec::default(),
    //         kube: self.kube.clone(),
    //         metrics: self.metrics.clone(),
    //     }
    // }

    async fn push(&mut self, ins: Box<dyn Instruction>) -> Result<()> {
        let result = ins.apply(self).await;
        self.inner.push(ins);
        result
    }

    pub async fn run(&mut self, prog: Vec<Box<dyn Instruction>>) -> Result<()> {
        for ins in prog {
            self.push(ins).await?;
        }
        Ok(())
    }

    pub async fn cleanup(mut self) -> Result<Value> {
        let mut ins = Vec::default();
        mem::swap(&mut ins, &mut self.inner);

        for ins in ins.into_iter().rev() {
            ins.delete(&mut self).await?;
        }
        Ok(Value::Object(self.metrics.inner.lock().await.clone()))
    }
}
