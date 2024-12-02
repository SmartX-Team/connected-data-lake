use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use tokio::fs;

use crate::ins::InstructionStack;

#[derive(Clone, Debug, PartialEq, Parser)]
pub struct Args {
    #[command(subcommand)]
    pub command: crate::command::Command,

    #[command(flatten)]
    pub common: CommonArgs,

    #[arg(long, default_value = "./outputs")]
    pub output_dir: PathBuf,
}

impl Args {
    pub(super) async fn execute(self) -> Result<()> {
        let mut stack = InstructionStack::try_new(self.common).await?;

        let mut error = None;
        for ins in self.command.to_instructions().await? {
            match stack.push(ins).await {
                Ok(()) => continue,
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }

        let value = stack.cleanup().await?;
        let path = {
            let mut path = self.output_dir;
            fs::create_dir_all(&path).await?;

            let now = Utc::now().to_rfc3339().replace(":", "-");
            path.push(format!("{now}.json"));
            path
        };
        fs::write(&path, ::serde_json::to_string_pretty(&value)?).await?;

        match error {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Parser)]
pub struct CommonArgs {
    #[arg(long, default_value_t = 1000)]
    pub check_interval_ms: u64,

    #[arg(long)]
    pub connected: bool,

    #[arg(long, default_value_t = 16)]
    pub num_threads: usize,
}
