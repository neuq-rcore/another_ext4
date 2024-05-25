use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

mod alloc;
mod dir;
mod extent;
mod file;
mod journal;
mod link;
mod rw;

#[derive(Debug)]
pub struct Ext4 {
    pub block_device: Arc<dyn BlockDevice>,
    pub super_block: SuperBlock,
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
        let super_block = block.read_offset_as::<SuperBlock>(BASE_OFFSET);
        // Root mount point
        let mount_point = MountPoint::new("/");
        // Create Ext4 instance
        let mut ext4 = Self {
            super_block,
            block_device,
            mount_point,
        };
        // Create root directory
        ext4.create_root_inode()?;
        Ok(ext4)
    }
}
