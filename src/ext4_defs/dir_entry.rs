//! # The Defination of Ext4 Directory Entry
//!
//! A directory is a series of data blocks and that each block contains a
//! linear array of directory entries.

use super::crc::*;
use super::Ext4Superblock;
use crate::constants::*;
use crate::prelude::*;
use alloc::string::FromUtf8Error;

#[repr(C)]
pub union Ext4DirEnInner {
    pub name_length_high: u8, // 高8位的文件名长度
    pub inode_type: FileType, // 引用的inode的类型（在rev >= 0.5中）
}

impl Debug for Ext4DirEnInner {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        unsafe {
            write!(
                f,
                "Ext4DirEnInternal {{ name_length_high: {:?} }}",
                self.name_length_high
            )
        }
    }
}

impl Default for Ext4DirEnInner {
    fn default() -> Self {
        Self {
            name_length_high: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct Ext4DirEntry {
    inode: InodeId,        // 该目录项指向的inode的编号
    rec_len: u16,          // 到下一个目录项的距离
    name_len: u8,          // 低8位的文件名长度
    inner: Ext4DirEnInner, // 联合体成员
    name: [u8; 255],       // 文件名
}

impl Default for Ext4DirEntry {
    fn default() -> Self {
        Self {
            inode: 0,
            rec_len: 0,
            name_len: 0,
            inner: Ext4DirEnInner::default(),
            name: [0; 255],
        }
    }
}

impl Ext4DirEntry {
    /// Create a new directory entry
    pub fn new(inode: InodeId, rec_len: u16, name: &str, dirent_type: FileType) -> Self {
        let mut name_bytes = [0u8; 255];
        let name_len = name.as_bytes().len();
        name_bytes[..name_len].copy_from_slice(name.as_bytes());
        Self {
            inode,
            rec_len,
            name_len: name_len as u8,
            inner: Ext4DirEnInner {
                inode_type: dirent_type,
            },
            name: name_bytes,
        }
    }

    /// Load a directory entry from bytes
    pub fn from_bytes(bytes: &[u8]) -> Self {
        unsafe { core::ptr::read(bytes.as_ptr() as *const _) }
    }

    pub fn name(&self) -> core::result::Result<String, FromUtf8Error> {
        let name_len = self.name_len as usize;
        let name = &self.name[..name_len];
        String::from_utf8(name.to_vec())
    }

    pub fn compare_name(&self, name: &str) -> bool {
        &self.name[..name.len()] == name.as_bytes()
    }

    pub fn set_name(&mut self, name: &str) {
        self.name_len = name.len() as u8;
        self.name[..name.len()].copy_from_slice(name.as_bytes());
    }

    /// Distance to the next directory entry
    pub fn rec_len(&self) -> u16 {
        self.rec_len
    }

    pub fn set_rec_len(&mut self, len: u16) {
        self.rec_len = len;
    }

    pub fn inode(&self) -> InodeId {
        self.inode
    }

    pub fn set_inode(&mut self, inode: InodeId) {
        self.inode = inode;
    }

    /// Unused directory entries are signified by inode = 0
    pub fn unused(&self) -> bool {
        self.inode == 0
    }

    /// Set the dir entry's inode type given the corresponding inode mode
    pub fn set_entry_type(&mut self, inode_mode: u16) {
        self.inner.inode_type = inode_mode2file_type(inode_mode);
    }

    /// Get the required size to save this directory entry, 4-byte aligned
    pub fn required_size(name_len: usize) -> usize {
        // u32 + u16 + u8 + Ext4DirEnInner + name -> align to 4
        (core::mem::size_of::<Ext4FakeDirEntry>() + name_len + 3) / 4 * 4
    }

    /// Get the used size of this directory entry, 4-bytes alighed
    pub fn used_size(&self) -> usize {
        Self::required_size(self.name_len as usize)
    }

    pub fn calc_csum(&self, s: &Ext4Superblock, blk_data: &[u8]) -> u32 {
        let ino_index = self.inode;
        let ino_gen = 0 as u32;

        let uuid = s.uuid();

        let mut csum = ext4_crc32c(EXT4_CRC32_INIT, &uuid, uuid.len() as u32);
        csum = ext4_crc32c(csum, &ino_index.to_le_bytes(), 4);
        csum = ext4_crc32c(csum, &ino_gen.to_le_bytes(), 4);
        let mut data = [0u8; 0xff4];
        unsafe {
            core::ptr::copy_nonoverlapping(blk_data.as_ptr(), data.as_mut_ptr(), blk_data.len());
        }
        csum = ext4_crc32c(csum, &data[..], 0xff4);
        csum
    }

    pub fn copy_to_byte_slice(&self, slice: &mut [u8], offset: usize) {
        let de_ptr = self as *const Ext4DirEntry as *const u8;
        let slice_ptr = slice as *mut [u8] as *mut u8;
        let count = core::mem::size_of::<Ext4DirEntry>();
        unsafe {
            core::ptr::copy_nonoverlapping(de_ptr, slice_ptr.add(offset), count);
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Ext4DirEntryTail {
    pub reserved_zero1: u32,
    pub rec_len: u16,
    pub reserved_zero2: u8,
    pub reserved_ft: u8,
    pub checksum: u32, // crc32c(uuid+inum+dirblock)
}

impl Ext4DirEntryTail {
    pub fn from_bytes(data: &mut [u8], blocksize: usize) -> Option<Self> {
        unsafe {
            let ptr = data as *mut [u8] as *mut u8;
            let t = *(ptr.add(blocksize - core::mem::size_of::<Ext4DirEntryTail>())
                as *mut Ext4DirEntryTail);
            if t.reserved_zero1 != 0 || t.reserved_zero2 != 0 {
                log::info!("t.reserved_zero1");
                return None;
            }
            if t.rec_len.to_le() != core::mem::size_of::<Ext4DirEntryTail>() as u16 {
                log::info!("t.rec_len");
                return None;
            }
            if t.reserved_ft != 0xDE {
                log::info!("t.reserved_ft");
                return None;
            }
            Some(t)
        }
    }

    pub fn set_csum(&mut self, s: &Ext4Superblock, diren: &Ext4DirEntry, blk_data: &[u8]) {
        self.checksum = diren.calc_csum(s, blk_data);
    }

    pub fn copy_to_byte_slice(&self, slice: &mut [u8], offset: usize) {
        let de_ptr = self as *const Ext4DirEntryTail as *const u8;
        let slice_ptr = slice as *mut [u8] as *mut u8;
        let count = core::mem::size_of::<Ext4DirEntryTail>();
        unsafe {
            core::ptr::copy_nonoverlapping(de_ptr, slice_ptr.add(offset), count);
        }
    }
}

/// Fake dir entry. A normal entry without `name` field`
#[repr(C)]
pub struct Ext4FakeDirEntry {
    inode: u32,
    entry_length: u16,
    name_length: u8,
    inode_type: u8,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[repr(u8)]
pub enum FileType {
    Unknown,
    RegularFile,
    Directory,
    CharacterDev,
    BlockDev,
    Fifo,
    Socket,
    SymLink,
}

pub fn inode_mode2file_type(inode_mode: u16) -> FileType {
    match inode_mode & EXT4_INODE_MODE_TYPE_MASK {
        EXT4_INODE_MODE_FILE => FileType::RegularFile,
        EXT4_INODE_MODE_DIRECTORY => FileType::Directory,
        EXT4_INODE_MODE_CHARDEV => FileType::CharacterDev,
        EXT4_INODE_MODE_BLOCKDEV => FileType::BlockDev,
        EXT4_INODE_MODE_FIFO => FileType::Fifo,
        EXT4_INODE_MODE_SOCKET => FileType::Socket,
        EXT4_INODE_MODE_SOFTLINK => FileType::SymLink,
        _ => FileType::Unknown,
    }
}

pub fn file_type2inode_mode(dirent_type: FileType) -> u16 {
    match dirent_type {
        FileType::RegularFile => EXT4_INODE_MODE_FILE,
        FileType::Directory => EXT4_INODE_MODE_DIRECTORY,
        FileType::SymLink => EXT4_INODE_MODE_SOFTLINK,
        FileType::CharacterDev => EXT4_INODE_MODE_CHARDEV,
        FileType::BlockDev => EXT4_INODE_MODE_BLOCKDEV,
        FileType::Fifo => EXT4_INODE_MODE_FIFO,
        FileType::Socket => EXT4_INODE_MODE_SOCKET,
        _ => EXT4_INODE_MODE_FILE,
    }
}
