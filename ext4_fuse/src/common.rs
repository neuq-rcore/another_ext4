use ext4_rs::{
    DirEntry, FileType as Ext4FileType, InodeRef, OpenFlags, INODE_BLOCK_SIZE,
};
use fuser::{FileAttr, FileType};
use std::time::{Duration, SystemTime};

/// A wrapper of ext4_rs::InodeRef
pub struct FuseInode(pub InodeRef);

impl FuseInode {
    pub fn get_attr(&self) -> FileAttr {
        FileAttr {
            ino: self.0.id as u64,
            size: self.0.inode.size(),
            blocks: self.0.inode.blocks_count(),
            atime: get_time(self.0.inode.atime as u64),
            mtime: get_time(self.0.inode.mtime as u64),
            ctime: get_time(self.0.inode.ctime as u64),
            crtime: SystemTime::UNIX_EPOCH,
            kind: translate_ftype(self.0.inode.file_type()),
            perm: self.0.inode.mode().perm_bits(),
            nlink: self.0.inode.links_cnt() as u32,
            uid: self.0.inode.uid as u32,
            gid: self.0.inode.gid as u32,
            rdev: 0,
            blksize: INODE_BLOCK_SIZE as u32,
            flags: 0,
        }
    }
}

/// File handler for fuse filesystem
pub struct FileHandler {
    pub id: u64,
    pub inode: u32,
    pub offset: usize,
    pub flags: OpenFlags,
}

impl FileHandler {
    pub fn new(id: u64, inode: u32, flags: OpenFlags) -> Self {
        Self {
            id,
            inode,
            offset: 0,
            flags,
        }
    }
}

/// Directory handler for fuse filesystem
pub struct DirHandler {
    pub id: u64,
    pub entries: Vec<DirEntry>,
    pub cur: usize,
}

impl DirHandler {
    pub fn new(id: u64, entries: Vec<DirEntry>) -> Self {
        Self {
            id,
            cur: 0,
            entries,
        }
    }

    pub fn next_entry(&mut self) -> Option<DirEntry> {
        let entry = if self.cur < self.entries.len() {
            Some(self.entries[self.cur].clone())
        } else {
            None
        };
        self.cur += 1;
        entry
    }
}

pub fn translate_ftype(file_type: Ext4FileType) -> FileType {
    match file_type {
        Ext4FileType::RegularFile => FileType::RegularFile,
        Ext4FileType::Directory => FileType::Directory,
        Ext4FileType::SymLink => FileType::Symlink,
        Ext4FileType::CharacterDev => FileType::CharDevice,
        Ext4FileType::BlockDev => FileType::BlockDevice,
        Ext4FileType::Fifo => FileType::NamedPipe,
        Ext4FileType::Socket => FileType::Socket,
        Ext4FileType::Unknown => FileType::RegularFile,
    }
}

fn get_time(time: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(time)
}
