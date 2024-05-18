//! # The Defination of Ext4 Inode Table Entry
//!
//! The inode table is a linear array of struct `Inode`. The table is sized to have
//! enough blocks to store at least `sb.inode_size * sb.inodes_per_group` bytes.
//!
//! The number of the block group containing an inode can be calculated as
//! `(inode_number - 1) / sb.inodes_per_group`, and the offset into the group's table is
//! `(inode_number - 1) % sb.inodes_per_group`. There is no inode 0.

use super::crc::*;
use super::AsBytes;
use super::BlockDevice;
use super::BlockGroupRef;
use super::ExtentHeader;
use super::Superblock;
use super::{ExtentNode, ExtentNodeMut};
use crate::constants::*;
use crate::prelude::*;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Linux2 {
    pub l_i_blocks_high: u16, // 原来是l_i_reserved1
    pub l_i_file_acl_high: u16,
    pub l_i_uid_high: u16,    // 这两个字段
    pub l_i_gid_high: u16,    // 原来是reserved2[0]
    pub l_i_checksum_lo: u16, // crc32c(uuid+inum+inode) LE
    pub l_i_reserved: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inode {
    pub mode: u16,
    pub uid: u16,
    pub size: u32,
    pub atime: u32,
    pub ctime: u32,
    pub mtime: u32,
    pub dtime: u32,
    pub gid: u16,
    pub links_count: u16,
    pub blocks: u32,
    pub flags: u32,
    pub osd1: u32,
    pub block: [u8; 60], // Block bitmap or extent tree
    pub generation: u32,
    pub file_acl: u32,
    pub size_hi: u32,
    pub faddr: u32,   /* Obsoleted fragment address */
    pub osd2: Linux2, // 操作系统相关的字段2

    pub i_extra_isize: u16,
    pub i_checksum_hi: u16,  // crc32c(uuid+inum+inode) BE
    pub i_ctime_extra: u32,  // 额外的修改时间（nsec << 2 | epoch）
    pub i_mtime_extra: u32,  // 额外的文件修改时间（nsec << 2 | epoch）
    pub i_atime_extra: u32,  // 额外的访问时间（nsec << 2 | epoch）
    pub i_crtime: u32,       // 文件创建时间
    pub i_crtime_extra: u32, // 额外的文件创建时间（nsec << 2 | epoch）
    pub i_version_hi: u32,   // 64位版本的高32位
}

/// Because `[u8; 60]` cannot derive `Default`, we implement it manually.
impl Default for Inode {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl AsBytes for Inode {}

impl Inode {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        unsafe { *(bytes.as_ptr() as *const Inode) }
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }

    pub fn set_flags(&mut self, f: u32) {
        self.flags |= f;
    }

    pub fn mode(&self) -> u16 {
        self.mode
    }

    pub fn set_mode(&mut self, mode: u16) {
        self.mode |= mode;
    }

    pub fn inode_type(&self, super_block: &Superblock) -> u32 {
        let mut v = self.mode;

        if super_block.creator_os() == EXT4_SUPERBLOCK_OS_HURD {
            v |= ((self.osd2.l_i_file_acl_high as u32) << 16) as u16;
        }

        (v & EXT4_INODE_MODE_TYPE_MASK) as u32
    }

    pub fn is_dir(&self, super_block: &Superblock) -> bool {
        self.inode_type(super_block) == EXT4_INODE_MODE_DIRECTORY as u32
    }

    pub fn is_softlink(&self, super_block: &Superblock) -> bool {
        self.inode_type(super_block) == EXT4_INODE_MODE_SOFTLINK as u32
    }

    pub fn links_cnt(&self) -> u16 {
        self.links_count
    }

    pub fn set_links_cnt(&mut self, cnt: u16) {
        self.links_count = cnt;
    }

    pub fn set_uid(&mut self, uid: u16) {
        self.uid = uid;
    }

    pub fn set_gid(&mut self, gid: u16) {
        self.gid = gid;
    }

    pub fn size(&mut self) -> u64 {
        self.size as u64 | ((self.size_hi as u64) << 32)
    }

    pub fn set_size(&mut self, size: u64) {
        self.size = ((size << 32) >> 32) as u32;
        self.size_hi = (size >> 32) as u32;
    }

    pub fn set_access_time(&mut self, access_time: u32) {
        self.atime = access_time;
    }

    pub fn set_change_inode_time(&mut self, change_inode_time: u32) {
        self.ctime = change_inode_time;
    }

    pub fn set_modif_time(&mut self, modif_time: u32) {
        self.mtime = modif_time;
    }

    pub fn set_del_time(&mut self, del_time: u32) {
        self.dtime = del_time;
    }

    pub fn blocks_count(&self) -> u64 {
        let mut blocks = self.blocks as u64;
        if self.osd2.l_i_blocks_high != 0 {
            blocks |= (self.osd2.l_i_blocks_high as u64) << 32;
        }
        blocks
    }

    pub fn set_blocks_count(&mut self, blocks_count: u32) {
        self.blocks = blocks_count;
    }

    pub fn set_generation(&mut self, generation: u32) {
        self.generation = generation;
    }

    pub fn set_extra_isize(&mut self, extra_isize: u16) {
        self.i_extra_isize = extra_isize;
    }

    fn copy_to_byte_slice(&self, slice: &mut [u8]) {
        unsafe {
            let inode_ptr = self as *const Inode as *const u8;
            let array_ptr = slice.as_ptr() as *mut u8;
            core::ptr::copy_nonoverlapping(inode_ptr, array_ptr, 0x9c);
        }
    }

    /* Extent methods */

    /// Get the immutable extent root node
    pub fn extent_node(&self) -> ExtentNode {
        ExtentNode::from_bytes(unsafe {
            core::slice::from_raw_parts(self.block.as_ptr() as *const u8, 60)
        })
    }

    /// Get the mutable extent root node
    pub fn extent_node_mut(&mut self) -> ExtentNodeMut {
        ExtentNodeMut::from_bytes(unsafe {
            core::slice::from_raw_parts_mut(self.block.as_mut_ptr() as *mut u8, 60)
        })
    }

    /// Initialize the `flags` and `block` field of inode. Mark the
    /// inode to use extent for block mapping. Initialize the root
    /// node of the extent tree
    pub fn extent_init(&mut self) {
        self.set_flags(EXT4_INODE_FLAG_EXTENTS);
        let header = ExtentHeader::new(0, 4, 0, 0);
        let header_ptr = &header as *const ExtentHeader as *const u8;
        let array_ptr = &mut self.block as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(header_ptr, array_ptr, size_of::<ExtentHeader>());
        }
    }
}

