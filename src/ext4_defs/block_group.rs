//！# The Defination of Ext4 Block Group Description
//!
//! Block Group Descriptor is the second field of Ext4 Block Group.
//!
//! | Super Block | Group Descriptor | Reserved GDT Blocks |
//! | Block Bitmap | Inode Bitmap | Inode Table | Data Blocks |
//!
//! See [`super`] for more information.

use super::crc::*;
use super::AsBytes;
use super::Bitmap;
use super::BlockDevice;
use super::SuperBlock;
use crate::constants::*;
use crate::prelude::*;

#[derive(Debug, Default, Clone, Copy)]
#[repr(C, packed)]
pub struct BlockGroupDesc {
    block_bitmap_lo: u32,            // 块位图块
    inode_bitmap_lo: u32,            // 节点位图块
    inode_table_first_block_lo: u32, // 节点表块
    free_blocks_count_lo: u16,       // 空闲块数
    free_inodes_count_lo: u16,       // 空闲节点数
    used_dirs_count_lo: u16,         // 目录数
    flags: u16,                      // EXT4_BG_flags (INODE_UNINIT, etc)
    exclude_bitmap_lo: u32,          // 快照排除位图
    block_bitmap_csum_lo: u16,       // crc32c(s_uuid+grp_num+bbitmap) LE
    inode_bitmap_csum_lo: u16,       // crc32c(s_uuid+grp_num+ibitmap) LE
    itable_unused_lo: u16,           // 未使用的节点数
    checksum: u16,                   // crc16(sb_uuid+group+desc)

    block_bitmap_hi: u32,            // 块位图块 MSB
    inode_bitmap_hi: u32,            // 节点位图块 MSB
    inode_table_first_block_hi: u32, // 节点表块 MSB
    free_blocks_count_hi: u16,       // 空闲块数 MSB
    free_inodes_count_hi: u16,       // 空闲节点数 MSB
    used_dirs_count_hi: u16,         // 目录数 MSB
    itable_unused_hi: u16,           // 未使用的节点数 MSB
    exclude_bitmap_hi: u32,          // 快照排除位图 MSB
    block_bitmap_csum_hi: u16,       // crc32c(s_uuid+grp_num+bbitmap) BE
    inode_bitmap_csum_hi: u16,       // crc32c(s_uuid+grp_num+ibitmap) BE
    reserved: u32,                   // 填充
}

unsafe impl AsBytes for BlockGroupDesc {}

impl BlockGroupDesc {
    const MIN_BLOCK_GROUP_DESC_SIZE: u16 = 32;
    const MAX_BLOCK_GROUP_DESC_SIZE: u16 = 64;

    pub fn block_bitmap_block(&self, s: &SuperBlock) -> PBlockId {
        let mut v = self.block_bitmap_lo as u64;
        if s.desc_size() > Self::MIN_BLOCK_GROUP_DESC_SIZE {
            v |= (self.block_bitmap_hi as u64) << 32;
        }
        v
    }

    pub fn inode_bitmap_block(&self, s: &SuperBlock) -> PBlockId {
        let mut v = self.inode_bitmap_lo as u64;
        if s.desc_size() > Self::MIN_BLOCK_GROUP_DESC_SIZE {
            v |= (self.inode_bitmap_hi as u64) << 32;
        }
        v
    }

    pub fn itable_unused(&mut self, s: &SuperBlock) -> u32 {
        let mut v = self.itable_unused_lo as u32;
        if s.desc_size() > Self::MIN_BLOCK_GROUP_DESC_SIZE {
            v |= ((self.itable_unused_hi as u64) << 32) as u32;
        }
        v
    }

    pub fn used_dirs_count(&self, s: &SuperBlock) -> u32 {
        let mut v = self.used_dirs_count_lo as u32;
        if s.desc_size() > Self::MIN_BLOCK_GROUP_DESC_SIZE {
            v |= ((self.used_dirs_count_hi as u64) << 32) as u32;
        }
        v
    }

    pub fn set_used_dirs_count(&mut self, s: &SuperBlock, cnt: u32) {
        self.itable_unused_lo = ((cnt << 16) >> 16) as u16;
        if s.desc_size() > Self::MIN_BLOCK_GROUP_DESC_SIZE {
            self.itable_unused_hi = (cnt >> 16) as u16;
        }
    }

    pub fn set_itable_unused(&mut self, s: &SuperBlock, cnt: u32) {
        self.itable_unused_lo = ((cnt << 16) >> 16) as u16;
        if s.desc_size() > Self::MIN_BLOCK_GROUP_DESC_SIZE {
            self.itable_unused_hi = (cnt >> 16) as u16;
        }
    }

    pub fn set_free_inodes_count(&mut self, s: &SuperBlock, cnt: u32) {
        self.free_inodes_count_lo = ((cnt << 16) >> 16) as u16;
        if s.desc_size() > Self::MIN_BLOCK_GROUP_DESC_SIZE {
            self.free_inodes_count_hi = (cnt >> 16) as u16;
        }
    }

    pub fn free_inodes_count(&self) -> u32 {
        ((self.free_inodes_count_hi as u32) << 16) | self.free_inodes_count_lo as u32
    }

