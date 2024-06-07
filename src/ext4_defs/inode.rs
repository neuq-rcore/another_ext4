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
    pub fn perm(&self) -> InodeMode {
        *self & InodeMode::PERM_MASK
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

#[derive(Debug, Clone)]
pub struct FileAttr {
    pub ino: InodeId,
    pub size: u64,
    pub atime: u32,
    pub mtime: u32,
    pub ctime: u32,
    pub crtime: u32,
    pub blocks: u64,
    pub ftype: FileType,
    pub perm: InodeMode,
    pub links: u16,
    pub uid: u32,
    pub gid: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct Linux2 {
    /// Upper 16-bits of the block count. See the note attached to i_blocks_lo.
    pub l_blocks_hi: u16,
    /// Upper 16-bits of the extended attribute block.
    pub l_file_acl_hi: u16,
    /// Upper 16-bits of the Owner UID.
    pub l_uid_hi: u16,
    /// Upper 16-bits of the GID.
    pub l_gid_hi: u16,
    /// Lower 16-bits of the inode checksum.
    pub l_checksum_lo: u16,
    /// Reserved.
    pub l_reserved: u16,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct Inode {
    /// File mode.
    mode: u16,
    /// Lower 16-bits of Owner UID.
    uid: u16,
    /// Lower 32-bits of size in bytes.
    size: u32,
    /// Last access time, in seconds since the epoch.
    atime: u32,
    /// Last inode change time, in seconds since the epoch.
    ctime: u32,
    /// Last data modification time, in seconds since the epoch.
    mtime: u32,
    /// Deletion Time, in seconds since the epoch.
    dtime: u32,
    /// Lower 16-bits of GID.
    gid: u16,
    /// Hard link count.
    link_count: u16,
    /// Lower 32-bits of "block" count.
    /// Note: this field is different from ext4 inode by now.
    /// Ext4 defines this as the count of 512-byte blocks.
    /// To simplify, we define this as the count of 4096-byte blocks.
    block_count: u32,
    /// Inode flags.
    flags: u32,
    /// Os related fields 1.
    osd1: u32,
    /// Block map or extent tree.
    block: [u8; 60],
    /// File version (for NFS).
    generation: u32,
    /// Lower 32-bits of extended attribute block.
    file_acl: u32,
    /// Upper 32-bits of file/directory size.
    size_hi: u32,
    /// (Obsolete) fragment address.
    faddr: u32,
    /// Os related fields 2.
    osd2: Linux2,
    /// Size of this inode - 128. Alternately, the size of the extended inode
    /// fields beyond the original ext2 inode, including this field.
    extra_isize: u16,
    /// Upper 16-bits of the inode checksum.
    checksum_hi: u16,
    /// Extra change time bits. This provides sub-second precision.
    ctime_extra: u32,
    /// Extra modification time bits. This provides sub-second precision.
    mtime_extra: u32,
    /// Extra access time bits. This provides sub-second precision.
    atime_extra: u32,
    /// File creation time, in seconds since the epoch.
    crtime: u32,
    /// Extra file creation time bits. This provides sub-second precision.
    crtime_extra: u32,
    /// Upper 32-bits for version number.
    version_hi: u32,
    /// Project id
    projid: u32,
}

/// Because `[u8; 60]` cannot derive `Default`, we implement it manually.
impl Default for Inode {
    fn default() -> Self {
        let mut inode: Self = unsafe { mem::zeroed() };
        inode.extra_isize = (size_of::<Inode>() - 128) as u16;
        inode
    }
}

unsafe impl AsBytes for Inode {}

impl Inode {
    const FLAG_EXTENTS: u32 = 0x00080000;

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

    pub fn perm(&self) -> InodeMode {
        self.mode().perm()
    }

    pub fn link_count(&self) -> u16 {
        self.link_count
    }

    pub fn set_link_count(&mut self, cnt: u16) {
        self.link_count = cnt;
    }

    pub fn uid(&self) -> u32 {
        self.uid as u32 | ((self.osd2.l_uid_hi as u32) << 16)
    }

    pub fn set_uid(&mut self, uid: u32) {
        self.uid = uid as u16;
        self.osd2.l_uid_hi = (uid >> 16) as u16;
    }

    pub fn gid(&self) -> u32 {
        self.gid as u32 | ((self.osd2.l_gid_hi as u32) << 16)
    }

    pub fn set_gid(&mut self, gid: u32) {
        self.gid = gid as u16;
        self.osd2.l_gid_hi = (gid >> 16) as u16;
    }

    pub fn size(&self) -> u64 {
        self.size as u64 | ((self.size_hi as u64) << 32)
    }

    pub fn set_size(&mut self, size: u64) {
        self.size = ((size << 32) >> 32) as u32;
        self.size_hi = (size >> 32) as u32;
    }

    pub fn atime(&self) -> u32 {
        self.atime
    }

    pub fn set_atime(&mut self, atime: u32) {
        self.atime = atime;
    }

    pub fn ctime(&self) -> u32 {
        self.ctime
    }

    pub fn set_ctime(&mut self, ctime: u32) {
        self.ctime = ctime;
    }

    pub fn mtime(&self) -> u32 {
        self.mtime
    }

    pub fn set_mtime(&mut self, mtime: u32) {
        self.mtime = mtime;
    }

    pub fn dtime(&self) -> u32 {
        self.dtime
    }

    pub fn set_dtime(&mut self, dtime: u32) {
        self.dtime = dtime;
    }

    pub fn crtime(&self) -> u32 {
        self.crtime
    }

    pub fn set_crtime(&mut self, crtime: u32) {
        self.crtime = crtime;
    }

    pub fn block_count(&self) -> u64 {
        self.block_count as u64 | ((self.osd2.l_blocks_hi as u64) << 32)
    }

    pub fn set_block_count(&mut self, cnt: u64) {
        self.block_count = cnt as u32;
        self.osd2.l_blocks_hi = (cnt >> 32) as u16;
    }

    pub fn set_generation(&mut self, generation: u32) {
        self.generation = generation;
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }

    pub fn set_flags(&mut self, f: u32) {
        self.flags |= f;
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
    pub fn extent_root(&self) -> ExtentNode {
        ExtentNode::from_bytes(unsafe {
            core::slice::from_raw_parts(self.block.as_ptr() as *const u8, 60)
        })
    }

    /// Get the mutable extent root node
    pub fn extent_root_mut(&mut self) -> ExtentNodeMut {
        ExtentNodeMut::from_bytes(unsafe {
            core::slice::from_raw_parts_mut(self.block.as_mut_ptr() as *mut u8, 60)
        })
    }

    /// Initialize the `flags` and `block` field of inode. Mark the
    /// inode to use extent for block mapping. Initialize the root
    /// node of the extent tree
    pub fn extent_init(&mut self) {
        self.set_flags(Self::FLAG_EXTENTS);
        self.extent_root_mut().init(0, 0);
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
        self.inode.osd2.l_checksum_lo = 0;
        self.inode.checksum_hi = 0;

        let mut checksum = ext4_crc32c(
            CRC32_INIT,
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

        self.inode.osd2.l_checksum_lo = ((checksum << 16) >> 16) as u16;
        if super_block.inode_size() > 128 {
            self.inode.checksum_hi = (checksum >> 16) as u16;
        }
    }
}
