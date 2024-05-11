use crate::prelude::*;
use crate::constants::*;
use super::BlockDevice;

#[derive(Debug)]
// A single block descriptor
pub struct Ext4Block<'a> {
    /// Physical block id
    pub pblock_id: PBlockId,
    /// Raw block data
    pub block_data: &'a mut [u8],
}

impl<'a> Ext4Block<'a> {
    pub fn sync_to_disk(&self, block_device: Arc<dyn BlockDevice>) {
        let block_id = self.pblock_id as usize;
        block_device.write_offset(block_id * BLOCK_SIZE, &self.block_data);
    }
}