//！# The Defination of Ext4 Block Group Description
//! 
//! Block Group Descriptor is the second field of Ext4 Block Group.
//! 
//! | Super Block | Group Descriptor | Reserved GDT Blocks | Block Bitmap |
//! | Inode Bitmap | Inode Table | Data Blocks |
//! 
//! See [`super`] for more information.

use crate::constants::*;
use crate::prelude::*;
use super::crc::*;
use super::BlockDevice;
use super::Ext4Superblock;

#[derive(Debug, Default, Clone, Copy)]
#[repr(C, packed)]
pub struct Ext4BlockGroupDesc {
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

impl TryFrom<&[u8]> for Ext4BlockGroupDesc {
    type Error = u64;
    fn try_from(data: &[u8]) -> core::result::Result<Self, u64> {
        let data = &data[..size_of::<Ext4BlockGroupDesc>()];
        Ok(unsafe { core::ptr::read(data.as_ptr() as *const _) })
    }
}

impl Ext4BlockGroupDesc {
    pub fn load(
        block_device: Arc<dyn BlockDevice>,
        super_block: &Ext4Superblock,
        block_group_id: usize,
    ) -> core::result::Result<Self, u64> {
        let dsc_cnt = BLOCK_SIZE / super_block.desc_size() as usize;
        let dsc_id = block_group_id / dsc_cnt;
        let first_data_block = super_block.first_data_block();

        let block_id = first_data_block as usize + dsc_id + 1;
        let offset = (block_group_id % dsc_cnt) * super_block.desc_size() as usize;

        let data = block_device.read_offset(block_id * BLOCK_SIZE);

        let block_group_data =
            &data[offset as usize..offset as usize + size_of::<Ext4BlockGroupDesc>()];

        let bg = Ext4BlockGroupDesc::try_from(block_group_data);

        bg
    }

    pub fn get_block_bitmap_block(&self, s: &Ext4Superblock) -> u64 {
        let mut v = self.block_bitmap_lo as u64;
        let desc_size = s.desc_size();
        if desc_size > EXT4_MIN_BLOCK_GROUP_DESCRIPTOR_SIZE {
            v |= (self.block_bitmap_hi as u64) << 32;
        }
        v
    }

    pub fn get_inode_bitmap_block(&self, s: &Ext4Superblock) -> u64 {
        let mut v = self.inode_bitmap_lo as u64;
        let desc_size = s.desc_size();
        if desc_size > EXT4_MIN_BLOCK_GROUP_DESCRIPTOR_SIZE {
            v |= (self.inode_bitmap_hi as u64) << 32;
        }
        v
    }

    pub fn get_itable_unused(&mut self, s: &Ext4Superblock) -> u32 {
        let mut v = self.itable_unused_lo as u32;
        if s.desc_size() > EXT4_MIN_BLOCK_GROUP_DESCRIPTOR_SIZE {
            v |= ((self.itable_unused_hi as u64) << 32) as u32;
        }
        v
    }

    pub fn get_used_dirs_count(&self, s: &Ext4Superblock) -> u32 {
        let mut v = self.used_dirs_count_lo as u32;
        if s.desc_size() > EXT4_MIN_BLOCK_GROUP_DESCRIPTOR_SIZE {
            v |= ((self.used_dirs_count_hi as u64) << 32) as u32;
        }
        v
    }

    pub fn set_used_dirs_count(&mut self, s: &Ext4Superblock, cnt: u32) {
        self.itable_unused_lo = ((cnt << 16) >> 16) as u16;
        if s.desc_size() > EXT4_MIN_BLOCK_GROUP_DESCRIPTOR_SIZE {
            self.itable_unused_hi = (cnt >> 16) as u16;
        }
    }

    pub fn set_itable_unused(&mut self, s: &Ext4Superblock, cnt: u32) {
        self.itable_unused_lo = ((cnt << 16) >> 16) as u16;
        if s.desc_size() > EXT4_MIN_BLOCK_GROUP_DESCRIPTOR_SIZE {
            self.itable_unused_hi = (cnt >> 16) as u16;
        }
    }

    pub fn set_free_inodes_count(&mut self, s: &Ext4Superblock, cnt: u32) {
        self.free_inodes_count_lo = ((cnt << 16) >> 16) as u16;
        if s.desc_size() > EXT4_MIN_BLOCK_GROUP_DESCRIPTOR_SIZE {
            self.free_inodes_count_hi = (cnt >> 16) as u16;
        }
    }

    pub fn inode_table_first_block_hi(&self) -> u32 {
        self.inode_table_first_block_hi
    }

    pub fn inode_table_first_block_lo(&self) -> u32 {
        self.inode_table_first_block_lo
    }

    pub fn get_free_inodes_count(&self) -> u32 {
        ((self.free_inodes_count_hi as u64) << 32) as u32 | self.free_inodes_count_lo as u32
    }

