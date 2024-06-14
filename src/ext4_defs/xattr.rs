//! Extended attributes (xattrs) are typically stored in a separate data block
//! on the disk and referenced from inodes via `inode.file_acl*`.
//!
//! There are two places where extended attributes can be found. The first place
//! is between the end of each inode entry and the beginning of the next inode
//! entry. The second place where extended attributes can be found is in the block
//! pointed to by `inode.file_acl`.
//!
//! We only implement the seperate data block storage of extended attributes.

use super::{AsBytes, Block};
use crate::constants::*;
use crate::prelude::*;

/// The beginning of an extended attribute block.
#[repr(C)]
#[derive(Debug)]
pub struct XattrHeader {
    /// Magic number for identification, 0xEA020000.
    magic: u32,
    /// Reference count.
    refcount: u32,
    /// Number of disk blocks used.
    blocks: u32,
    /// Hash value of all attributes. (UNUSED by now)
    hash: u32,
    /// Checksum of the extended attribute block.
    checksum: u32,
    /// Reserved for future use.
    reserved: [u32; 3],
}

unsafe impl AsBytes for XattrHeader {}

impl XattrHeader {
    const XATTR_MAGIC: u32 = 0xEA020000;

    pub fn new() -> Self {
        XattrHeader {
            magic: Self::XATTR_MAGIC,
            refcount: 1,
            blocks: 1,
            hash: 0,
            checksum: 0,
            reserved: [0; 3],
        }
    }
}

/// Following the struct `XattrHeader` is an array of `XattrEntry`.
#[repr(C)]
#[derive(Debug)]
pub struct XattrEntry {
    /// Length of name.
    name_len: u8,
    /// Attribute name index (UNUSED by now)
    name_index: u8,
    /// Location of this attribute's value on the disk block where
    /// it is stored. For a block this value is relative to the start
    /// of the block (i.e. the header).
    /// value = `block[value_offset..value_offset + value_size]`
    value_offset: u16,
    /// The inode where the value is stored. Zero indicates the value
    /// is in the same block as this entry (FIXED 0 by now)
    value_inum: u32,
    /// Length of attribute value.
    value_size: u32,
    /// Hash value of attribute name and attribute value (UNUSED by now)
    hash: u32,
    /// Attribute name, max 255 bytes.
    name: [u8; 255],
}

/// Fake xattr entry. A normal entry without `name` field.
#[repr(C)]
pub struct FakeXattrEntry {
    name_len: u8,
    name_index: u8,
    value_offset: u16,
    value_inum: u32,
    value_size: u32,
    hash: u32,
}
unsafe impl AsBytes for FakeXattrEntry {}

/// The actual size of the extended attribute entry is determined by `name_len`.
/// So we need to implement `AsBytes` methods specifically for `XattrEntry`.
unsafe impl AsBytes for XattrEntry {
    fn from_bytes(bytes: &[u8]) -> Self {
        let fake_entry = FakeXattrEntry::from_bytes(bytes);
        let mut entry = XattrEntry {
            name_len: fake_entry.name_len,
            name_index: fake_entry.name_index,
            value_offset: fake_entry.value_offset,
            value_inum: fake_entry.value_inum,
            value_size: fake_entry.value_size,
            hash: fake_entry.hash,
            name: [0; 255],
        };
        let name_len = entry.name_len as usize;
        let name_offset = size_of::<FakeXattrEntry>();
        entry.name[..name_len].copy_from_slice(&bytes[name_offset..name_offset + name_len]);
        entry
    }
    fn to_bytes(&self) -> &[u8] {
        let name_len = self.name_len as usize;
        unsafe {
            core::slice::from_raw_parts(
                self as *const Self as *const u8,
                size_of::<FakeXattrEntry>() + name_len,
            )
        }
    }
}

impl XattrEntry {
    /// Create a new xattr entry.
    pub fn new(name: &str, value_size: usize, value_offset: usize) -> Self {
        let mut name_bytes = [0u8; 255];
        let name_len = name.as_bytes().len();
        name_bytes[..name_len].copy_from_slice(name.as_bytes());
        Self {
            name_len: name.len() as u8,
            name_index: 0,
            value_offset: value_offset as u16,
            value_inum: 0,
            value_size: value_size as u32,
            hash: 0,
            name: name_bytes,
        }
    }

    /// Get the required size to save a xattr entry, 4-byte aligned
    pub fn required_size(name_len: usize) -> usize {
        // u32 + u16 + u8 + Ext4DirEnInner + name -> align to 4
        (core::mem::size_of::<FakeXattrEntry>() + name_len + 3) / 4 * 4
    }

    /// Get the used size of this xattr entry, 4-bytes alighed
    pub fn used_size(&self) -> usize {
        Self::required_size(self.name_len as usize)
    }
}

