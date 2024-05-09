use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

impl Ext4 {
    /// Allocate a new data block for an inode, return the physical block number
    pub(super) fn alloc_block(&mut self, inode_ref: &mut Ext4InodeRef, goal: PBlockId) -> PBlockId {
        let bgid = goal / self.blocks_per_group as u64;
        let idx_in_bg = goal % self.blocks_per_group as u64;

        // Load block group descriptor
        let mut bg =
            Ext4BlockGroupDesc::load(self.block_device.clone(), &self.super_block, bgid as usize)
                .unwrap();
        let block_bmap_offset = bg.get_block_bitmap_block(&self.super_block) as usize * BLOCK_SIZE;
        // Load block bitmap
        let raw_bitmap = &mut self.block_device.read_offset(block_bmap_offset);
        let mut bitmap = Bitmap::new(raw_bitmap);
        // Find and first free block
        let fblock = bitmap.find_and_set_first_clear_bit(idx_in_bg as usize, 8 * BLOCK_SIZE);
        if fblock.is_none() {
            return 0;
        }

        // Set block group checksum
        bg.set_block_bitmap_csum(&self.super_block, &bitmap);
        self.block_device
            .write_offset(block_bmap_offset, bitmap.as_raw());

        // Update superblock free blocks count
        let free_blocks = self.super_block.free_blocks_count();
        self.super_block.set_free_blocks_count(free_blocks); // TODO: why not - 1?
        self.super_block.sync_to_disk(self.block_device.clone());

        // Update inode blocks (different block size!) count
        let inode_blocks = inode_ref.inode.blocks_count();
        inode_ref.inode.set_blocks_count(inode_blocks as u32 + 8); // TODO: why + 8?
        self.write_back_inode_with_csum(inode_ref);

        // Update block group free blocks count
        let fb_cnt = bg.get_free_blocks_count();
        bg.set_free_blocks_count(fb_cnt - 1);

        bg.sync_to_disk_with_csum(self.block_device.clone(), bgid as usize, &self.super_block);

        fblock.unwrap() as PBlockId
    }

    /// Append a data block for an inode, return a pair of (logical block id, physical block id)
    pub(super) fn inode_append_block(&mut self, inode_ref: &mut Ext4InodeRef) -> (LBlockId, PBlockId) {
        let inode_size = inode_ref.inode.size();
        // The new logical block id
        let iblock = ((inode_size + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64) as u32;
        // Check the extent tree to get the physical block id
        let fblock = self.extent_get_pblock_create(inode_ref, iblock, 1);
        // Update the inode
        inode_ref.inode.set_size(inode_size + BLOCK_SIZE as u64);
        self.write_back_inode_with_csum(inode_ref);
        (iblock, fblock)
    }

    pub(super) fn ext4_fs_inode_blocks_init(inode_ref: &mut Ext4InodeRef) {
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

    /// Allocate a new inode in the filesystem, returning the inode and its number
    pub(super) fn alloc_inode(&mut self, filetype: FileType) -> Ext4InodeRef {
        // Allocate an inode
        let is_dir = filetype == FileType::Directory;
        let id = self.do_alloc_inode(is_dir);

        // Initialize the inode
        let mut inode = Ext4Inode::default();
        let mode = if filetype == FileType::Directory {
            0o777 | EXT4_INODE_MODE_DIRECTORY as u16
        } else if filetype == FileType::SymLink {
            0o777 | EXT4_INODE_MODE_SOFTLINK as u16
        } else {
            0o666 | file_type2inode_mode(filetype) as u16
        };
        inode.set_mode(mode);
        if self.super_block.inode_size() > EXT4_GOOD_OLD_INODE_SIZE {
            inode.set_extra_isize(self.super_block.extra_size());
        }
        let mut inode_ref = Ext4InodeRef::new(id, inode);

        // Sync the inode to disk
        self.write_back_inode_with_csum(&mut inode_ref);

        inode_ref
    }

    /// Allocate a new inode in the filesystem, returning its number.
    fn do_alloc_inode(&mut self, is_dir: bool) -> u32 {
        let mut bgid = self.last_inode_bg_id;
        let bg_count = self.super_block.block_groups_count();

        while bgid <= bg_count {
            // Load block group descriptor
            let mut bg = Ext4BlockGroupDesc::load(
                self.block_device.clone(),
                &self.super_block,
                bgid as usize,
            )
            .unwrap();
            // If there are no free inodes in this block group, try the next one
            if bg.free_inodes_count() == 0 {
                bgid += 1;
                continue;
            }

            // Load inode bitmap
            let inode_bitmap_block = bg.get_inode_bitmap_block(&self.super_block);
            let mut raw_data = self
                .block_device
                .read_offset(inode_bitmap_block as usize * BLOCK_SIZE);
            let inode_count = self.super_block.inode_count_in_group(bgid);
            let bitmap_size: u32 = inode_count / 0x8;
            let mut bitmap_data = &mut raw_data[..bitmap_size as usize];
            let mut bitmap = Bitmap::new(&mut bitmap_data);

            // Find a free inode
            let idx_in_bg = bitmap
                .find_and_set_first_clear_bit(0, inode_count as usize)
                .unwrap() as u32;

            // Update bitmap in disk
            self.block_device
                .write_offset(inode_bitmap_block as usize * BLOCK_SIZE, &bitmap.as_raw());
            bg.set_inode_bitmap_csum(&self.super_block, &bitmap);

            // Modify filesystem counters
            let free_inodes = bg.free_inodes_count() - 1;
            bg.set_free_inodes_count(&self.super_block, free_inodes);

            // Increment used directories counter
            if is_dir {
                let used_dirs = bg.get_used_dirs_count(&self.super_block) - 1;
                bg.set_used_dirs_count(&self.super_block, used_dirs);
            }

            // Decrease unused inodes count
            let mut unused = bg.get_itable_unused(&self.super_block);
            let free = inode_count - unused as u32;
            if idx_in_bg >= free {
                unused = inode_count - (idx_in_bg + 1);
                bg.set_itable_unused(&self.super_block, unused);
            }

            bg.sync_to_disk_with_csum(self.block_device.clone(), bgid as usize, &self.super_block);

            // Update superblock
            self.super_block.decrease_free_inodes_count();
            self.super_block.sync_to_disk(self.block_device.clone());

            // Compute the absolute i-node number
            let inodes_per_group = self.super_block.inodes_per_group();
            let inode_num = bgid * inodes_per_group + (idx_in_bg + 1);
            log::info!("alloc inode {:x?}", inode_num);

            return inode_num;
        }
        log::info!("no free inode");
        0
    }
}
