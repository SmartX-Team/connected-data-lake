pub mod create;

use anyhow::Result;
use clap::Subcommand;

use crate::{args::CommonArgs, ins::Instruction};

#[derive(Clone, Debug, PartialEq, Subcommand)]
pub enum Command {
    Create(self::create::CreateArgs),
}

impl Command {
    pub(super) async fn to_instructions(
        self,
        common: CommonArgs,
    ) -> Result<Vec<Box<dyn Instruction>>> {
        match self {
            Self::Create(args) => args.to_instructions(common).await,
        }
    }
}