/// The block that stores extended attributes for an inode. The block is
/// pointed to `by inode.file_acl`.
///
/// `XattrHeader` is the beginning of an extended attribute block. Following
/// the struct `XattrHeader` is an array of `XattrEntry`. Attribute values
/// follow the end of the entry table. The values are stored starting at the
/// end of the block and grow towards the xattr_header/xattr_entry table. When
/// the two collide, the disk block fills up, and the filesystem returns `ENOSPC`.
pub struct XattrBlock(Block);

impl XattrBlock {
    pub fn new(block: Block) -> Self {
        XattrBlock(block)
    }

    pub fn init(&mut self) {
        let header = XattrHeader::new();
        self.0.write_offset_as(0, &header);
    }

    pub fn block(self) -> Block {
        self.0
    }

    /// Get a xattr by name, return the value.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let mut entry_start = size_of::<XattrHeader>();
        // Iterate over entry table
        while entry_start < BLOCK_SIZE {
            // Check `name_len`, 0 indicates the end of the entry table.
            if self.0.data[entry_start] == 0 {
                // Target xattr not found
                break;
            }
            let entry: XattrEntry = self.0.read_offset_as(entry_start);
            // Compare name
            if name.as_bytes() == &entry.name[..entry.name_len as usize] {
                return Some(
                    &self
                        .0
                        .read_offset(entry.value_offset as usize, entry.value_size as usize),
                );
            }
            entry_start += entry.used_size();
        }
        None
    }

    /// Insert a xattr entry into the block. Return true if success.
    pub fn insert(&mut self, name: &str, value: &[u8]) -> bool {
        let mut entry_start = size_of::<XattrHeader>();
        let mut value_end = BLOCK_SIZE;
        // Iterate over entry table, find the position to insert entry
        while entry_start < BLOCK_SIZE {
            // Check `name_len`, 0 indicates the end of the entry table.
            if self.0.data[entry_start] == 0 {
                // Insert to the end of table
                break;
            }
            let entry: XattrEntry = self.0.read_offset_as(entry_start);
            entry_start += entry.used_size();
            value_end = entry.value_offset as usize;
        }
        // `[entry_start, value_end)` is the empty space
        // Check space
        let required_size = XattrEntry::required_size(name.len()) + value.len() + 1;
        if value_end - entry_start < required_size {
            return false;
        }
        // Insert entry
        let value_offset = value_end - value.len();
        let entry = XattrEntry::new(name, value.len(), value_offset);
        self.0.write_offset_as(entry_start, &entry);
        // Insert value
        self.0.write_offset(value_offset, value);
        true
    }

    /// Remove a xattr entry from the block. Return true if success.
    pub fn remove(&mut self, name: &str) -> bool {
        let mut entry_start = size_of::<XattrHeader>();
        // Iterate over entry table, find the position to remove entry
        while entry_start < BLOCK_SIZE {
            // Check `name_len`, 0 indicates the end of the entry table.
            if self.0.data[entry_start] == 0 {
                // Target xattr not found
                return false;
            }
            let entry: XattrEntry = self.0.read_offset_as(entry_start);
            // Compare name
            if name.as_bytes() == &entry.name[..entry.name_len as usize] {
                break;
            }
            entry_start += entry.used_size();
        }
        // `entry_start` now points to the removed entry.
        let removed_entry: XattrEntry = self.0.read_offset_as(entry_start);
        let removed_entry_size = removed_entry.used_size();
        // `value_end` points to the end of removed value
        let mut value_end = removed_entry.value_offset as usize + removed_entry.value_size as usize;

        // Move the following entries and values
        while entry_start + removed_entry_size < BLOCK_SIZE {
            let next_entry_start = entry_start + removed_entry_size;
            // Check `name_len`, 0 indicates the end of the entry table.
            if self.0.data[next_entry_start] == 0 {
                break;
            }
            // Get the entry to move
            let mut next_entry: XattrEntry = self.0.read_offset_as(next_entry_start);
            // Get its value
            let next_value = self
                .0
                .read_offset(
                    next_entry.value_offset as usize,
                    next_entry.value_size as usize,
                )
                .to_owned();
            // Move the value
            let value_offset = value_end - next_value.len();
            self.0.write_offset(value_offset, &next_value);
            // Update entry
            next_entry.value_offset = value_offset as u16;
            // Write the entry to block
            self.0.write_offset_as(entry_start, &next_entry);
            // Update offset
            value_end -= next_value.len();
            entry_start += next_entry.used_size();
        }
        // Clear [entry_offset, value_offset)
        trace!("Clearing [{}, {})", entry_start, value_end);
        assert!(entry_start < value_end);
        self.0.data[entry_start..value_end].fill(0);
        true
    }
}