/// A combination of an `Inode` and its id
#[derive(Clone)]
pub struct InodeRef {
    pub id: InodeId,
    pub inode: Inode,
}

impl InodeRef {
    pub fn new(id: InodeId, inode: Inode) -> Self {
        Self { id, inode }
    }

    pub fn load_from_disk(
        block_device: Arc<dyn BlockDevice>,
        super_block: &Superblock,
        id: InodeId,
    ) -> Self {
        let (block_id, offset) = Self::disk_pos(super_block, block_device.clone(), id);
        let block = block_device.read_block(block_id);
        Self {
            id,
            inode: Inode::from_bytes(block.read_offset(offset, size_of::<Inode>())),
        }
    }

    /// Find the position of an inode in the block device. Return the
    /// block id and the offset within the block.
    ///
    /// Each block group contains `sb.inodes_per_group` inodes.
    /// Because inode 0 is defined not to exist, this formula can
    /// be used to find the block group that an inode lives in:
    /// `bg = (inode_id - 1) / sb.inodes_per_group`.
    ///
    /// The particular inode can be found within the block group's
    /// inode table at `index = (inode_id - 1) % sb.inodes_per_group`.
    /// To get the byte address within the inode table, use
    /// `offset = index * sb.inode_size`.
    fn disk_pos(
        super_block: &Superblock,
        block_device: Arc<dyn BlockDevice>,
        inode_id: InodeId,
    ) -> (PBlockId, usize) {
        let inodes_per_group = super_block.inodes_per_group();
        let group = ((inode_id - 1) / inodes_per_group) as BlockGroupId;
        let inode_size = super_block.inode_size() as usize;
        let index = ((inode_id - 1) % inodes_per_group) as usize;

        let bg = BlockGroupRef::load_from_disk(block_device, super_block, group);
        let block_id =
            bg.desc.inode_table_first_block() + (index * inode_size / BLOCK_SIZE) as PBlockId;
        let offset = (index * inode_size) % BLOCK_SIZE;
        (block_id, offset)
    }

    pub fn sync_to_disk_without_csum(
        &self,
        block_device: Arc<dyn BlockDevice>,
        super_block: &Superblock,
    ) {
        let (block_id, offset) = Self::disk_pos(super_block, block_device.clone(), self.id);
        let mut block = block_device.read_block(block_id);
        block.write_offset_as(offset, &self.inode);
        block.sync_to_disk(block_device);
    }

    pub fn sync_to_disk_with_csum(
        &mut self,
        block_device: Arc<dyn BlockDevice>,
        super_block: &Superblock,
    ) {
        self.set_checksum(super_block);
        self.sync_to_disk_without_csum(block_device, super_block);
    }

    pub fn set_checksum(&mut self, super_block: &Superblock) {
        let inode_size = super_block.inode_size();

        let ino_index = self.id as u32;
        let ino_gen = self.inode.generation;

        // Preparation: temporarily set bg checksum to 0
        self.inode.osd2.l_i_checksum_lo = 0;
        self.inode.i_checksum_hi = 0;

        let mut checksum = ext4_crc32c(
            EXT4_CRC32_INIT,
            &super_block.uuid(),
            super_block.uuid().len() as u32,
        );
        checksum = ext4_crc32c(checksum, &ino_index.to_le_bytes(), 4);
        checksum = ext4_crc32c(checksum, &ino_gen.to_le_bytes(), 4);

        let mut raw_data = [0u8; 0x100];
        self.inode.copy_to_byte_slice(&mut raw_data);

        // inode checksum
        checksum = ext4_crc32c(checksum, &raw_data, inode_size as u32);

        if inode_size == 128 {
            checksum &= 0xFFFF;
        }

        self.inode.osd2.l_i_checksum_lo = ((checksum << 16) >> 16) as u16;
        if super_block.inode_size() > 128 {
            self.inode.i_checksum_hi = (checksum >> 16) as u16;
        }
    }
}
