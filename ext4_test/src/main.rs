use ext4_rs::{BlockDevice, Ext4, BLOCK_SIZE};
use log::info;
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
    fn read_offset(&self, offset: usize) -> Vec<u8> {
        // info!("Reading block at offset {}", offset);
        let mut file = &self.0;
        let mut buffer = vec![0u8; BLOCK_SIZE];
        let _r = file.seek(SeekFrom::Start(offset as u64));
        let _r = file.read_exact(&mut buffer);
        buffer
    }

    fn write_offset(&self, offset: usize, data: &[u8]) {
        // info!("Writing block at offset {}", offset);
        let mut file = &self.0;
        let _r = file.seek(SeekFrom::Start(offset as u64));
        let _r = file.write_all(data);
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
    ext4.ext4_dir_mk("1").expect("mkdir failed");
    ext4.ext4_dir_mk("1/2").expect("mkdir failed");
    ext4.ext4_dir_mk("1/2/3").expect("mkdir failed");
    ext4.ext4_dir_mk("1/2/3/4").expect("mkdir failed");
    ext4.ext4_dir_mk("2").expect("mkdir failed");
    ext4.ext4_dir_mk("2/3").expect("mkdir failed");
    ext4.ext4_dir_mk("2/3/4").expect("mkdir failed");
    ext4.ext4_dir_mk("3").expect("mkdir failed");
}

fn open_test(ext4: &mut Ext4) {
    ext4.ext4_open("1/2/3/4/5", "w+", true)
        .expect("open failed");
    ext4.ext4_open("1/2/3/4/5", "r", true).expect("open failed");
    ext4.ext4_open("1/2/3/4/5", "a", true).expect("open failed");
    ext4.ext4_open("2/4", "w+", true).expect("open failed");
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
}
