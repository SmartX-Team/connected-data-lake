use anyhow::Result;
use clap::{Parser, ValueEnum};
use serde_json::Value;
use tracing::instrument;

use crate::ins::{self, Instruction};

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
    pub(super) async fn to_instructions(self) -> Result<Vec<Box<dyn Instruction>>> {
        let Self { ty, num_k } = self;

        match ty {
            CreateType::Dataset => Ok(vec![
                Box::new(ins::static_metric::Instruction {
                    key: "kind",
                    value: Value::String("create_datasets".into()),
                }),
                Box::new(ins::static_metric::Instruction {
                    key: "num_datasets",
                    value: Value::Number(num_k.into()),
                }),
                Box::new(ins::static_metric::Instruction {
                    key: "num_ponds",
                    value: Value::Number(1usize.into()),
                }),
                Box::new(ins::create_ponds::Instruction { num_k: 1 }),
                Box::new(ins::elapsed_time::Instruction {
                    label: "create_datasets",
                }),
                Box::new(ins::create_datasets::Instruction { num_k }),
            ]),
            CreateType::Pond => Ok(vec![
                Box::new(ins::static_metric::Instruction {
                    key: "kind",
                    value: Value::String("create_ponds".into()),
                }),
                Box::new(ins::static_metric::Instruction {
                    key: "num_ponds",
                    value: Value::Number(num_k.into()),
                }),
                Box::new(ins::elapsed_time::Instruction {
                    label: "create_ponds",
                }),
                Box::new(ins::create_ponds::Instruction { num_k }),
            ]),
        }
    }
}
