use anyhow::Result;
use clap::{Parser, ValueEnum};
use tracing::instrument;

use crate::{
    args::CommonArgs,
    ins::{self, Instruction},
};

#[derive(Copy, Clone, Debug, PartialEq, ValueEnum)]
pub enum CreateType {
    Dataset,
    Pond,
}

#[derive(Clone, Debug, PartialEq, Parser)]
pub struct CreateArgs {
    pub ty: CreateType,

    #[arg(long, default_value_t = 1)]
    pub num_k: usize,
}

impl CreateArgs {
    #[instrument(skip_all)]
    pub(super) async fn to_instructions(
        self,
        common: CommonArgs,
    ) -> Result<Vec<Box<dyn Instruction>>> {
        let Self { ty, num_k } = self;

        match ty {
            CreateType::Dataset => Ok(vec![
                Box::new(ins::create_ponds::Instruction { num_k: 1 }),
                Box::new(ins::create_datasets::Instruction { num_k }),
            ]),
            CreateType::Pond => Ok(vec![Box::new(ins::create_ponds::Instruction { num_k })]),
        }
    }
}
