//! A directory is a series of data blocks and that each block contains a
//! linear array of directory entries.

use super::crc::*;
use super::AsBytes;
use super::FileType;
use crate::constants::*;
use crate::format_error;
use crate::prelude::*;
use crate::Block;

#[repr(C)]
#[derive(Clone, Copy)]
pub union DirEnInner {
    name_length_high: u8, // 高8位的文件名长度
    inode_type: FileType, // 引用的inode的类型（在rev >= 0.5中）
}

impl Debug for DirEnInner {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        unsafe { write!(f, "inode_type: {:?}", self.inode_type) }
    }
}

impl Default for DirEnInner {
    fn default() -> Self {
        Self {
            name_length_high: 0,
        }
    }
}

/// Directory entry.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// 该目录项指向的inode的编号
    inode: InodeId,
    /// 到下一个目录项的距离
    rec_len: u16,
    /// 低8位的文件名长度
    name_len: u8,
    /// 联合体成员
    inner: DirEnInner,
    /// 文件名
    name: [u8; 255],
}

/// Fake dir entry. A normal entry without `name` field
#[repr(C)]
pub struct FakeDirEntry {
    inode: u32,
    rec_len: u16,
    name_len: u8,
    inode_type: FileType,
}
unsafe impl AsBytes for FakeDirEntry {}

/// The actual size of the directory entry is determined by `name_len`.
/// So we need to implement `AsBytes` methods specifically for `DirEntry`.
unsafe impl AsBytes for DirEntry {
    fn from_bytes(bytes: &[u8]) -> Self {
        let fake_entry = FakeDirEntry::from_bytes(bytes);
        let mut entry = DirEntry {
            inode: fake_entry.inode,
            rec_len: fake_entry.rec_len,
            name_len: fake_entry.name_len,
            inner: DirEnInner {
                inode_type: fake_entry.inode_type,
            },
            name: [0; 255],
        };
        let name_len = entry.name_len as usize;
        let name_offset = size_of::<FakeDirEntry>();
        entry.name[..name_len].copy_from_slice(&bytes[name_offset..name_offset + name_len]);
        entry
    }
    fn to_bytes(&self) -> &[u8] {
        let name_len = self.name_len as usize;
        unsafe {
            core::slice::from_raw_parts(
                self as *const Self as *const u8,
                size_of::<FakeDirEntry>() + name_len,
            )
        }
    }
}

impl DirEntry {
    /// Create a new directory entry
    pub fn new(inode: InodeId, rec_len: u16, name: &str, dirent_type: FileType) -> Self {
        let mut name_bytes = [0u8; 255];
        let name_len = name.as_bytes().len();
        name_bytes[..name_len].copy_from_slice(name.as_bytes());
        Self {
            inode,
            rec_len,
            name_len: name_len as u8,
            inner: DirEnInner {
                inode_type: dirent_type,
            },
            name: name_bytes,
        }
    }

    pub fn name(&self) -> Result<String> {
        let name_len = self.name_len as usize;
        let name = &self.name[..name_len];
        String::from_utf8(name.to_vec()).map_err(|_| {
            format_error!(
                ErrCode::EINVAL,
                "Invalid UTF-8 sequence in directory entry name"
            )
        })
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

    /// Set a directory entry as unused
    pub fn set_unused(&mut self) {
        self.inode = 0
    }

    /// Set the dir entry's file type
    pub fn set_type(&mut self, file_type: FileType) {
        self.inner.inode_type = file_type;
    }

    /// Get the required size to save a directory entry, 4-byte aligned
    pub fn required_size(name_len: usize) -> usize {
        // u32 + u16 + u8 + Ext4DirEnInner + name -> align to 4
        (core::mem::size_of::<FakeDirEntry>() + name_len + 3) / 4 * 4
    }

    /// Get the used size of this directory entry, 4-bytes alighed
    pub fn used_size(&self) -> usize {
        Self::required_size(self.name_len as usize)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct DirEntryTail {
    reserved_zero1: u32,
    rec_len: u16,
    reserved_zero2: u8,
    reserved_ft: u8,
    checksum: u32, // crc32c(uuid+inum+dirblock)
}

unsafe impl AsBytes for DirEntryTail {}

impl DirEntryTail {
    pub fn new() -> Self {
        Self {
            reserved_zero1: 0,
            rec_len: 12,
            reserved_zero2: 0,
            reserved_ft: 0xDE,
            checksum: 0,
        }
    }

    pub fn set_csum(&mut self, uuid: &[u8], ino: InodeId, ino_gen: u32, block: &Block) {
        let mut csum = crc32(CRC32_INIT, &uuid);
        csum = crc32(csum, &ino.to_le_bytes());
        csum = crc32(csum, &ino_gen.to_le_bytes());
        self.checksum = crc32(csum, &block.data[..size_of::<DirEntryTail>()]);
    }
}
