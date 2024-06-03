//! # The Defination of Ext4 Directory Entry
//!
//! A directory is a series of data blocks and that each block contains a
//! linear array of directory entries.

use super::crc::*;
use super::AsBytes;
use super::FileType;
use super::SuperBlock;
use crate::constants::*;
use crate::format_error;
use crate::prelude::*;

#[repr(C)]
#[derive(Clone, Copy)]
pub union DirEnInner {
    pub name_length_high: u8, // 高8位的文件名长度
    pub inode_type: FileType, // 引用的inode的类型（在rev >= 0.5中）
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

#[repr(C)]
#[derive(Debug, Clone)]
pub struct DirEntry {
    inode: InodeId,    // 该目录项指向的inode的编号
    rec_len: u16,      // 到下一个目录项的距离
    name_len: u8,      // 低8位的文件名长度
    inner: DirEnInner, // 联合体成员
    name: [u8; 255],   // 文件名
}

impl Default for DirEntry {
    fn default() -> Self {
        Self {
            inode: 0,
            rec_len: 0,
            name_len: 0,
            inner: DirEnInner::default(),
            name: [0; 255],
        }
    }
}

/// The actual size of the directory entry is determined by `name_len`.
/// So we need to implement `AsBytes` methods specifically for `DirEntry`.
impl AsBytes for DirEntry {
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

    pub fn calc_csum(&self, s: &SuperBlock, blk_data: &[u8]) -> u32 {
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
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct DirEntryTail {
    pub reserved_zero1: u32,
    pub rec_len: u16,
    pub reserved_zero2: u8,
    pub reserved_ft: u8,
    pub checksum: u32, // crc32c(uuid+inum+dirblock)
}

impl AsBytes for DirEntryTail {}

impl DirEntryTail {
    pub fn set_csum(&mut self, s: &SuperBlock, diren: &DirEntry, blk_data: &[u8]) {
        self.checksum = diren.calc_csum(s, blk_data);
    }
}

/// Fake dir entry. A normal entry without `name` field`
#[repr(C)]
pub struct FakeDirEntry {
    inode: u32,
    rec_len: u16,
    name_len: u8,
    inode_type: FileType,
}

impl AsBytes for FakeDirEntry {}
