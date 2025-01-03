use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::{info, instrument, Level};

use super::InstructionStack;

#[derive(Clone, Debug)]
pub struct Instruction {
    pub key: &'static str,
    pub value: Value,
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, stack: &mut InstructionStack) -> Result<()> {
        let Self { key, value } = self.clone();
        let InstructionStack { metrics, .. } = stack;
        info!("static_metric: {key}={value:?}");
        metrics.write(key.into(), value).await;
        Ok(())
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn delete(&self, _stack: &mut InstructionStack) -> Result<()> {
        Ok(())
    }
}
