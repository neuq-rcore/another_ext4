use super::utils::*;
use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

impl Ext4 {
    pub fn ext4_ialloc_alloc_inode(&self, index: &mut u32, is_dir: bool) {
        let mut bgid = self.last_inode_bg_id;
        let bg_count = self.super_block.block_groups_count();

        while bgid <= bg_count {
            if bgid == bg_count {
                bgid = 0;
                continue;
            }

            let block_device = self.block_device.clone();

            let raw_data = self.block_device.read_offset(BASE_OFFSET);
            let mut super_block = Ext4Superblock::try_from(raw_data).unwrap();

            let mut bg =
                Ext4BlockGroupDesc::load(block_device.clone(), &super_block, bgid as usize).unwrap();

            let mut free_inodes = bg.get_free_inodes_count();
            let mut used_dirs = bg.get_used_dirs_count(&super_block);

            if free_inodes > 0 {
                let inode_bitmap_block = bg.get_inode_bitmap_block(&super_block);

                let mut raw_data = self
                    .block_device
                    .read_offset(inode_bitmap_block as usize * BLOCK_SIZE);

                let inodes_in_bg = super_block.get_inodes_in_group_cnt(bgid);

                let bitmap_size: u32 = inodes_in_bg / 0x8;

                let mut bitmap_data = &mut raw_data[..bitmap_size as usize];

                let mut idx_in_bg = 0 as u32;

                ext4_bmap_bit_find_clr(bitmap_data, 0, inodes_in_bg, &mut idx_in_bg);
                ext4_bmap_bit_set(&mut bitmap_data, idx_in_bg);

                // update bitmap in disk
                self.block_device
                    .write_offset(inode_bitmap_block as usize * BLOCK_SIZE, &bitmap_data);

                bg.set_block_group_ialloc_bitmap_csum(&super_block, &bitmap_data);

                /* Modify filesystem counters */
                free_inodes -= 1;
                bg.set_free_inodes_count(&super_block, free_inodes);

                /* Increment used directories counter */
                if is_dir {
                    used_dirs += 1;
                    bg.set_used_dirs_count(&super_block, used_dirs);
                }

                /* Decrease unused inodes count */
                let mut unused = bg.get_itable_unused(&super_block);
                let free = inodes_in_bg - unused as u32;
                if idx_in_bg >= free {
                    unused = inodes_in_bg - (idx_in_bg + 1);
                    bg.set_itable_unused(&super_block, unused);
                }

                bg.sync_to_disk_with_csum(block_device.clone(), bgid as usize, &super_block);
                // bg.sync_block_group_to_disk(block_device.clone(), bgid as usize, &super_block);

                /* Update superblock */
                super_block.decrease_free_inodes_count();
                // super_block.sync_super_block_to_disk(block_device.clone());

                /* Compute the absolute i-nodex number */
                let inodes_per_group = super_block.inodes_per_group();
                let inode_num = bgid * inodes_per_group + (idx_in_bg + 1);
                *index = inode_num;

                // log::info!("alloc inode {:x?}", inode_num);
                return;
            }

            bgid += 1;
        }
        log::info!("no free inode");
    }

    pub fn ext4_balloc_alloc_block(
        &self,
        inode_ref: &mut Ext4InodeRef,
        goal: Ext4FsBlockId,
        fblock: &mut Ext4FsBlockId,
    ) {
        // let mut alloc: ext4_fsblk_t = 0;
        // let mut bmp_blk_adr: ext4_fsblk_t;
        // let mut rel_blk_idx: u32 = 0;
        // let mut free_blocks: u64;
        // let mut r: i32;

        let block_device = self.block_device.clone();

        let super_block_data = block_device.read_offset(BASE_OFFSET);
        let mut super_block = Ext4Superblock::try_from(super_block_data).unwrap();

        // let inodes_per_group = super_block.inodes_per_group();
        let blocks_per_group = super_block.blocks_per_group();

        let bgid = goal / blocks_per_group as u64;
        let idx_in_bg = goal % blocks_per_group as u64;

        let mut bg =
            Ext4BlockGroupDesc::load(block_device.clone(), &super_block, bgid as usize).unwrap();

        let block_bitmap_block = bg.get_block_bitmap_block(&super_block);
        let mut raw_data = block_device.read_offset(block_bitmap_block as usize * BLOCK_SIZE);
        let mut data: &mut Vec<u8> = &mut raw_data;
        let mut rel_blk_idx = 0 as u32;

        ext4_bmap_bit_find_clr(data, idx_in_bg as u32, 0x8000, &mut rel_blk_idx);
        *fblock = rel_blk_idx as u64;
        ext4_bmap_bit_set(&mut data, rel_blk_idx);

        bg.set_block_group_balloc_bitmap_csum(&super_block, &data);
        block_device.write_offset(block_bitmap_block as usize * BLOCK_SIZE, &data);

        /* Update superblock free blocks count */
        let super_blk_free_blocks = super_block.free_blocks_count();
        // super_blk_free_blocks -= 1;
        super_block.set_free_blocks_count(super_blk_free_blocks);
        super_block.sync_to_disk(block_device.clone());

        /* Update inode blocks (different block size!) count */
        let mut inode_blocks = inode_ref.inode.blocks_count();
        inode_blocks += 8;
        inode_ref
            .inode
            .set_blocks_count(inode_blocks as u32);
        self.write_back_inode(inode_ref);

        /* Update block group free blocks count */
        let mut fb_cnt = bg.get_free_blocks_count();
        fb_cnt -= 1;
        bg.set_free_blocks_count(fb_cnt);
        bg.sync_to_disk_with_csum(block_device, bgid as usize, &super_block);
    }

