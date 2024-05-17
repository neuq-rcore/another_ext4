use ext4_rs::{Block, BlockDevice, Ext4, BLOCK_SIZE};
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
        let _r = file.seek(SeekFrom::Start(block_id * BLOCK_SIZE as u64));
        let _r = file.read_exact(&mut buffer);
        Block::new(block_id, buffer)
    }

    fn write_block(&self, block: &Block) {
        let mut file = &self.0;
        let _r = file.seek(SeekFrom::Start(block.block_id * BLOCK_SIZE as u64));
        let _r = file.write_all(&block.data);
    }
}

fn logger_init() {
    SimpleLogger::new().init().unwrap();
    log::set_max_level(log::LevelFilter::Debug);
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
    ext4.mkdir("1").expect("mkdir failed");
    ext4.mkdir("1/2").expect("mkdir failed");
    ext4.mkdir("1/2/3").expect("mkdir failed");
    ext4.mkdir("1/2/3/4").expect("mkdir failed");
    ext4.mkdir("2").expect("mkdir failed");
    ext4.mkdir("2/3").expect("mkdir failed");
    ext4.mkdir("2/3/4").expect("mkdir failed");
    ext4.mkdir("3").expect("mkdir failed");
}

fn open_test(ext4: &mut Ext4) {
    ext4.open("1/2/3/4/5", "w+", true).expect("open failed");
    ext4.open("1/2/3/4/5", "r", true).expect("open failed");
    ext4.open("1/2/3/4/5", "a", true).expect("open failed");
    ext4.open("2/4", "w+", true).expect("open failed");
}

fn read_write_test(ext4: &mut Ext4) {
    let buffer = "hello world".as_bytes();
    let mut wfile = ext4.open("1/2/3/4/5", "w+", true).expect("open failed");
    ext4.write(&mut wfile, buffer).expect("write failed");
    let mut rfile = ext4.open("1/2/3/4/5", "r", true).expect("open failed");
    let mut buffer2 = vec![0u8; buffer.len()];
    ext4.read(&mut rfile, &mut buffer2, buffer.len())
        .expect("read failed");
    assert_eq!(buffer, buffer2);
}

fn main() {
    logger_init();
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
}
