use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

impl Ext4 {
    /// Allocate a new data block for an inode, return the physical block number
    pub(super) fn alloc_block(&mut self, inode_ref: &mut InodeRef) -> Result<PBlockId> {
        // Calc block group id
        let inodes_per_group = self.super_block.inodes_per_group();
        let bgid = ((inode_ref.id - 1) / inodes_per_group) as BlockGroupId;

        // Load block group descriptor
        let mut bg = self.read_block_group(bgid);

        // Load block bitmap
        let bitmap_block_id = bg.desc.block_bitmap_block(&self.super_block);
        let mut bitmap_block = self.block_device.read_block(bitmap_block_id);
        let mut bitmap = Bitmap::new(&mut bitmap_block.data);

        // Find the first free block
        let fblock = bitmap
            .find_and_set_first_clear_bit(0, 8 * BLOCK_SIZE)
            .ok_or(Ext4Error::new(ErrCode::ENOSPC))? as PBlockId;

        // Set block group checksum
        bg.desc.set_block_bitmap_csum(&self.super_block, &bitmap);
        self.block_device.write_block(&bitmap_block);

        // Update superblock free blocks count
        let free_blocks = self.super_block.free_blocks_count() - 1;
        self.super_block.set_free_blocks_count(free_blocks);
        self.super_block.sync_to_disk(self.block_device.clone());

        // Update inode blocks (different block size!) count
        let inode_blocks = inode_ref.inode.blocks_count() + (BLOCK_SIZE / INODE_BLOCK_SIZE) as u64;
        inode_ref.inode.set_blocks_count(inode_blocks);
        self.write_inode_with_csum(inode_ref);

        // Update block group free blocks count
        let fb_cnt = bg.desc.get_free_blocks_count() - 1;
        bg.desc.set_free_blocks_count(fb_cnt);

        self.write_block_group_with_csum(&mut bg);

        info!("Alloc block {} ok", fblock);
        Ok(fblock)
    }

    /// Append a data block for an inode, return a pair of (logical block id, physical block id)
    pub(super) fn inode_append_block(
        &mut self,
        inode_ref: &mut InodeRef,
    ) -> Result<(LBlockId, PBlockId)> {
        let inode_size = inode_ref.inode.size();
        // The new logical block id
        let iblock = ((inode_size + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64) as u32;
        // Check the extent tree to get the physical block id
        let fblock = self.extent_get_pblock_create(inode_ref, iblock, 1)?;
        // Update the inode
        inode_ref.inode.set_size(inode_size + BLOCK_SIZE as u64);
        self.write_inode_with_csum(inode_ref);
        Ok((iblock, fblock))
    }

    /// Allocate(initialize) the root inode of the file system
    pub(super) fn alloc_root_inode(&mut self) -> Result<InodeRef> {
        let mut inode = Inode::default();
        inode.set_mode(0o777 | EXT4_INODE_MODE_DIRECTORY);
        inode.extent_init();
        if self.super_block.inode_size() > EXT4_GOOD_OLD_INODE_SIZE {
            inode.set_extra_isize(self.super_block.extra_size());
        }
        let mut root = InodeRef::new(EXT4_ROOT_INO, inode);
        let root_self = root.clone();

        // Add `.` and `..` entries
        self.dir_add_entry(&mut root, &root_self, ".")?;
        self.dir_add_entry(&mut root, &root_self, "..")?;
        root.inode.links_count += 2;

        self.write_inode_with_csum(&mut root);
        Ok(root)
    }

    /// Allocate a new inode in the file system, returning the inode and its number
    pub(super) fn alloc_inode(&mut self, filetype: FileType) -> Result<InodeRef> {
        // Allocate an inode
        let is_dir = filetype == FileType::Directory;
        let id = self.do_alloc_inode(is_dir)?;

        // Initialize the inode
        let mut inode = Inode::default();
        let mode = if filetype == FileType::Directory {
            0o777 | EXT4_INODE_MODE_DIRECTORY
        } else if filetype == FileType::SymLink {
            0o777 | EXT4_INODE_MODE_SOFTLINK
        } else {
            0o666 | file_type2inode_mode(filetype)
        };
        inode.set_mode(mode);
        inode.extent_init();
        if self.super_block.inode_size() > EXT4_GOOD_OLD_INODE_SIZE {
            inode.set_extra_isize(self.super_block.extra_size());
        }
        let mut inode_ref = InodeRef::new(id, inode);

        // Sync the inode to disk
        self.write_inode_with_csum(&mut inode_ref);

        info!("Alloc inode {} ok", inode_ref.id);
        Ok(inode_ref)
    }

    /// Allocate a new inode in the filesystem, returning its number.
    fn do_alloc_inode(&mut self, is_dir: bool) -> Result<InodeId> {
        let mut bgid = 0;
        let bg_count = self.super_block.block_groups_count();

        while bgid <= bg_count {
            // Load block group descriptor
            let mut bg = self.read_block_group(bgid);
            // If there are no free inodes in this block group, try the next one
            if bg.desc.free_inodes_count() == 0 {
                bgid += 1;
                continue;
            }

            // Load inode bitmap
            let bitmap_block_id = bg.desc.inode_bitmap_block(&self.super_block);
            let mut bitmap_block = self.block_device.read_block(bitmap_block_id);
            let inode_count = self.super_block.inode_count_in_group(bgid) as usize;
            let mut bitmap = Bitmap::new(&mut bitmap_block.data[..inode_count / 8]);

            // Find a free inode
            let idx_in_bg = bitmap.find_and_set_first_clear_bit(0, inode_count).unwrap() as u32;

            // Update bitmap in disk
            bg.desc.set_inode_bitmap_csum(&self.super_block, &bitmap);
            self.block_device.write_block(&bitmap_block);

            // Modify filesystem counters
            let free_inodes = bg.desc.free_inodes_count() - 1;
            bg.desc
                .set_free_inodes_count(&self.super_block, free_inodes);

            // Increment used directories counter
            if is_dir {
                let used_dirs = bg.desc.used_dirs_count(&self.super_block) - 1;
                bg.desc.set_used_dirs_count(&self.super_block, used_dirs);
            }

            // Decrease unused inodes count
            let mut unused = bg.desc.itable_unused(&self.super_block);
            let free = inode_count as u32 - unused;
            if idx_in_bg >= free {
                unused = inode_count as u32 - (idx_in_bg + 1);
                bg.desc.set_itable_unused(&self.super_block, unused);
            }

            self.write_block_group_with_csum(&mut bg);

            // Update superblock
            self.super_block.decrease_free_inodes_count();
            self.super_block.sync_to_disk(self.block_device.clone());

            // Compute the absolute i-node number
            let inodes_per_group = self.super_block.inodes_per_group();
            let inode_id = bgid * inodes_per_group + (idx_in_bg + 1);

            return Ok(inode_id);
        }
        log::info!("no free inode");
        Err(Ext4Error::new(ErrCode::ENOSPC))
    }
}
