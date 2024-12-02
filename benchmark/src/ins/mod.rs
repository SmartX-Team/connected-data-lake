pub mod create_datasets;
pub mod create_ponds;

use anyhow::Result;
use async_trait::async_trait;
use kube::Client;

#[async_trait]
pub trait Instruction
where
    Self: Send,
{
    async fn apply(&self, kube: &Client) -> Result<()>;

    async fn delete(&self, kube: &Client) -> Result<()>;
}

pub struct InstructionStack {
    inner: Vec<Box<dyn Instruction>>,
    kube: Client,
}

impl InstructionStack {
    pub async fn try_default() -> Result<Self> {
        Ok(Self {
            inner: Vec::default(),
            kube: Client::try_default().await?,
        })
    }

    pub async fn push(&mut self, ins: Box<dyn Instruction>) -> Result<()> {
        self.inner.push(ins);
        self.inner.last().unwrap().apply(&self.kube).await?;
        Ok(())
    }

    pub async fn cleanup(self) -> Result<()> {
        for ins in self.inner.into_iter().rev() {
            ins.delete(&self.kube).await?;
        }
        Ok(())
    }
}
