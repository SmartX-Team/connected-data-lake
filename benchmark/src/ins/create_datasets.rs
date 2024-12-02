use anyhow::Result;
use async_trait::async_trait;
use kube::Client;
use tracing::{instrument, Level};

#[derive(Debug)]
pub struct Instruction {
    pub num_k: usize,
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, kube: &Client) -> Result<()> {
        Ok(())
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn delete(&self, kube: &Client) -> Result<()> {
        Ok(())
    }
}
