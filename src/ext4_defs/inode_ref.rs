use super::Ext4Inode;
use crate::Ext4;
use crate::prelude::*;
use crate::constants::*;

pub struct Ext4InodeRef {
    pub inode_num: u32,
    pub inner: InodeRefInner,
    pub fs: Weak<Ext4>,
}

impl Ext4InodeRef {
    pub fn new(fs: Weak<Ext4>) -> Self {
        let inner = InodeRefInner {
            inode: Ext4Inode::default(),
            weak_self: Weak::new(),
        };
        let inode = Self {
            inode_num: 0,
            inner,
            fs,
        };
        inode
    }

    pub fn fs(&self) -> Arc<Ext4> {
        self.fs.upgrade().unwrap()
    }

    pub fn get_inode_ref(fs: Weak<Ext4>, inode_num: u32) -> Self {
        let fs_clone = fs.clone();

        let fs = fs.upgrade().unwrap();
        let super_block = fs.super_block;

        let inodes_per_group = super_block.inodes_per_group();
        let inode_size = super_block.inode_size() as u64;
        let group = (inode_num - 1) / inodes_per_group;
        let index = (inode_num - 1) % inodes_per_group;
        let group = fs.block_groups[group as usize];
        let inode_table_blk_num = group.get_inode_table_blk_num();
        let offset =
            inode_table_blk_num as usize * BLOCK_SIZE + index as usize * inode_size as usize;

        let data = fs.block_device.read_offset(offset);
        let inode_data = &data[..core::mem::size_of::<Ext4Inode>()];
        let inode = Ext4Inode::try_from(inode_data).unwrap();

        let inner = InodeRefInner {
            inode,
            weak_self: Weak::new(),
        };
        let inode = Self {
            inode_num,
            inner,
            fs: fs_clone,
        };

        inode
    }

    pub fn write_back_inode(&mut self) {
        let fs = self.fs();
        let block_device = fs.block_device.clone();
        let super_block = fs.super_block.clone();
        let inode_id = self.inode_num;
        self.inner
            .inode
            .sync_inode_to_disk_with_csum(block_device, &super_block, inode_id)
            .unwrap()
    }

    pub fn write_back_inode_without_csum(&mut self) {
        let fs = self.fs();
        let block_device = fs.block_device.clone();
        let super_block = fs.super_block.clone();
        let inode_id = self.inode_num;
        self.inner
            .inode
            .sync_inode_to_disk(block_device, &super_block, inode_id)
            .unwrap()
    }
}

pub struct InodeRefInner {
    pub inode: Ext4Inode,
    pub weak_self: Weak<Ext4InodeRef>,
}

impl InodeRefInner {
    pub fn inode(&self) -> Arc<Ext4InodeRef> {
        self.weak_self.upgrade().unwrap()
    }

    pub fn write_back_inode(&mut self) {
        let weak_inode_ref = self.weak_self.clone().upgrade().unwrap();
        let fs = weak_inode_ref.fs();
        let block_device = fs.block_device.clone();
        let super_block = fs.super_block.clone();
        let inode_id = weak_inode_ref.inode_num;
        self.inode
            .sync_inode_to_disk_with_csum(block_device, &super_block, inode_id)
            .unwrap()
    }
}