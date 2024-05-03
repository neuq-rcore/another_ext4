use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

mod dir;
mod file;
mod alloc;
mod utils;
mod extent;
mod link;

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
    pub fn new(block_device: Arc<dyn BlockDevice>) -> Arc<Self> {
        // Load the superblock
        // TODO: if the main superblock is corrupted, should we load the backup?
        let raw_data = block_device.read_offset(BASE_OFFSET);
        let super_block = Ext4Superblock::try_from(raw_data).unwrap();

        // log::info!("super_block: {:x?}", super_block);
        let inodes_per_group = super_block.inodes_per_group();
        let blocks_per_group = super_block.blocks_per_group();
        let inode_size = super_block.inode_size();

        // Load the block groups information
        let load_block_groups =
            |_fs: Weak<Ext4>, block_device: Arc<dyn BlockDevice>| -> Result<Vec<Ext4BlockGroupDesc>> {
                let block_groups_count = super_block.block_groups_count() as usize;
                let mut block_groups = Vec::with_capacity(block_groups_count);
                for idx in 0..block_groups_count {
                    let block_group =
                        Ext4BlockGroupDesc::load(block_device.clone(), &super_block, idx).unwrap();
                    block_groups.push(block_group);
                }
                Ok(block_groups)
            };

        let mount_point = Ext4MountPoint::new("/");

        let ext4: Arc<Ext4> = Arc::new_cyclic(|weak_ref| Self {
            super_block,
            inodes_per_group,
            blocks_per_group,
            inode_size: inode_size as usize,
            block_groups: load_block_groups(weak_ref.clone(), block_device.clone()).unwrap(),
            block_device,
            mount_point,
            last_inode_bg_id: 0,
        });

        ext4
    }

    // start transaction
    pub fn ext4_trans_start(&self) {}

    // stop transaction
    pub fn ext4_trans_abort(&self) {}

    fn get_inode_ref(&self, inode_id: u32) -> Ext4InodeRef {
        let super_block = self.super_block;

        let inodes_per_group = super_block.inodes_per_group();
        let inode_size = super_block.inode_size() as u64;
        let group = (inode_id - 1) / inodes_per_group;
        let index = (inode_id - 1) % inodes_per_group;
        let group = self.block_groups[group as usize];
        let inode_table_blk_num = group.get_inode_table_blk_num();
        let offset =
            inode_table_blk_num as usize * BLOCK_SIZE + index as usize * inode_size as usize;

        let data = self.block_device.read_offset(offset);
        let inode_data = &data[..core::mem::size_of::<Ext4Inode>()];
        let inode = Ext4Inode::try_from(inode_data).unwrap();

        Ext4InodeRef::new(inode_id, inode)
    }

    fn get_root_inode_ref(&self) -> Ext4InodeRef {
        self.get_inode_ref(EXT4_ROOT_INO)
    }

    fn write_back_inode(&self, inode_ref: &mut Ext4InodeRef) {
        let block_device = self.block_device.clone();
        let super_block = self.super_block.clone();
        let inode_id = inode_ref.inode_id;
        inode_ref
            .inode
            .sync_to_disk_with_csum(block_device, &super_block, inode_id)
            .unwrap()
    }

    fn write_back_inode_without_csum(&self, inode_ref: &mut Ext4InodeRef) {
        let block_device = self.block_device.clone();
        let super_block = self.super_block.clone();
        let inode_id = inode_ref.inode_id;
        inode_ref
            .inode
            .sync_to_disk(block_device, &super_block, inode_id)
            .unwrap()
    }
}
