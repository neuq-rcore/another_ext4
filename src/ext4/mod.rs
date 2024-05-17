use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

mod alloc;
mod dir;
mod extent;
mod file;
mod journal;
mod link;

#[derive(Debug)]
pub struct Ext4 {
    pub block_device: Arc<dyn BlockDevice>,
    pub super_block: Superblock,
    pub mount_point: MountPoint,
}

impl Ext4 {
    /// Opens and loads an Ext4 from the `block_device`.
    ///
    /// | Super Block | Group Descriptor | Reserved GDT Blocks |
    /// | Block Bitmap | Inode Bitmap | Inode Table | Data Blocks |
    pub fn load(block_device: Arc<dyn BlockDevice>) -> Result<Self> {
        // Load the superblock
        // TODO: if the main superblock is corrupted, should we load the backup?
        let block = block_device.read_block(0);
        let super_block = block.read_offset_as::<Superblock>(BASE_OFFSET);
        // Root mount point
        let mount_point = MountPoint::new("/");
        // Create Ext4 instance
        let mut ext4 = Self {
            super_block,
            block_device,
            mount_point,
        };
        // Create root directory
        ext4.alloc_root_inode()?;
        Ok(ext4)
    }

    /// Read an inode from block device, return an `InodeRef` that combines
    /// the inode and its id.
    fn get_inode_ref(&self, inode_id: InodeId) -> InodeRef {
        InodeRef::read_from_disk(self.block_device.clone(), &self.super_block, inode_id)
    }

    /// Read the root inode from block device
    fn get_root_inode_ref(&self) -> InodeRef {
        self.get_inode_ref(EXT4_ROOT_INO)
    }

    /// Write back an inode to block device with checksum
    fn write_back_inode_with_csum(&self, inode_ref: &mut InodeRef) {
        inode_ref
            .sync_to_disk_with_csum(self.block_device.clone(), &self.super_block)
            
    }

    /// Write back an inode to block device without checksum
    fn write_back_inode_without_csum(&self, inode_ref: &mut InodeRef) {
        inode_ref
            .sync_to_disk_without_csum(self.block_device.clone(), &self.super_block)
    }
}
