use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use tracing::{info, instrument, Level};

use super::InstructionStack;

#[derive(Clone, Debug)]
pub struct Instruction {
    pub label: &'static str,
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, stack: &mut InstructionStack) -> Result<()> {
        let Self { label } = self;
        let InstructionStack { metrics, .. } = stack;
        info!("elapsed_time: {label}_timestamp_begin");
        metrics
            .write(
                format!("{label}_timestamp_begin"),
                Utc::now().timestamp_micros(),
            )
            .await;
        Ok(())
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn delete(&self, stack: &mut InstructionStack) -> Result<()> {
        let Self { label } = self;
        let InstructionStack { metrics, .. } = stack;
        info!("elapsed_time: {label}_timestamp_end");
        metrics
            .write(
                format!("{label}_timestamp_end"),
                Utc::now().timestamp_micros(),
            )
            .await;
        Ok(())
    }
}
