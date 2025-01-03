pub mod create;
pub mod sync;

use anyhow::Result;
use clap::Subcommand;

use crate::ins::Instruction;

#[derive(Clone, Debug, PartialEq, Subcommand)]
pub enum Command {
    Create(self::create::CreateArgs),
    Sync(self::sync::SyncArgs),
}

impl Command {
    pub(super) async fn to_instructions(self) -> Result<Vec<Box<dyn Instruction>>> {
        match self {
            Self::Create(args) => args.to_instructions().await,
            Self::Sync(args) => args.to_instructions().await,
        }
    }
}
