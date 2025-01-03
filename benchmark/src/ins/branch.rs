use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{info, instrument, Level};

use super::InstructionStack;

pub struct Instruction {
    pub prog: Vec<Box<dyn super::Instruction>>,
    stack: Mutex<Option<InstructionStack>>,
}

impl Instruction {
    pub fn new(prog: Vec<Box<dyn super::Instruction>>) -> Self {
        Self {
            prog,
            stack: Default::default(),
        }
    }
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, parent: &mut InstructionStack) -> Result<()> {
        let Self { prog, stack } = self;
        let mut stack = stack.lock().await;
        stack.replace(parent.branch().await);
        let stack = stack.as_mut().unwrap();
        info!("branch");
        for ins in prog {
            ins.apply(stack).await?;
        }
        Ok(())
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn delete(&self, _parent: &mut InstructionStack) -> Result<()> {
        let Self { prog, stack } = self;
        let mut stack = match stack.lock().await.take() {
            Some(stack) => stack,
            None => return Ok(()),
        };
        for ins in prog.iter().rev() {
            ins.delete(&mut stack).await?;
        }
        Ok(())
    }
}
