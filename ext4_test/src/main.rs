use ext4_rs::{Ext4, InodeMode, OpenFlags, EXT4_ROOT_INO};
use simple_logger::SimpleLogger;
use std::sync::Arc;
use block_file::BlockFile;

mod block_file;

const ROOT_INO: u32 = EXT4_ROOT_INO;

fn make_ext4() {
    let _ = std::process::Command::new("rm")
        .args(["-rf", "ext4.img"])
        .status();
    let _ = std::process::Command::new("dd")
        .args(["if=/dev/zero", "of=ext4.img", "bs=1M", "count=512"])
        .status();
    let _ = std::process::Command::new("mkfs.ext4")
        .args(["ext4.img"])
        .output();
}

fn open_ext4() -> Ext4 {
    let file = BlockFile::new("ext4.img");
    println!("creating ext4");
    Ext4::load(Arc::new(file)).expect("open ext4 failed")
}

fn mkdir_test(ext4: &mut Ext4) {
    let dir_mode: InodeMode = InodeMode::DIRECTORY | InodeMode::ALL_RWX;
    ext4.generic_create(ROOT_INO, "d1", dir_mode)
        .expect("mkdir failed");
    ext4.generic_create(ROOT_INO, "d1/d2", dir_mode)
        .expect("mkdir failed");
    ext4.generic_create(ROOT_INO, "d1/d2/d3", dir_mode)
        .expect("mkdir failed");
    ext4.generic_create(ROOT_INO, "d1/d2/d3/d4", dir_mode)
        .expect("mkdir failed");
    ext4.generic_create(ROOT_INO, "d2", dir_mode)
        .expect("mkdir failed");
    ext4.generic_create(ROOT_INO, "d2/d3", dir_mode)
        .expect("mkdir failed");
    ext4.generic_create(ROOT_INO, "d2/d3/d4", dir_mode)
        .expect("mkdir failed");
    ext4.generic_create(ROOT_INO, "d3", dir_mode)
        .expect("mkdir failed");
}

fn create_test(ext4: &mut Ext4) {
    let file_mode: InodeMode = InodeMode::FILE | InodeMode::ALL_RWX;
    ext4.generic_create(ROOT_INO, "d1/d2/d3/d4/f1", file_mode)
        .expect("open failed");
    ext4.generic_create(ROOT_INO, "d3/f0", file_mode)
        .expect("open failed");
    ext4.generic_create(ROOT_INO, "d3/f1", file_mode)
        .expect("open failed");
    ext4.generic_create(ROOT_INO, "f1", file_mode)
        .expect("open failed");
}

fn read_write_test(ext4: &mut Ext4) {
    let wbuffer = "hello world".as_bytes();
    let wfile = ext4
        .generic_open(ROOT_INO, "d3/f0", OpenFlags::O_WRONLY)
        .expect("open failed");
    ext4.write(wfile.inode, 0, wbuffer).expect("write failed");

    let mut rbuffer = vec![0u8; wbuffer.len() + 100]; // Test end of file
    let rfile = ext4
        .generic_open(ROOT_INO, "d3/f0", OpenFlags::O_RDONLY)
        .expect("open failed");
    let rcount = ext4.read(rfile.inode, 0, &mut rbuffer).expect("read failed");

    assert_eq!(wbuffer, &rbuffer[..rcount]);
}

fn large_read_write_test(ext4: &mut Ext4) {
    let wbuffer = vec![99u8; 1024 * 1024 * 16];
    let wfile = ext4
        .generic_open(ROOT_INO, "d3/f1", OpenFlags::O_WRONLY)
        .expect("open failed");
    ext4.write(wfile.inode, 0, &wbuffer).expect("write failed");

    let rfile = ext4
        .generic_open(ROOT_INO, "d3/f1", OpenFlags::O_RDONLY)
        .expect("open failed");
    let mut rbuffer = vec![0u8; wbuffer.len()];
    let rcount = ext4.read(rfile.inode, 0,&mut rbuffer).expect("read failed");

    assert_eq!(wbuffer, &rbuffer[..rcount]);
}

fn remove_file_test(ext4: &mut Ext4) {
    ext4.generic_remove(ROOT_INO, "d3/f0")
        .expect("remove file failed");
    ext4.generic_lookup(ROOT_INO, "d3/f0")
        .expect_err("file not removed");
    ext4.generic_remove(ROOT_INO, "d3/f1")
        .expect("remove file failed");
    ext4.generic_lookup(ROOT_INO, "d3/f1")
        .expect_err("file not removed");
    ext4.generic_remove(ROOT_INO, "f1")
        .expect("remove file failed");
    ext4.generic_lookup(ROOT_INO, "f1")
        .expect_err("file not removed");
    ext4.generic_remove(ROOT_INO, "d1/not_exist")
        .expect_err("remove file failed");
}

fn remove_dir_test(ext4: &mut Ext4) {
    ext4.generic_remove(ROOT_INO, "d2")
        .expect_err("remove unempty dir");
    ext4.generic_create(ROOT_INO, "dtmp", InodeMode::DIRECTORY | InodeMode::ALL_RWX)
        .expect("mkdir failed");
    ext4.generic_lookup(ROOT_INO, "dtmp")
        .expect("dir not created");
    ext4.generic_remove(ROOT_INO, "dtmp")
        .expect("remove file failed");
    ext4.generic_lookup(ROOT_INO, "dtmp")
        .expect_err("dir not removed");
}

fn main() {
    SimpleLogger::new().init().unwrap();
    log::set_max_level(log::LevelFilter::Off);
    make_ext4();
    println!("ext4.img created");
    let mut ext4 = open_ext4();
    println!("ext4 opened");
    mkdir_test(&mut ext4);
    println!("mkdir test done");
    create_test(&mut ext4);
    println!("create test done");
    read_write_test(&mut ext4);
    println!("read write test done");
    large_read_write_test(&mut ext4);
    println!("large read write test done");
    remove_file_test(&mut ext4);
    println!("remove file test done");
    remove_dir_test(&mut ext4);
    println!("remove dir test done");
}
