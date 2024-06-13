#![feature(trait_upcasting)]

mod block_dev;
mod common;
mod fuse_fs;

use block_dev::BlockMem;
use clap::Parser;
use fuse_fs::StateExt4FuseFs;
use fuser::MountOption;
use log::{error, info};
use simple_logger::SimpleLogger;
use std::sync::Arc;

#[derive(Parser, Debug)]
struct Args {
    /// Fs mount point
    #[arg(short, long)]
    mountpoint: String,

    /// Fs block count
    #[arg(short, long, default_value_t = 4096)]
    block: u64,
}

fn main() {
    let args = Args::parse();

    SimpleLogger::new().init().unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    info!("Use mountpoint \"{}\"", args.mountpoint);

    // Initialize block device and filesystem
    let block_mem = Arc::new(BlockMem::new(args.block));
    block_mem.mkfs();
    let fs = StateExt4FuseFs::new(block_mem);

    // Mount fs and enter session loop
    let options = Vec::<MountOption>::new();
    info!("Mount ext4fs to \"{}\"", args.mountpoint);
    let res = fuser::mount2(fs, &args.mountpoint, &options);
    if let Err(e) = res {
        error!("Error occured: {:?}", e);
    }
}
