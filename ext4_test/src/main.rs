use ext4_rs::{Block, BlockDevice, Ext4, BLOCK_SIZE};
use log::warn;
use simple_logger::SimpleLogger;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Arc;

#[derive(Debug)]
pub struct BlockFile(File);

impl BlockFile {
    pub fn new(path: &str) -> Self {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        Self(file)
    }
}

impl BlockDevice for BlockFile {
    fn read_block(&self, block_id: u64) -> Block {
        let mut file = &self.0;
        let mut buffer = [0u8; BLOCK_SIZE];
        // warn!("read_block {}", block_id);
        let _r = file.seek(SeekFrom::Start(block_id * BLOCK_SIZE as u64));
        let _r = file.read_exact(&mut buffer);
        Block::new(block_id, buffer)
    }

    fn write_block(&self, block: &Block) {
        let mut file = &self.0;
        // warn!("write_block {}", block.block_id);
        let _r = file.seek(SeekFrom::Start(block.block_id * BLOCK_SIZE as u64));
        let _r = file.write_all(&block.data);
    }
}

fn logger_init() {
    SimpleLogger::new().init().unwrap();
}

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
    ext4.mkdir("d1").expect("mkdir failed");
    ext4.mkdir("d1/d2").expect("mkdir failed");
    ext4.mkdir("d1/d2/d3").expect("mkdir failed");
    ext4.mkdir("d1/d2/d3/d4").expect("mkdir failed");
    ext4.mkdir("d2").expect("mkdir failed");
    ext4.mkdir("d2/d3").expect("mkdir failed");
    ext4.mkdir("d2/d3/d4").expect("mkdir failed");
    ext4.mkdir("d3").expect("mkdir failed");
}

fn open_test(ext4: &mut Ext4) {
    ext4.open("d1/d2/d3/d4/f1", "w+", true)
        .expect("open failed");
    ext4.open("d1/d2/d3/d4/f1", "r", true).expect("open failed");
    ext4.open("d1/d2/d3/d4/f5", "a", true).expect("open failed");
    ext4.open("d2/f4", "w+", true).expect("open failed");
    ext4.open("f1", "w+", true).expect("open failed");
}

fn read_write_test(ext4: &mut Ext4) {
    let wbuffer = "hello world".as_bytes();
    let mut wfile = ext4.open("d3/f0", "w+", true).expect("open failed");
    ext4.write(&mut wfile, wbuffer).expect("write failed");

    let mut rbuffer = vec![0u8; wbuffer.len()];
    let mut rfile = ext4.open("d3/f0", "r", true).expect("open failed");
    ext4.read(&mut rfile, &mut rbuffer, wbuffer.len())
        .expect("read failed");

    assert_eq!(wbuffer, rbuffer);
}

fn large_read_write_test(ext4: &mut Ext4) {
    let wbuffer = vec![99u8; 1024 * 1024 * 16];
    let mut wfile = ext4.open("d3/f1", "w+", true).expect("open failed");
    ext4.write(&mut wfile, &wbuffer).expect("write failed");

    let mut rfile = ext4.open("d3/f1", "r", true).expect("open failed");
    let mut rbuffer = vec![0u8; wbuffer.len()];
    ext4.read(&mut rfile, &mut rbuffer, wbuffer.len())
        .expect("read failed");

    assert_eq!(wbuffer, rbuffer);
}

fn remove_file_test(ext4: &mut Ext4) {
    ext4.remove_file("d3/f0").expect("remove file failed");
    ext4.open("d3/f0", "r", true).expect_err("open failed");
    ext4.remove_file("d3/f1").expect("remove file failed");
    ext4.open("d3/f1", "r", true).expect_err("open failed");
    ext4.remove_file("f1").expect("remove file failed");
    ext4.open("f1", "r", true).expect_err("open failed");
    ext4.remove_file("d1/not_exist").expect_err("remove file failed");
}

fn main() {
    logger_init();
    log::set_max_level(log::LevelFilter::Off);
    make_ext4();
    println!("ext4.img created");
    let mut ext4 = open_ext4();
    println!("ext4 opened");
    mkdir_test(&mut ext4);
    println!("mkdir test done");
    open_test(&mut ext4);
    println!("open test done");
    read_write_test(&mut ext4);
    println!("read write test done");
    large_read_write_test(&mut ext4);
    println!("large read write test done");
    log::set_max_level(log::LevelFilter::Debug);
    remove_file_test(&mut ext4);
    println!("remove file test done");
}
