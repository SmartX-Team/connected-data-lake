use std::mem;

use anyhow::Result;
use async_trait::async_trait;
use kube::{config::KubeConfigOptions, Client, Config};
use tokio::sync::Mutex;
use tracing::{info, instrument, Level};

use super::InstructionStack;

pub struct Instruction {
    name: String,
    last: Mutex<Option<Client>>,
}

impl Instruction {
    pub fn new(name: String) -> Self {
        Self {
            name,
            last: Default::default(),
        }
    }
}

#[async_trait]
impl super::Instruction for Instruction {
    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn apply(&self, stack: &mut InstructionStack) -> Result<()> {
        let Self { name, last } = self;
        info!("checkout_context: {name}");

        let options = KubeConfigOptions {
            context: Some(name.clone()),
            ..Default::default()
        };
        let config = Config::from_kubeconfig(&options).await?;
        let mut kube = Client::try_from(config)?;
        mem::swap(&mut kube, &mut stack.kube);
        last.lock().await.replace(kube);
        Ok(())
    }

    #[instrument(skip_all, err(level = Level::ERROR))]
    async fn delete(&self, stack: &mut InstructionStack) -> Result<()> {
        let Self { last, .. } = self;
        let kube = match last.lock().await.take() {
            Some(kube) => kube,
            None => return Ok(()),
        };
        stack.kube = kube;
        Ok(())
    }
}
