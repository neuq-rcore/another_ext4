use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;
use core::cmp::min;

impl Ext4 {
    /// Find the given logic block id in the extent tree, return the search path
    fn find_extent(&self, inode_ref: &InodeRef, iblock: LBlockId) -> Vec<ExtentSearchPath> {
        let mut path: Vec<ExtentSearchPath> = Vec::new();
        let mut ex_node = inode_ref.inode.extent();
        ex_node.print();
        let mut pblock = 0;
        let mut block_data: Vec<u8>;

        // Go until leaf
        while ex_node.header().depth() > 0 {
            let index = ex_node.extent_index_search(iblock);
            if index.is_err() {
                // TODO: no extent index
                panic!("Unhandled error");
            }
            path.push(ExtentSearchPath::new_inner(pblock, index));
            // Get the target extent index
            let ex_idx = ex_node.extent_index_at(index.unwrap());
            // Load the next extent node
            let next = ex_idx.leaf();
            // Note: block data cannot be released until the next assigment
            block_data = self.block_device.read_offset(next as usize * BLOCK_SIZE);
            // Load the next extent header
            ex_node = ExtentNode::from_bytes(&block_data);
            pblock = next;
        }

        // Leaf
        let index = ex_node.extent_search(iblock);
        debug!("Extent search {} res {:?}", iblock, index);
        path.push(ExtentSearchPath::new_leaf(pblock, index));

        path
    }

    /// Given a logic block id, find the corresponding fs block id.
    /// Return 0 if not found.
    pub(super) fn extent_get_pblock(
        &self,
        inode_ref: &InodeRef,
        iblock: LBlockId,
    ) -> Result<PBlockId> {
        let path = self.find_extent(inode_ref, iblock);
        // Leaf is the last element of the path
        let leaf = path.last().unwrap();
        if let Ok(index) = leaf.index {
            // Note: block data must be defined here to keep it alive
            let block_data: Vec<u8>;
            let ex_node = if leaf.pblock != 0 {
                // Load the extent node
                block_data = self
                    .block_device
                    .read_offset(leaf.pblock as usize * BLOCK_SIZE);
                // Load the next extent header
                ExtentNode::from_bytes(&block_data)
            } else {
                // Root node
                inode_ref.inode.extent()
            };
            let ex = ex_node.extent_at(index);
            Ok(ex.start_pblock() + (iblock - ex.start_lblock()) as PBlockId)
        } else {
            Err(Ext4Error::new(ErrCode::ENOENT))
        }
    }

    /// Given a logic block id, find the corresponding fs block id.
    /// Create a new extent if not found.
    pub(super) fn extent_get_pblock_create(
        &mut self,
        inode_ref: &mut InodeRef,
        iblock: LBlockId,
        block_count: u32,
    ) -> Result<PBlockId> {
        let path = self.find_extent(inode_ref, iblock);
        // Leaf is the last element of the path
        let leaf = path.last().unwrap();
        // Note: block data must be defined here to keep it alive
        let mut block_data: Vec<u8>;
        let ex_node = if leaf.pblock != 0 {
            // Load the extent node
            block_data = self
                .block_device
                .read_offset(leaf.pblock as usize * BLOCK_SIZE);
            // Load the next extent header
            ExtentNodeMut::from_bytes(&mut block_data)
        } else {
            // Root node
            inode_ref.inode.extent_mut()
        };
        match leaf.index {
            Ok(index) => {
                // Found, return the corresponding fs block id
                let ex = ex_node.extent_at(index);
                Ok(ex.start_pblock() + (iblock - ex.start_lblock()) as PBlockId)
            }
            Err(_) => {
                // Not found, create a new extent
                let block_count = min(block_count, EXT_MAX_BLOCKS - iblock);
                // Allocate physical block
                let fblock = self.alloc_block(inode_ref, 0)?;
                // Create a new extent
                let new_ext = Ext4Extent::new(iblock, fblock, block_count as u16);
                // Insert the new extent
                self.insert_extent(inode_ref, leaf, &new_ext);
                Ok(fblock)
            }
        }
    }

    /// Insert a new extent into a leaf node of the extent tree. Return whether
    /// the node needs to be split.
    pub(super) fn insert_extent(
        &self,
        inode_ref: &mut InodeRef,
        leaf: &ExtentSearchPath,
        new_ext: &Ext4Extent,
    ) -> bool {
        // Note: block data must be defined here to keep it alive
        let mut block_data = Vec::<u8>::new();
        let mut ex_node = if leaf.pblock != 0 {
            // Load the extent node
            block_data = self
                .block_device
                .read_offset(leaf.pblock as usize * BLOCK_SIZE);
            // Load the next extent header
            ExtentNodeMut::from_bytes(&mut block_data)
        } else {
            // Root node
            inode_ref.inode.extent_mut()
        };
        // The position where the new extent should be inserted
        let index = leaf.index.unwrap_err();
        let targ_ext = ex_node.extent_mut_at(index);
        let split: bool;

        debug!("Create extent");
        if targ_ext.is_uninit() {
            // 1. The position has an uninitialized extent
            *targ_ext = new_ext.clone();

            let en_count = ex_node.header().entries_count() + 1;
            ex_node.header_mut().set_entries_count(en_count);
            split = false;
        } else {
            // 2. The position has a valid extent
            // Insert the extent and move the following extents
            let mut i = index;
            while i < ex_node.header().entries_count() as usize {
                *ex_node.extent_mut_at(i + 1) = *ex_node.extent_at(i);
                i += 1;
            }
            *ex_node.extent_mut_at(index) = new_ext.clone();

            let en_count = ex_node.header().entries_count() + 1;
            ex_node.header_mut().set_entries_count(en_count);
            // Check if the extent node is full
            split = ex_node.header().entries_count() >= ex_node.header().max_entries_count();
        }

        ex_node.print();
        // Write back to disk
        if !block_data.is_empty() {
            self.block_device
                .write_offset(leaf.pblock as usize * BLOCK_SIZE, &block_data);
        } else {
            self.write_back_inode_without_csum(inode_ref);
        }

        split
    }
}
