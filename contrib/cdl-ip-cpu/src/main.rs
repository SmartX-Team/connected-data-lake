mod args;

use cdl_ip_core::task::{TaskBuilder, TaskExt};
use cdl_ip_linux_io_uring::{
    node::file::{ReadFile, WriteFile},
    task::LocalTaskBuilder,
};
use clap::Parser;
use tracing::info;

fn main() {
    ::cdl_k8s_core::otel::init_once();
    info!("Welcome to Connected Data Lake - CPU IP!");

    let args = self::args::Args::parse();

    let input = ReadFile::new("/tmp/bigfile").unwrap();
    let output = WriteFile::new("/tmp/bigfile-copied").unwrap();

    let ex = LocalTaskBuilder::default();
    ex.open(input.try_into().unwrap())
        .wait(output.try_into().unwrap())
        .unwrap();
}
