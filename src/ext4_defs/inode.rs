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
use super::SuperBlock;
use super::{ExtentNode, ExtentNodeMut};
use crate::constants::*;
use crate::prelude::*;
use crate::FileType;

bitflags! {
    #[derive(PartialEq, Debug, Clone, Copy)]
    pub struct InodeMode: u16 {
        // Premission
        const PERM_MASK = 0xFFF;
        const USER_READ = 0x100;
        const USER_WRITE = 0x80;
        const USER_EXEC = 0x40;
        const GROUP_READ = 0x20;
        const GROUP_WRITE = 0x10;
        const GROUP_EXEC = 0x8;
        const OTHER_READ = 0x4;
        const OTHER_WRITE = 0x2;
        const OTHER_EXEC = 0x1;
        // File type
        const TYPE_MASK = 0xF000;
        const FIFO = 0x1000;
        const CHARDEV = 0x2000;
        const DIRECTORY = 0x4000;
        const BLOCKDEV = 0x6000;
        const FILE = 0x8000;
        const SOFTLINK = 0xA000;
        const SOCKET = 0xC000;
    }
}

impl InodeMode {
    /// Enable read, write, and execute for all users.
    pub const ALL_RWX: InodeMode = InodeMode::from_bits_retain(0o777);
    /// Enable read and write for all users.
    pub const ALL_RW: InodeMode = InodeMode::from_bits_retain(0o666);

    /// Set an inode mode from a file type and permission bits.
    pub fn from_type_and_perm(file_type: FileType, perm: InodeMode) -> Self {
        (match file_type {
            FileType::RegularFile => InodeMode::FILE,
            FileType::Directory => InodeMode::DIRECTORY,
            FileType::CharacterDev => InodeMode::CHARDEV,
            FileType::BlockDev => InodeMode::BLOCKDEV,
            FileType::Fifo => InodeMode::FIFO,
            FileType::Socket => InodeMode::SOCKET,
            FileType::SymLink => InodeMode::SOFTLINK,
            _ => InodeMode::FILE,
        }) | (perm & InodeMode::PERM_MASK)
    }
    /// Get permission bits of an inode mode.
    pub fn perm_bits(&self) -> u16 {
        (*self & InodeMode::PERM_MASK).bits() as u16
    }
    /// Get the file type of an inode mode.
    pub fn file_type(&self) -> FileType {
        match *self & InodeMode::TYPE_MASK {
            InodeMode::CHARDEV => FileType::CharacterDev,
            InodeMode::DIRECTORY => FileType::Directory,
            InodeMode::BLOCKDEV => FileType::BlockDev,
            InodeMode::FILE => FileType::RegularFile,
            InodeMode::FIFO => FileType::Fifo,
            InodeMode::SOCKET => FileType::Socket,
            InodeMode::SOFTLINK => FileType::SymLink,
            _ => FileType::Unknown,
        }
    }
}

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

    pub fn mode(&self) -> InodeMode {
        InodeMode::from_bits_truncate(self.mode)
    }

    pub fn set_mode(&mut self, mode: InodeMode) {
        self.mode = mode.bits();
    }

    pub fn file_type(&self) -> FileType {
        self.mode().file_type()
    }

    pub fn is_file(&self) -> bool {
        self.file_type() == FileType::RegularFile
    }

    pub fn is_dir(&self) -> bool {
        self.file_type() == FileType::Directory
    }

    pub fn is_softlink(&self) -> bool {
        self.file_type() == FileType::SymLink
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

    pub fn size(&self) -> u64 {
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

    pub fn set_blocks_count(&mut self, blocks_count: u64) {
        self.blocks = (blocks_count & 0xFFFFFFFF) as u32;
        self.osd2.l_i_blocks_high = (blocks_count >> 32) as u16;
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
        self.extent_node_mut().init(0, 0);
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
        block_device: &dyn BlockDevice,
        super_block: &SuperBlock,
        id: InodeId,
    ) -> Self {
        let (block_id, offset) = Self::disk_pos(super_block, block_device, id);
        let block = block_device.read_block(block_id);
        Self {
            id,
            inode: block.read_offset_as(offset),
        }
    }

    pub fn sync_to_disk_without_csum(
        &self,
        block_device: &dyn BlockDevice,
        super_block: &SuperBlock,
    ) {
        let (block_id, offset) = Self::disk_pos(super_block, block_device, self.id);
        let mut block = block_device.read_block(block_id);
        block.write_offset_as(offset, &self.inode);
        block_device.write_block(&block)
    }

    pub fn sync_to_disk_with_csum(
        &mut self,
        block_device: &dyn BlockDevice,
        super_block: &SuperBlock,
    ) {
        self.set_checksum(super_block);
        self.sync_to_disk_without_csum(block_device, super_block);
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
        super_block: &SuperBlock,
        block_device: &dyn BlockDevice,
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

    fn set_checksum(&mut self, super_block: &SuperBlock) {
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
