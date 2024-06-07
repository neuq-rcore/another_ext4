#![feature(trait_upcasting)]

mod block_dev;
mod common;
mod fuse_fs;

use block_dev::BlockMem;
use fuse_fs::StateExt4FuseFs;
use fuser::MountOption;
use log::{error, info};
use simple_logger::SimpleLogger;
use std::sync::Arc;

fn main() {
    SimpleLogger::new().init().unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    // Get mountpoint
    let mp = match option_env!("MP") {
        Some(mp) => mp,
        _ => panic!("No mount point specified!"),
    };
    info!("Use mountpoint \"{}\"", mp);

    // Initialize block device and filesystem
    let block_mem = Arc::new(BlockMem::new(512));
    block_mem.mkfs();
    let fs = StateExt4FuseFs::new(block_mem);

    // Mount fs and enter session loop
    let options = Vec::<MountOption>::new();
    info!("Mount ext4fs to \"{}\"", mp);
    let res = fuser::mount2(fs, &mp, &options);
    if let Err(e) = res {
        error!("Error occured: {:?}", e);
    }
}