    pub fn ext4_fs_append_inode_dblk(
        &self,
        inode_ref: &mut Ext4InodeRef,
        iblock: &mut Ext4LogicBlockId,
        fblock: &mut Ext4FsBlockId,
    ) {
        let inode_size = inode_ref.inode.size();
        let block_size = BLOCK_SIZE as u64;
    
        *iblock = ((inode_size + block_size - 1) / block_size) as u32;
    
        let mut current_fsblk: Ext4FsBlockId = 0;
        self.ext4_extent_get_blocks(inode_ref, *iblock, 1, &mut current_fsblk, true, &mut 0);
    
        let current_block = current_fsblk;
        *fblock = current_block;
    
        inode_ref
            .inode
            .set_size(inode_size + BLOCK_SIZE as u64);
    
        self.write_back_inode(inode_ref);
    
        // let mut inode_ref = Ext4InodeRef::get_inode_ref(inode_ref.fs().self_ref.clone(), inode_ref.inode_num);
    
        // log::info!("ext4_fs_append_inode_dblk inode {:x?} inode_size {:x?}", inode_ref.inode_num, inode_ref.inner.inode.size);
        // log::info!("fblock {:x?}", fblock);
    }
    
    pub fn ext4_fs_inode_blocks_init(inode_ref: &mut Ext4InodeRef) {
        // log::info!(
        //     "ext4_fs_inode_blocks_init mode {:x?}",
        //     inode_ref.inner.inode.mode
        // );
    
        let inode = &mut inode_ref.inode;
        let mode = inode.mode;
        let inode_type = InodeMode::from_bits(mode & EXT4_INODE_MODE_TYPE_MASK as u16).unwrap();
    
        match inode_type {
            InodeMode::S_IFDIR => {}
            InodeMode::S_IFREG => {}
            /* Reset blocks array. For inode which is not directory or file, just
             * fill in blocks with 0 */
            _ => {
                log::info!("inode_type {:?}", inode_type);
                return;
            }
        }
    
        /* Initialize extents */
        inode.set_flags(EXT4_INODE_FLAG_EXTENTS as u32);
    
        /* Initialize extent root header */
        inode.extent_tree_init();
        // log::info!("inode iblock {:x?}", inode.block);
    
        // inode_ref.dirty = true;
    }
    
    pub fn ext4_fs_alloc_inode(&self, child_inode_ref: &mut Ext4InodeRef, filetype: u8) -> usize {
        let mut is_dir = false;
    
        let inode_size = self.super_block.inode_size();
        let extra_size = self.super_block.extra_size();
    
        if filetype == DirEntryType::EXT4_DE_DIR.bits() {
            is_dir = true;
        }
    
        let mut index = 0;
        let _rc = self.ext4_ialloc_alloc_inode(&mut index, is_dir);
    
        child_inode_ref.inode_id = index;
    
        let inode = &mut child_inode_ref.inode;
    
        /* Initialize i-node */
    
        let mode = if is_dir {
            0o777 | EXT4_INODE_MODE_DIRECTORY as u16
        } else if filetype == 0x7 {
            0o777 | EXT4_INODE_MODE_SOFTLINK as u16
        } else {
            let t = ext4_fs_correspond_inode_mode(filetype);
            // log::info!("ext4_fs_correspond_inode_mode {:x?}", ext4_fs_correspond_inode_mode(filetype));
            0o666 | t as u16
        };
    
        inode.set_mode(mode);
        inode.set_links_cnt(0);
        inode.set_uid(0);
        inode.set_gid(0);
        inode.set_size(0);
        inode.set_access_time(0);
        inode.set_change_inode_time(0);
        inode.set_modif_time(0);
        inode.set_del_time(0);
        inode.set_flags(0);
        inode.set_generation(0);
    
        if inode_size > EXT4_GOOD_OLD_INODE_SIZE {
            let extra_size = extra_size;
            inode.set_extra_isize(extra_size);
        }
    
        EOK
    }
}
