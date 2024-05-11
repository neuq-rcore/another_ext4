use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

impl Ext4 {
    /// Find a directory entry that matches a given name under a parent directory
    pub(super) fn dir_find_entry(&self, parent: &Ext4InodeRef, name: &str) -> Result<Ext4DirEntry> {
        info!("dir find entry: {} under parent {}", name, parent.inode_id);
        let inode_size: u32 = parent.inode.size;
        let total_blocks: u32 = inode_size / BLOCK_SIZE as u32;
        let mut iblock: LBlockId = 0;

        while iblock < total_blocks {
            // Get the fs block id
            let fblock = self.extent_get_pblock(parent, iblock)?;
            // Load block from disk
            let block_data = self.block_device.read_offset(fblock as usize * BLOCK_SIZE);
            // Find the entry in block
            let res = Self::find_entry_in_block(&block_data, name);
            if let Ok(r) = res {
                return Ok(r);
            }
            iblock += 1
        }
        Err(Ext4Error::new(ErrCode::ENOENT))
    }

    /// Find a directory entry that matches a given name in a given block
    ///
    /// Save the result in `Ext4DirSearchResult`
    fn find_entry_in_block(block_data: &[u8], name: &str) -> Result<Ext4DirEntry> {
        let mut offset = 0;
        while offset < block_data.len() {
            let de = Ext4DirEntry::from_bytes(&block_data[offset..]);
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
        parent: &mut Ext4InodeRef,
        child: &Ext4InodeRef,
        path: &str,
    ) -> Result<()> {
        info!(
            "Adding entry: parent {}, child {}, path {}",
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
            let mut data = self.block_device.read_offset(fblock as usize * BLOCK_SIZE);
            let mut ext4_block = Ext4Block {
                logical_block_id: iblock,
                disk_block_id: fblock,
                block_data: &mut data,
                dirty: false,
            };
            debug!("Insert dirent to old block {}", fblock);
            // Try inserting the entry to parent block
            self.insert_entry_to_old_block(&mut ext4_block, child, path)?;
            // Current block has no enough space
            iblock += 1;
        }

        // No free block found - needed to allocate a new data block
        // Append a new data block
        let (iblock, fblock) = self.inode_append_block(parent)?;
        // Load new block
        let block_device = self.block_device.clone();
        let mut data = block_device.read_offset(fblock as usize * BLOCK_SIZE);
        let mut new_block = Ext4Block {
            logical_block_id: iblock,
            disk_block_id: fblock,
            block_data: &mut data,
            dirty: false,
        };
        debug!("Insert dirent to new block {}", fblock);
        // Write the entry to block
        self.insert_entry_to_new_block(&mut new_block, child, path)
    }

    /// Insert a directory entry of a child inode into a new parent block.
    /// A new block must have enough space
    fn insert_entry_to_new_block(
        &self,
        dst_blk: &mut Ext4Block,
        child: &Ext4InodeRef,
        name: &str,
    ) -> Result<()> {
        // Set the entry
        let rec_len = BLOCK_SIZE - size_of::<Ext4DirEntryTail>();
        let new_entry = Ext4DirEntry::new(
            child.inode_id,
            rec_len as u16,
            name,
            inode_mode2file_type(child.inode.mode()),
        );

        // Write entry to block
        new_entry.copy_to_byte_slice(&mut dst_blk.block_data, 0);

        // Set tail
        let mut tail = Ext4DirEntryTail::default();
        tail.rec_len = size_of::<Ext4DirEntryTail>() as u16;
        tail.reserved_ft = 0xDE;
        tail.set_csum(&self.super_block, &new_entry, &dst_blk.block_data[..]);
        // Copy tail to block
        let tail_offset = BLOCK_SIZE - size_of::<Ext4DirEntryTail>();
        tail.copy_to_byte_slice(&mut dst_blk.block_data, tail_offset);

        // Sync block to disk
        dst_blk.sync_to_disk(self.block_device.clone());
        Ok(())
    }

    /// Try insert a directory entry of child inode into a parent block.
    /// Return `ENOSPC` if parent block has no enough space.
    fn insert_entry_to_old_block(
        &self,
        dst_blk: &mut Ext4Block,
        child: &Ext4InodeRef,
        name: &str,
    ) -> Result<()> {
        let required_size = Ext4DirEntry::required_size(name.len());
        let mut offset = 0;

        while offset < dst_blk.block_data.len() {
            let mut de = Ext4DirEntry::from_bytes(&dst_blk.block_data[offset..]);
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
            de.copy_to_byte_slice(&mut dst_blk.block_data, offset);
            // Insert the new entry
            let new_entry = Ext4DirEntry::new(
                child.inode_id,
                free_size as u16,
                name,
                inode_mode2file_type(child.inode.mode()),
            );
            new_entry.copy_to_byte_slice(&mut dst_blk.block_data, offset + used_size);

            // Set tail csum
            let mut tail =
                Ext4DirEntryTail::from_bytes(&mut dst_blk.block_data, BLOCK_SIZE).unwrap();
            tail.set_csum(&self.super_block, &de, &dst_blk.block_data[offset..]);
            // Write tail to blk_data
            let tail_offset = BLOCK_SIZE - size_of::<Ext4DirEntryTail>();
            tail.copy_to_byte_slice(&mut dst_blk.block_data, tail_offset);

            // Sync to disk
            dst_blk.sync_to_disk(self.block_device.clone());
            return Ok(());
        }
        Err(Ext4Error::new(ErrCode::ENOSPC))
    }

    /// Create a new directory. `path` is the absolute path of the new directory.
    pub fn ext4_dir_mk(&mut self, path: &str) -> Result<()> {
        // get open flags
        let iflags = OpenFlags::from_str("w").unwrap();
        self.generic_open(
            path,
            iflags,
            FileType::Directory,
            &self.get_root_inode_ref(),
        )
        .map(|_| {
            info!("ext4_dir_mk: {}", path);
        })
    }
}
