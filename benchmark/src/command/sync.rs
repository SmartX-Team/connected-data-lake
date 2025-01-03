use std::net::Ipv4Addr;

use anyhow::Result;
use clap::Parser;
use serde_json::Value;
use tracing::instrument;

use crate::ins::{self, Instruction};

#[derive(Clone, Debug, PartialEq, Parser)]
pub struct SyncArgs {
    #[arg(long, default_value_t = 1)]
    pub num_k: usize,

    #[arg(long, env = "SOURCE_BUCKET_ADDRESS")]
    pub source_bucket_address: Ipv4Addr,
}

impl SyncArgs {
    #[instrument(skip_all)]
    pub(super) async fn to_instructions(self) -> Result<Vec<Box<dyn Instruction>>> {
        let Self {
            num_k,
            source_bucket_address,
        } = self;

        Ok(vec![
            Box::new(ins::static_metric::Instruction {
                key: "kind",
                value: Value::String("sync_datasets".into()),
            }),
            Box::new(ins::static_metric::Instruction {
                key: "num_datasets",
                value: Value::Number(num_k.into()),
            }),
            Box::new(ins::static_metric::Instruction {
                key: "num_ponds",
                value: Value::Number(1usize.into()),
            }),
            Box::new(ins::checkout_context::Instruction::new(
                "autodata-hot-storage-1@ops.autodata-hot-storage-1.openark".into(),
            )),
            Box::new(ins::create_ponds::Instruction {
                address: None,
                name: None,
                num_k: 1,
            }),
            Box::new(ins::create_datasets::Instruction { num_k }),
            Box::new(ins::checkout_context::Instruction::new(
                "autodata-hot-storage-2@ops.autodata-hot-storage-2.openark".into(),
            )),
            Box::new(ins::create_ponds::Instruction {
                address: None,
                name: None,
                num_k: 1,
            }),
            Box::new(ins::create_ponds::Instruction {
                address: Some(source_bucket_address),
                name: Some("remote".into()),
                num_k: 1,
            }),
            Box::new(ins::elapsed_time::Instruction {
                label: "sync_datasets",
            }),
            Box::new(ins::create_syncs::Instruction { num_k }),
        ])
    }
}
