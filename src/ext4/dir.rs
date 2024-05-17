use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

impl Ext4 {
    /// Find a directory entry that matches a given name under a parent directory
    pub(super) fn dir_find_entry(&self, parent: &InodeRef, name: &str) -> Result<DirEntry> {
        info!("Dir find entry: {} under parent {}", name, parent.inode_id);
        let inode_size: u32 = parent.inode.size;
        let total_blocks: u32 = inode_size / BLOCK_SIZE as u32;
        let mut iblock: LBlockId = 0;

        while iblock < total_blocks {
            // Get the fs block id
            debug!("Parent {:?}", parent.inode);
            let fblock = self.extent_get_pblock(parent, iblock)?;
            // Load block from disk
            let block = self.block_device.read_block(fblock);
            // Find the entry in block
            let res = Self::find_entry_in_block(&block, name);
            if let Ok(r) = res {
                return Ok(r);
            }
            iblock += 1
        }
        Err(Ext4Error::new(ErrCode::ENOENT))
    }

    /// Find a directory entry that matches a given name in a given block
    fn find_entry_in_block(block: &Block, name: &str) -> Result<DirEntry> {
        let mut offset = 0;
        while offset < BLOCK_SIZE {
            let de = DirEntry::from_bytes(&block.data[offset..]);
            offset += de.rec_len() as usize;
            // Unused dir entry
            if de.unused() {
                continue;
            }
            // Compare name
            if de.compare_name(name) {
                return Ok(de);
            }
        }
        Err(Ext4Error::new(ErrCode::ENOENT))
    }

    /// Add an entry to a directory
    pub(super) fn dir_add_entry(
        &mut self,
        parent: &mut InodeRef,
        child: &InodeRef,
        path: &str,
    ) -> Result<()> {
        info!(
            "Dir add entry: parent {}, child {}, path {}",
            parent.inode_id, child.inode_id, path
        );
        let inode_size = parent.inode.size();
        let total_blocks = inode_size as u32 / BLOCK_SIZE as u32;

        // Try finding a block with enough space
        let mut iblock: LBlockId = 0;
        while iblock < total_blocks {
            // Get the parent physical block id, create if not exist
            let fblock = self.extent_get_pblock_create(parent, iblock, 1)?;
            // Load the parent block from disk
            let mut block = self.block_device.read_block(fblock);
            // Try inserting the entry to parent block
            self.insert_entry_to_old_block(&mut block, child, path)?;
            // Current block has no enough space
            iblock += 1;
        }

        // No free block found - needed to allocate a new data block
        // Append a new data block
        let (_, fblock) = self.inode_append_block(parent)?;
        // Load new block
        let mut new_block = self.block_device.read_block(fblock);
        // Write the entry to block
        self.insert_entry_to_new_block(&mut new_block, child, path)
    }

    /// Insert a directory entry of a child inode into a new parent block.
    /// A new block must have enough space
    fn insert_entry_to_new_block(
        &self,
        dst_blk: &mut Block,
        child: &InodeRef,
        name: &str,
    ) -> Result<()> {
        // Set the entry
        let rec_len = BLOCK_SIZE - size_of::<DirEntryTail>();
        let new_entry = DirEntry::new(
            child.inode_id,
            rec_len as u16,
            name,
            inode_mode2file_type(child.inode.mode()),
        );

        // Write entry to block
        new_entry.copy_to_byte_slice(&mut dst_blk.data, 0);

        // Set tail
        let mut tail = DirEntryTail::default();
        tail.rec_len = size_of::<DirEntryTail>() as u16;
        tail.reserved_ft = 0xDE;
        tail.set_csum(&self.super_block, &new_entry, &dst_blk.data[..]);
        // Copy tail to block
        let tail_offset = BLOCK_SIZE - size_of::<DirEntryTail>();
        tail.copy_to_byte_slice(&mut dst_blk.data, tail_offset);

        // Sync block to disk
        dst_blk.sync_to_disk(self.block_device.clone());
        Ok(())
    }

    /// Try insert a directory entry of child inode into a parent block.
    /// Return `ENOSPC` if parent block has no enough space.
    fn insert_entry_to_old_block(
        &self,
        dst_blk: &mut Block,
        child: &InodeRef,
        name: &str,
    ) -> Result<()> {
        let required_size = DirEntry::required_size(name.len());
        let mut offset = 0;

        while offset < dst_blk.data.len() {
            let mut de = DirEntry::from_bytes(&dst_blk.data[offset..]);
            let rec_len = de.rec_len() as usize;

            // Try splitting dir entry
            // The size that `de` actually uses
            let used_size = de.used_size();
            // The rest size
            let free_size = rec_len - used_size;

            // Compare size
            if free_size < required_size {
                // No enough space, try next dir ent
                offset = offset + rec_len;
                continue;
            }
            // Has enough space
            // Update the old entry
            de.set_rec_len(used_size as u16);
            de.copy_to_byte_slice(&mut dst_blk.data, offset);
            // Insert the new entry
            let new_entry = DirEntry::new(
                child.inode_id,
                free_size as u16,
                name,
                inode_mode2file_type(child.inode.mode()),
            );
            new_entry.copy_to_byte_slice(&mut dst_blk.data, offset + used_size);

            // Set tail csum
            let mut tail =
                DirEntryTail::from_bytes(&mut dst_blk.data, BLOCK_SIZE).unwrap();
            tail.set_csum(&self.super_block, &de, &dst_blk.data[offset..]);
            // Write tail to blk_data
            let tail_offset = BLOCK_SIZE - size_of::<DirEntryTail>();
            tail.copy_to_byte_slice(&mut dst_blk.data, tail_offset);

            // Sync to disk
            dst_blk.sync_to_disk(self.block_device.clone());
            return Ok(());
        }
        Err(Ext4Error::new(ErrCode::ENOSPC))
    }

    /// Create a new directory. `path` is the absolute path of the new directory.
    pub fn mkdir(&mut self, path: &str) -> Result<()> {
        // get open flags
        let iflags = OpenFlags::from_str("w").unwrap();
        self.generic_open(
            path,
            iflags,
            FileType::Directory,
            &self.get_root_inode_ref(),
        )
        .map(|_| {
            info!("ext4_dir_mk: {} ok", path);
        })
    }
}