    pub fn inode_table_first_block(&self) -> PBlockId {
        ((self.inode_table_first_block_hi as u64) << 32) | self.inode_table_first_block_lo as u64
    }

    pub fn get_free_blocks_count(&self) -> u64 {
        let mut v = self.free_blocks_count_lo as u64;
        if self.free_blocks_count_hi != 0 {
            v |= (self.free_blocks_count_hi as u64) << 32;
        }
        v
    }

    pub fn set_free_blocks_count(&mut self, cnt: u64) {
        self.free_blocks_count_lo = ((cnt << 32) >> 32) as u16;
        self.free_blocks_count_hi = (cnt >> 32) as u16;
    }

    pub fn calc_inode_bitmap_csum(bitmap: &Bitmap, s: &SuperBlock) -> u32 {
        let inodes_per_group = s.inodes_per_group();
        let uuid = s.uuid();
        let mut csum = ext4_crc32c(CRC32_INIT, &uuid, uuid.len() as u32);
        csum = ext4_crc32c(csum, bitmap.as_raw(), (inodes_per_group + 7) / 8);
        csum
    }

    pub fn calc_block_bitmap_csum(bitmap: &Bitmap, s: &SuperBlock) -> u32 {
        let blocks_per_group = s.blocks_per_group();
        let uuid = s.uuid();
        let mut csum = ext4_crc32c(CRC32_INIT, &uuid, uuid.len() as u32);
        csum = ext4_crc32c(csum, bitmap.as_raw(), (blocks_per_group / 8) as u32);
        csum
    }

    pub fn set_inode_bitmap_csum(&mut self, s: &SuperBlock, bitmap: &Bitmap) {
        let desc_size = s.desc_size();

        let csum = Self::calc_inode_bitmap_csum(&bitmap, s);
        let lo_csum = (csum & 0xFFFF).to_le();
        let hi_csum = (csum >> 16).to_le();

        if (s.features_read_only() & 0x400) >> 10 == 0 {
            return;
        }
        self.inode_bitmap_csum_lo = lo_csum as u16;
        if desc_size == Self::MAX_BLOCK_GROUP_DESC_SIZE {
            self.inode_bitmap_csum_hi = hi_csum as u16;
        }
    }

    pub fn set_block_bitmap_csum(&mut self, s: &SuperBlock, bitmap: &Bitmap) {
        let desc_size = s.desc_size();

        let csum = Self::calc_block_bitmap_csum(&bitmap, s);
        let lo_csum = (csum & 0xFFFF).to_le();
        let hi_csum = (csum >> 16).to_le();

        if (s.features_read_only() & 0x400) >> 10 == 0 {
            return;
        }
        self.block_bitmap_csum_lo = lo_csum as u16;
        if desc_size == Self::MAX_BLOCK_GROUP_DESC_SIZE {
            self.block_bitmap_csum_hi = hi_csum as u16;
        }
    }
}

/// A combination of a `BlockGroupDesc` and its id
#[derive(Debug)]
pub struct BlockGroupRef {
    /// The block group id
    pub id: BlockGroupId,
    /// The block group descriptor
    pub desc: BlockGroupDesc,
}

impl BlockGroupRef {
    /// Load a block group descriptor from the disk
    pub fn load_from_disk(
        block_device: &dyn BlockDevice,
        super_block: &SuperBlock,
        block_group_id: BlockGroupId,
    ) -> Self {
        let (block_id, offset) = Self::disk_pos(super_block, block_group_id);
        let block = block_device.read_block(block_id as PBlockId);
        let desc = block.read_offset_as::<BlockGroupDesc>(offset);
        Self {
            id: block_group_id,
            desc,
        }
    }

    pub fn sync_to_disk_without_csum(
        &self,
        block_device: &dyn BlockDevice,
        super_block: &SuperBlock,
    ) {
        let (block_id, offset) = Self::disk_pos(super_block, self.id);
        let mut block = block_device.read_block(block_id as PBlockId);
        block.write_offset_as(offset, &self.desc);
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

    /// Find the position of a block group descriptor in the block device.
    /// Return the block id and the offset within the block.
    fn disk_pos(s: &SuperBlock, block_group_id: BlockGroupId) -> (PBlockId, usize) {
        let desc_per_block = BLOCK_SIZE as u32 / s.desc_size() as u32;
        let block_id = s.first_data_block() + block_group_id / desc_per_block + 1;
        let offset = (block_group_id % desc_per_block) * s.desc_size() as u32;
        (block_id as PBlockId, offset as usize)
    }

    fn set_checksum(&mut self, super_block: &SuperBlock) {
        let desc_size = super_block.desc_size();

        // uuid checksum
        let mut checksum = ext4_crc32c(
            CRC32_INIT,
            &super_block.uuid(),
            super_block.uuid().len() as u32,
        );

        // bgid checksum
        checksum = ext4_crc32c(checksum, &self.id.to_le_bytes(), 4);

        // cast self to &[u8]
        let self_bytes =
            unsafe { core::slice::from_raw_parts(self as *const _ as *const u8, 0x40 as usize) };

        // bg checksum
        checksum = ext4_crc32c(checksum, self_bytes, desc_size as u32);

        let crc = (checksum & 0xFFFF) as u16;
        self.desc.checksum = crc;
    }
}
