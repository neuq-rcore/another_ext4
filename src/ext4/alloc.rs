use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::format_error;
use crate::prelude::*;
use crate::return_error;

impl Ext4 {
    /// Create a new inode, returning the inode and its number
    pub(super) fn create_inode(&mut self, mode: InodeMode) -> Result<InodeRef> {
        // Allocate an inode
        let is_dir = mode.file_type() == FileType::Directory;
        let id = self.alloc_inode(is_dir)?;

        // Initialize the inode
        let mut inode = Inode::default();
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

    /// Create(initialize) the root inode of the file system
    pub(super) fn create_root_inode(&mut self) -> Result<InodeRef> {
        let mut inode = Inode::default();
        inode.set_mode(InodeMode::from_type_and_perm(
            FileType::Directory,
            InodeMode::ALL_RWX,
        ));
        inode.extent_init();
        if self.super_block.inode_size() > EXT4_GOOD_OLD_INODE_SIZE {
            inode.set_extra_isize(self.super_block.extra_size());
        }
        let mut root = InodeRef::new(EXT4_ROOT_INO, inode);
        let root_self = root.clone();

        // Add `.` and `..` entries
        self.dir_add_entry(&mut root, &root_self, ".")?;
        self.dir_add_entry(&mut root, &root_self, "..")?;

        self.write_inode_with_csum(&mut root);
        Ok(root)
    }

    /// Free an allocated inode and all data blocks allocated for it
    pub(super) fn free_inode(&mut self, inode: &mut InodeRef) -> Result<()> {
        // Free the data blocks allocated for the inode
        let pblocks = self.extent_get_all_pblocks(&inode)?;
        for pblock in pblocks {
            // Deallocate the block
            self.dealloc_block(inode, pblock)?;
            // Clear the block content
            self.write_block(&Block::new(pblock, [0; BLOCK_SIZE]));
        }
        // Deallocate the inode
        self.dealloc_inode(&inode)?;
        // Clear the inode content
        inode.inode = unsafe { core::mem::zeroed() };
        self.write_inode_without_csum(inode);
        Ok(())
    }

    /// Append a data block for an inode, return a pair of (logical block id, physical block id)
    ///
    /// Only data blocks allocated by `inode_append_block` will be counted in `inode.size`. Blocks
    /// allocated by calling `alloc_block` directly will not be counted, e.g. blocks allocated
    /// to save the inode's extent tree.
    pub(super) fn inode_append_block(
        &mut self,
        inode: &mut InodeRef,
    ) -> Result<(LBlockId, PBlockId)> {
        let inode_size = inode.inode.size();
        // The new logical block id
        let iblock = ((inode_size + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64) as u32;
        // Check the extent tree to get the physical block id
        let fblock = self.extent_get_pblock_create(inode, iblock, 1)?;
        // Update inode block count
        let block_count = inode.inode.block_count() + 1;
        inode.inode.set_block_count(block_count);
        self.write_inode_with_csum(inode);
        
        Ok((iblock, fblock))
    }

    /// Allocate a new physical block for an inode, return the physical block number
    pub(super) fn alloc_block(&mut self, inode: &mut InodeRef) -> Result<PBlockId> {
        // Calc block group id
        let inodes_per_group = self.super_block.inodes_per_group();
        let bgid = ((inode.id - 1) / inodes_per_group) as BlockGroupId;

        // Load block group descriptor
        let mut bg = self.read_block_group(bgid);

        // Load block bitmap
        let bitmap_block_id = bg.desc.block_bitmap_block(&self.super_block);
        let mut bitmap_block = self.read_block(bitmap_block_id);
        let mut bitmap = Bitmap::new(&mut bitmap_block.data);

        // Find the first free block
        let fblock = bitmap
            .find_and_set_first_clear_bit(0, 8 * BLOCK_SIZE)
            .ok_or(format_error!(
                ErrCode::ENOSPC,
                "No free blocks in block group {}",
                bgid
            ))? as PBlockId;

        // Set block group checksum
        bg.desc.set_block_bitmap_csum(&self.super_block, &bitmap);
        self.write_block(&bitmap_block);

        // Update superblock free blocks count
        let free_blocks = self.super_block.free_blocks_count() - 1;
        self.super_block.set_free_blocks_count(free_blocks);
        self.write_super_block();

        // Update inode blocks (different block size!) count
        let inode_blocks = inode.inode.block_count() + (BLOCK_SIZE / INODE_BLOCK_SIZE) as u64;
        inode.inode.set_block_count(inode_blocks);
        self.write_inode_with_csum(inode);

        // Update block group free blocks count
        let fb_cnt = bg.desc.get_free_blocks_count() - 1;
        bg.desc.set_free_blocks_count(fb_cnt);

        self.write_block_group_with_csum(&mut bg);

        info!("Alloc block {} ok", fblock);
        Ok(fblock)
    }

    /// Deallocate a physical block allocated for an inode
    pub(super) fn dealloc_block(
        &mut self,
        inode: &mut InodeRef,
        pblock: PBlockId,
    ) -> Result<()> {
        // Calc block group id
        let inodes_per_group = self.super_block.inodes_per_group();
        let bgid = ((inode.id - 1) / inodes_per_group) as BlockGroupId;

        // Load block group descriptor
        let mut bg = self.read_block_group(bgid);

        // Load block bitmap
        let bitmap_block_id = bg.desc.block_bitmap_block(&self.super_block);
        let mut bitmap_block = self.read_block(bitmap_block_id);
        let mut bitmap = Bitmap::new(&mut bitmap_block.data);

        // Free the block
        if bitmap.is_bit_clear(pblock as usize) {
            return_error!(ErrCode::EINVAL, "Block {} is already free", pblock);
        }
        bitmap.clear_bit(pblock as usize);

        // Set block group checksum
        bg.desc.set_block_bitmap_csum(&self.super_block, &bitmap);
        self.write_block(&bitmap_block);

        // Update superblock free blocks count
        let free_blocks = self.super_block.free_blocks_count() + 1;
        self.super_block.set_free_blocks_count(free_blocks);
        self.write_super_block();

        // Update inode blocks (different block size!) count
        let inode_blocks = inode.inode.block_count() - (BLOCK_SIZE / INODE_BLOCK_SIZE) as u64;
        inode.inode.set_block_count(inode_blocks);
        self.write_inode_with_csum(inode);

        // Update block group free blocks count
        let fb_cnt = bg.desc.get_free_blocks_count() + 1;
        bg.desc.set_free_blocks_count(fb_cnt);

        self.write_block_group_with_csum(&mut bg);

        info!("Free block {} ok", pblock);
        Ok(())
    }

    /// Allocate a new inode, returning the inode number.
    fn alloc_inode(&mut self, is_dir: bool) -> Result<InodeId> {
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
            let mut bitmap_block = self.read_block(bitmap_block_id);
            let inode_count = self.super_block.inode_count_in_group(bgid) as usize;
            let mut bitmap = Bitmap::new(&mut bitmap_block.data[..inode_count / 8]);

            // Find a free inode
            let idx_in_bg =
                bitmap
                    .find_and_set_first_clear_bit(0, inode_count)
                    .ok_or(format_error!(
                        ErrCode::ENOSPC,
                        "No free inodes in block group {}",
                        bgid
                    ))? as u32;

            // Update bitmap in disk
            bg.desc.set_inode_bitmap_csum(&self.super_block, &bitmap);
            self.write_block(&bitmap_block);

            // Modify filesystem counters
            let free_inodes = bg.desc.free_inodes_count() - 1;
            bg.desc
                .set_free_inodes_count(&self.super_block, free_inodes);

            // Increase used directories counter
            if is_dir {
                let used_dirs = bg.desc.used_dirs_count(&self.super_block) + 1;
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
            self.write_super_block();

            // Compute the absolute i-node number
            let inodes_per_group = self.super_block.inodes_per_group();
            let inode_id = bgid * inodes_per_group + (idx_in_bg + 1);

            return Ok(inode_id);
        }

        log::info!("no free inode");
        return_error!(ErrCode::ENOSPC, "No free inodes in block group {}", bgid);
    }

    /// Free an inode
    fn dealloc_inode(&mut self, inode_ref: &InodeRef) -> Result<()> {
        // Calc block group id and index in block group
        let inodes_per_group = self.super_block.inodes_per_group();
        let bgid = ((inode_ref.id - 1) / inodes_per_group) as BlockGroupId;
        let idx_in_bg = (inode_ref.id - 1) % inodes_per_group;

        // Load block group descriptor
        let mut bg = self.read_block_group(bgid);

        // Load inode bitmap
        let bitmap_block_id = bg.desc.inode_bitmap_block(&self.super_block);
        let mut bitmap_block = self.read_block(bitmap_block_id);
        let inode_count = self.super_block.inode_count_in_group(bgid) as usize;
        let mut bitmap = Bitmap::new(&mut bitmap_block.data[..inode_count / 8]);

        // Free the inode
        if bitmap.is_bit_clear(idx_in_bg as usize) {
            return_error!(
                ErrCode::EINVAL,
                "Inode {} is already free in block group {}",
                inode_ref.id,
                bgid
            );
        }
        bitmap.clear_bit(idx_in_bg as usize);

        // Update bitmap in disk
        bg.desc.set_inode_bitmap_csum(&self.super_block, &bitmap);
        self.write_block(&bitmap_block);

        // Modify filesystem counters
        let free_inodes = bg.desc.free_inodes_count() + 1;
        bg.desc
            .set_free_inodes_count(&self.super_block, free_inodes);

        // Increase used directories counter
        if inode_ref.inode.is_dir() {
            let used_dirs = bg.desc.used_dirs_count(&self.super_block) - 1;
            bg.desc.set_used_dirs_count(&self.super_block, used_dirs);
        }

        // Decrease unused inodes count
        let unused = bg.desc.itable_unused(&self.super_block) + 1;
        bg.desc.set_itable_unused(&self.super_block, unused);

        self.write_block_group_with_csum(&mut bg);

        // Update superblock
        self.super_block.decrease_free_inodes_count();
        self.write_super_block();

        Ok(())
    }
}
