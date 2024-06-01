mod block_file;
mod common;
mod fuse_fs;

use block_file::BlockFile;
use ext4_rs::Ext4;
use fuse_fs::Ext4FuseFs;
use fuser::MountOption;
use log::{error, info};
use simple_logger::SimpleLogger;
use std::sync::Arc;

fn make_ext4(path: &str) {
    let _ = std::process::Command::new("rm")
        .args(["-rf", path])
        .status();
    let _ = std::process::Command::new("dd")
        .args([
            "if=/dev/zero",
            &format!("of={}", path),
            "bs=1M",
            "count=512",
        ])
        .status();
    let _ = std::process::Command::new("mkfs.ext4")
        .args([path])
        .output();
}

fn main() {
    SimpleLogger::new().init().unwrap();
    log::set_max_level(log::LevelFilter::Debug);

    // Get block file
    let bf = match option_env!("BF") {
        Some(bf) => bf,
        _ => panic!("No block file specified!"),
    };
    info!("Use block file \"{}\"", bf);
    make_ext4(bf);

    // Get mountpoint
    let mp = match option_env!("MP") {
        Some(mp) => mp,
        _ => panic!("No mount point specified!"),
    };
    info!("Use mountpoint \"{}\"", mp);

    // Initialize block device and filesystem
    let block_file = Arc::new(BlockFile::new(bf));
    let fs = Ext4FuseFs::new(Ext4::load(block_file).expect("Load Ext4 filesystem failed"));

    // Mount fs and enter session loop
    let options = Vec::<MountOption>::new();
    info!("Mount ext4fs to \"{}\"", mp);
    let res = fuser::mount2(fs, &mp, &options);
    if let Err(e) = res {
        error!("Error occured: {:?}", e);
    }
}
