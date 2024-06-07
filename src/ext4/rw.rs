use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

use super::Ext4;

impl Ext4 {
    /// Read super block from block device
    #[allow(unused)]
    pub(super) fn read_super_block(&self) -> SuperBlock {
        SuperBlock::load_from_disk(self.block_device.as_ref())
    }

    /// Write super block to block device
    pub(super) fn write_super_block(&self, sb: &SuperBlock) {
        sb.sync_to_disk(self.block_device.as_ref());
    }

    /// Read a block from block device
    pub(super) fn read_block(&self, block_id: PBlockId) -> Block {
        self.block_device.read_block(block_id)
    }

    /// Write a block to block device
    pub(super) fn write_block(&self, block: &Block) {
        self.block_device.write_block(block)
    }

    /// Read an inode from block device, return an `InodeRef` that
    /// combines the inode and its id.
    pub(super) fn read_inode(&self, inode_id: InodeId) -> InodeRef {
        InodeRef::load_from_disk(
            self.block_device.as_ref(),
            &self.read_super_block(),
            inode_id,
        )
    }

    /// Read the root inode from block device
    #[allow(unused)]
    pub(super) fn read_root_inode(&self) -> InodeRef {
        self.read_inode(EXT4_ROOT_INO)
    }

    /// Write an inode to block device with checksum
    pub(super) fn write_inode_with_csum(&self, inode_ref: &mut InodeRef) {
        inode_ref.sync_to_disk_with_csum(self.block_device.as_ref(), &self.read_super_block())
    }

    /// Write an inode to block device without checksum
    pub(super) fn write_inode_without_csum(&self, inode_ref: &InodeRef) {
        inode_ref.sync_to_disk_without_csum(self.block_device.as_ref(), &self.read_super_block())
    }

    /// Read a block group descriptor from block device, return an `BlockGroupRef`
    /// that combines the block group descriptor and its id.
    pub(super) fn read_block_group(&self, block_group_id: BlockGroupId) -> BlockGroupRef {
        BlockGroupRef::load_from_disk(
            self.block_device.as_ref(),
            &self.read_super_block(),
            block_group_id,
        )
    }

    /// Write a block group descriptor to block device with checksum
    pub(super) fn write_block_group_with_csum(&self, bg_ref: &mut BlockGroupRef) {
        bg_ref.sync_to_disk_with_csum(self.block_device.as_ref(), &self.read_super_block())
    }

    /// Write a block group descriptor to block device without checksum
    #[allow(unused)]
    pub(super) fn write_block_group_without_csum(&self, bg_ref: &BlockGroupRef) {
        bg_ref.sync_to_disk_without_csum(self.block_device.as_ref(), &self.read_super_block())
    }
}