    pub fn get_inode_table_blk_num(&self) -> u32 {
        ((self.inode_table_first_block_hi as u64) << 32) as u32 | self.inode_table_first_block_lo
    }

    pub fn sync_block_group_to_disk(
        &self,
        block_device: Arc<dyn BlockDevice>,
        bgid: usize,
        super_block: &Ext4Superblock,
    ) {
        let dsc_cnt = BLOCK_SIZE / super_block.desc_size() as usize;
        // let dsc_per_block = dsc_cnt;
        let dsc_id = bgid / dsc_cnt;
        // let first_meta_bg = super_block.first_meta_bg;
        let first_data_block = super_block.first_data_block();
        let block_id = first_data_block as usize + dsc_id + 1;
        let offset = (bgid % dsc_cnt) * super_block.desc_size() as usize;

        let data = unsafe {
            core::slice::from_raw_parts(self as *const _ as *const u8, size_of::<Ext4BlockGroupDesc>())
        };
        block_device.write_offset(block_id * BLOCK_SIZE + offset, data);
    }

    pub fn get_block_group_checksum(&mut self, bgid: u32, super_block: &Ext4Superblock) -> u16 {
        let desc_size = super_block.desc_size();

        let orig_checksum = self.checksum;

        // 准备：暂时将bg校验和设为0
        self.checksum = 0;

        // uuid checksum
        let mut checksum = ext4_crc32c(
            EXT4_CRC32_INIT,
            &super_block.uuid(),
            super_block.uuid().len() as u32,
        );

        // bgid checksum
        checksum = ext4_crc32c(checksum, &bgid.to_le_bytes(), 4);

        // cast self to &[u8]
        let self_bytes =
            unsafe { core::slice::from_raw_parts(self as *const _ as *const u8, 0x40 as usize) };

        // bg checksum
        checksum = ext4_crc32c(checksum, self_bytes, desc_size as u32);

        self.checksum = orig_checksum;

        let crc = (checksum & 0xFFFF) as u16;

        crc
    }

    pub fn set_block_group_checksum(&mut self, bgid: u32, super_block: &Ext4Superblock) {
        let csum = self.get_block_group_checksum(bgid, super_block);
        self.checksum = csum;
    }

    pub fn sync_to_disk_with_csum(
        &mut self,
        block_device: Arc<dyn BlockDevice>,
        bgid: usize,
        super_block: &Ext4Superblock,
    ) {
        self.set_block_group_checksum(bgid as u32, super_block);
        self.sync_block_group_to_disk(block_device, bgid, super_block)
    }

    pub fn set_block_group_ialloc_bitmap_csum(&mut self, s: &Ext4Superblock, bitmap: &[u8]) {
        let desc_size = s.desc_size();

        let csum = ext4_ialloc_bitmap_csum(bitmap, s);
        let lo_csum = (csum & 0xFFFF).to_le();
        let hi_csum = (csum >> 16).to_le();

        if (s.features_read_only() & 0x400) >> 10 == 0 {
            return;
        }
        self.inode_bitmap_csum_lo = lo_csum as u16;
        if desc_size == EXT4_MAX_BLOCK_GROUP_DESCRIPTOR_SIZE {
            self.inode_bitmap_csum_hi = hi_csum as u16;
        }
    }

    pub fn set_block_group_balloc_bitmap_csum(&mut self, s: &Ext4Superblock, bitmap: &[u8]) {
        let desc_size = s.desc_size();

        let csum = ext4_balloc_bitmap_csum(bitmap, s);
        let lo_csum = (csum & 0xFFFF).to_le();
        let hi_csum = (csum >> 16).to_le();

        if (s.features_read_only() & 0x400) >> 10 == 0 {
            return;
        }
        self.block_bitmap_csum_lo = lo_csum as u16;
        if desc_size == EXT4_MAX_BLOCK_GROUP_DESCRIPTOR_SIZE {
            self.block_bitmap_csum_hi = hi_csum as u16;
        }
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
}

fn ext4_ialloc_bitmap_csum(bitmap: &[u8], s: &Ext4Superblock) -> u32 {
    let inodes_per_group = s.inodes_per_group();
    let uuid = s.uuid();
    let mut csum = ext4_crc32c(EXT4_CRC32_INIT, &uuid, uuid.len() as u32);
    csum = ext4_crc32c(csum, bitmap, (inodes_per_group + 7) / 8);
    csum
}

fn ext4_balloc_bitmap_csum(bitmap: &[u8], s: &Ext4Superblock) -> u32 {
    let blocks_per_group = s.blocks_per_group();
    let uuid = s.uuid();
    let mut csum = ext4_crc32c(EXT4_CRC32_INIT, &uuid, uuid.len() as u32);
    csum = ext4_crc32c(csum, bitmap, (blocks_per_group / 8) as u32);
    csum
}
