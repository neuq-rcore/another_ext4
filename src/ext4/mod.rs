use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

mod alloc;
mod dir;
mod extent;
mod file;
mod journal;
mod link;
mod bitmap;
mod utils;

#[derive(Debug)]
pub struct Ext4 {
    pub block_device: Arc<dyn BlockDevice>,
    pub super_block: Ext4Superblock,
    pub block_groups: Vec<Ext4BlockGroupDesc>,
    pub inodes_per_group: u32,
    pub blocks_per_group: u32,
    pub inode_size: usize,
    pub last_inode_bg_id: u32,
    pub mount_point: Ext4MountPoint,
}

impl Ext4 {
    /// Opens and loads an Ext4 from the `block_device`.
    ///
    /// | Super Block | Group Descriptor | Reserved GDT Blocks |
    /// | Block Bitmap | Inode Bitmap | Inode Table | Data Blocks |
    pub fn load(block_device: Arc<dyn BlockDevice>) -> Self {
        // Load the superblock
        // TODO: if the main superblock is corrupted, should we load the backup?
        let raw_data = block_device.read_offset(BASE_OFFSET);
        let super_block = Ext4Superblock::try_from(raw_data).unwrap();
        let inodes_per_group = super_block.inodes_per_group();
        let blocks_per_group = super_block.blocks_per_group();
        let inode_size = super_block.inode_size() as usize;

        // Load the block groups description
        let block_groups_count = super_block.block_groups_count() as usize;
        let mut block_groups = Vec::with_capacity(block_groups_count);
        for idx in 0..block_groups_count {
            let block_group =
                Ext4BlockGroupDesc::load(block_device.clone(), &super_block, idx).unwrap();
            block_groups.push(block_group);
        }

        // Root mount point
        let mount_point = Ext4MountPoint::new("/");

        // Create Ext4 instance
        Self {
            super_block,
            inodes_per_group,
            blocks_per_group,
            inode_size,
            block_groups,
            block_device,
            mount_point,
            last_inode_bg_id: 0,
        }
    }

    /// Read an inode from block device, return an`Ext4InodeRef` that combines
    /// the inode and its id.
    fn get_inode_ref(&self, inode_id: u32) -> Ext4InodeRef {
        Ext4InodeRef::read_from_disk(self.block_device.clone(), &self.super_block, inode_id)
    }

    /// Read the root inode from block device
    fn get_root_inode_ref(&self) -> Ext4InodeRef {
        self.get_inode_ref(EXT4_ROOT_INO)
    }

    /// Write back an inode to block device with checksum
    fn write_back_inode_with_csum(&self, inode_ref: &mut Ext4InodeRef) {
        inode_ref
            .sync_to_disk_with_csum(self.block_device.clone(), &self.super_block)
            .unwrap()
    }

    /// Write back an inode to block device without checksum
    fn write_back_inode_without_csum(&self, inode_ref: &mut Ext4InodeRef) {
        inode_ref
            .sync_to_disk_without_csum(self.block_device.clone(), &self.super_block)
            .unwrap()
    }
}
