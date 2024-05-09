use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;
use core::cmp::min;

impl Ext4 {
    /// Find the given logic block id in the extent tree, return the search path
    fn find_extent(&self, inode_ref: &mut Ext4InodeRef, iblock: LBlockId) -> Vec<ExtentSearchPath> {
        let mut path: Vec<ExtentSearchPath> = Vec::new();
        let mut eh = inode_ref.inode.extent_header().clone();
        let mut pblock = 0;
        let mut block_data: Vec<u8>;
        // Go until leaf
        while eh.depth() > 0 {
            let index = eh.extent_index_search(iblock);
            if index.is_err() {
                // TODO: no extent index
                panic!("Unhandled error");
            }
            path.push(ExtentSearchPath::new_inner(pblock, index));
            // Get the target extent index
            let ex_idx = eh.extent_index_at(index.unwrap());
            // Load the next extent node
            let next = ex_idx.leaf();
            // Note: block data cannot be released until the next assigment
            block_data = self.block_device.read_offset(next as usize * BLOCK_SIZE);
            // Load the next extent header
            eh = Ext4ExtentHeader::load_from_block(&block_data);
            pblock = next;
        }
        // Leaf
        let index = eh.extent_search(iblock);
        path.push(ExtentSearchPath::new_leaf(pblock, index));

        path
    }

    /// Given a logic block id, find the corresponding fs block id.
    /// Return 0 if not found.
    pub(super) fn extent_get_pblock(&self, inode_ref: &mut Ext4InodeRef, iblock: LBlockId) -> PBlockId {
        let path = self.find_extent(inode_ref, iblock);
        // Leaf is the last element of the path
        let leaf = path.last().unwrap();
        if let Ok(index) = leaf.index {
            // Note: block data must be defined here to keep it alive
            let block_data: Vec<u8>;
            let eh = if leaf.pblock != 0 {
                // Load the extent node
                block_data = self
                    .block_device
                    .read_offset(leaf.pblock as usize * BLOCK_SIZE);
                // Load the next extent header
                Ext4ExtentHeader::load_from_block(&block_data)
            } else {
                // Root node
                inode_ref.inode.extent_header().clone()
            };
            let ex = eh.extent_at(index);
            ex.start_pblock() + (iblock - ex.start_lblock()) as PBlockId
        } else {
            0
        }
    }

    /// Given a logic block id, find the corresponding fs block id.
    /// Create a new extent if not found.
    pub(super) fn extent_get_pblock_create(
        &mut self,
        inode_ref: &mut Ext4InodeRef,
        iblock: LBlockId,
        block_count: u32,
    ) -> PBlockId {
        let path = self.find_extent(inode_ref, iblock);
        // Leaf is the last element of the path
        let leaf = path.last().unwrap();
        // Note: block data must be defined here to keep it alive
        let block_data: Vec<u8>;
        let eh = if leaf.pblock != 0 {
            // Load the extent node
            block_data = self
                .block_device
                .read_offset(leaf.pblock as usize * BLOCK_SIZE);
            // Load the next extent header
            Ext4ExtentHeader::load_from_block(&block_data)
        } else {
            // Root node
            inode_ref.inode.extent_header().clone()
        };
        match leaf.index {
            Ok(index) => {
                // Found, return the corresponding fs block id
                let ex = eh.extent_at(index);
                ex.start_pblock() + (iblock - ex.start_lblock()) as PBlockId
            }
            Err(_) => {
                // Not found, create a new extent
                let block_count = min(block_count, EXT_MAX_BLOCKS - iblock);
                // Allocate physical block
                let fblock = self.alloc_block(inode_ref, 0);
                // Create a new extent
                let new_ext = Ext4Extent::new(iblock, fblock, block_count as u16);
                // Insert the new extent
                self.insert_extent(inode_ref, leaf, &new_ext);
                fblock
            }
        }
    }

    /// Insert a new extent into a leaf node of the extent tree. Return whether
    /// the node needs to be split.
    pub(super) fn insert_extent(
        &self,
        inode_ref: &mut Ext4InodeRef,
        leaf: &ExtentSearchPath,
        new_ext: &Ext4Extent,
    ) -> bool {
        // Note: block data must be defined here to keep it alive
        let mut block_data = Vec::<u8>::new();
        let mut eh = if leaf.pblock != 0 {
            // Load the extent node
            block_data = self
                .block_device
                .read_offset(leaf.pblock as usize * BLOCK_SIZE);
            // Load the next extent header
            Ext4ExtentHeader::load_from_block(&block_data)
        } else {
            // Root node
            inode_ref.inode.extent_header().clone()
        };
        // The position where the new extent should be inserted
        let index = leaf.index.unwrap_err();
        let targ_ext = eh.extent_mut_at(index);
        let split: bool;

        if targ_ext.is_uninit() {
            // 1. The position has an uninitialized extent
            *targ_ext = new_ext.clone();
            split = false;
        } else {
            // 2. The position has a valid extent
            // Insert the extent and move the following extents
            let mut i = index;
            while i < eh.entries_count() as usize {
                *eh.extent_mut_at(i + 1) = *eh.extent_at(i);
                i += 1;
            }
            *eh.extent_mut_at(index) = new_ext.clone();
            eh.set_entries_count(eh.entries_count() + 1);
            // Check if the extent node is full
            split = eh.entries_count() >= eh.max_entries_count();
        }

        if !block_data.is_empty() {
            self.block_device
                .write_offset(leaf.pblock as usize * BLOCK_SIZE, &block_data);
        } else {
            self.write_back_inode_without_csum(inode_ref);
        }

        split
    }
}
