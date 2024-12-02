use anyhow::Result;
use clap::Parser;

use crate::ins::InstructionStack;

#[derive(Clone, Debug, PartialEq, Parser)]
pub struct Args {
    #[command(subcommand)]
    pub command: crate::command::Command,

    #[command(flatten)]
    pub common: CommonArgs,
}

impl Args {
    pub(super) async fn execute(self) -> Result<()> {
        let mut stack = InstructionStack::try_default().await?;

        let mut error = None;
        for ins in self.command.to_instructions(self.common).await? {
            match stack.push(ins).await {
                Ok(()) => continue,
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }

        stack.cleanup().await?;
        match error {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Parser)]
pub struct CommonArgs {
    #[arg(long)]
    pub connected: bool,
}
