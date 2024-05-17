//! The Defination of Ext4 Extent (Header, Index)
//!
//! Extents are arranged as a tree. Each node of the tree begins with a struct
//! [`Ext4ExtentHeader`].
//!
//! If the node is an interior node (eh.depth > 0), the header is followed by
//! eh.entries_count instances of struct [`Ext4ExtentIndex`]; each of these index
//! entries points to a block containing more nodes in the extent tree.
//!
//! If the node is a leaf node (eh.depth == 0), then the header is followed by
//! eh.entries_count instances of struct [`Ext4Extent`]; these instances point
//! to the file's data blocks. The root node of the extent tree is stored in
//! inode.i_block, which allows for the first four extents to be recorded without
//! the use of extra metadata blocks.

use crate::constants::*;
use crate::prelude::*;

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Ext4ExtentHeader {
    /// Magic number, 0xF30A.
    magic: u16,

    /// Number of valid entries following the header.
    entries_count: u16,

    /// Maximum number of entries that could follow the header.
    max_entries_count: u16,

    /// Depth of this extent node in the extent tree.
    /// 0 = this extent node points to data blocks;
    /// otherwise, this extent node points to other extent nodes.
    /// The extent tree can be at most 5 levels deep:
    /// a logical block number can be at most 2^32,
    /// and the smallest n that satisfies 4*(((blocksize - 12)/12)^n) >= 2^32 is 5.
    depth: u16,

    /// Generation of the tree. (Used by Lustre, but not standard ext4).
    generation: u32,
}

impl Ext4ExtentHeader {
    pub fn new(entries_count: u16, max_entries_count: u16, depth: u16, generation: u32) -> Self {
        Self {
            magic: EXT4_EXTENT_MAGIC,
            entries_count,
            max_entries_count,
            depth,
            generation,
        }
    }

    /// Loads an extent header from a data block.
    pub fn load_from_block(block_data: &[u8]) -> &Self {
        unsafe { &*(block_data.as_ptr() as *const Ext4ExtentHeader) }
    }

    // 获取extent header的魔数
    pub fn magic(&self) -> u16 {
        self.magic
    }

    // 设置extent header的魔数
    pub fn set_magic(&mut self) {
        self.magic = EXT4_EXTENT_MAGIC;
    }

    // 获取extent header的条目数
    pub fn entries_count(&self) -> u16 {
        self.entries_count
    }

    // 设置extent header的条目数
    pub fn set_entries_count(&mut self, count: u16) {
        self.entries_count = count;
    }

    // 获取extent header的最大条目数
    pub fn max_entries_count(&self) -> u16 {
        self.max_entries_count
    }

    // 设置extent header的最大条目数
    pub fn set_max_entries_count(&mut self, max_count: u16) {
        self.max_entries_count = max_count;
    }

    // 获取extent header的深度
    pub fn depth(&self) -> u16 {
        self.depth
    }

    // 设置extent header的深度
    pub fn set_depth(&mut self, depth: u16) {
        self.depth = depth;
    }

    // 获取extent header的生成号
    pub fn generation(&self) -> u32 {
        self.generation
    }

    // 设置extent header的生成号
    pub fn set_generation(&mut self, generation: u32) {
        self.generation = generation;
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Ext4ExtentIndex {
    /// This index node covers file blocks from ‘block’ onward.
    pub first_block: u32,

    /// Lower 32-bits of the block number of the extent node that is
    /// the next level lower in the tree. The tree node pointed to
    /// can be either another internal node or a leaf node, described below.
    pub leaf_lo: u32,

    /// Upper 16-bits of the previous field.
    pub leaf_hi: u16,

    pub padding: u16,
}

impl Ext4ExtentIndex {
    /// The physical block number of the extent node that is the next level lower in the tree
    pub fn leaf(&self) -> PBlockId {
        (self.leaf_hi as PBlockId) << 32 | self.leaf_lo as PBlockId
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Ext4Extent {
    /// First file block number that this extent covers.
    first_block: u32,

    /// Number of blocks covered by extent.
    /// If the value of this field is <= 32768, the extent is initialized.
    /// If the value of the field is > 32768, the extent is uninitialized
    /// and the actual extent length is ee_len - 32768.
    /// Therefore, the maximum length of a initialized extent is 32768 blocks,
    /// and the maximum length of an uninitialized extent is 32767.
    block_count: u16,

    /// Upper 16-bits of the block number to which this extent points.
    start_hi: u16,

    /// Lower 32-bits of the block number to which this extent points.
    start_lo: u32,
}

impl Ext4Extent {
    /// Create a new extent with start logic block number, start physical block number, and block count
    pub fn new(start_lblock: LBlockId, start_pblock: PBlockId, block_count: u16) -> Self {
        Self {
            first_block: start_lblock,
            block_count,
            start_hi: (start_pblock >> 32) as u16,
            start_lo: start_pblock as u32,
        }
    }

    /// The start logic block number that this extent covers
    pub fn start_lblock(&self) -> LBlockId {
        self.first_block
    }

    /// Set the start logic block number that this extent covers
    pub fn set_start_lblock(&mut self, start_lblock: LBlockId) {
        self.first_block = start_lblock;
    }

    /// The start physical block number to which this extent points
    pub fn start_pblock(&self) -> PBlockId {
        self.start_lo as PBlockId | ((self.start_hi as PBlockId) << 32)
    }

    /// Set the start physical block number to which this extent points
    pub fn set_start_pblock(&mut self, start_pblock: PBlockId) {
        self.start_hi = (start_pblock >> 32) as u16;
        self.start_lo = start_pblock as u32;
    }

    /// The actual number of blocks covered by this extent
    pub fn block_count(&self) -> LBlockId {
        (if self.block_count <= EXT_INIT_MAX_LEN {
            self.block_count
        } else {
            self.block_count - EXT_INIT_MAX_LEN
        }) as LBlockId
    }

    /// Set the number of blocks covered by this extent
    pub fn set_block_count(&mut self, block_count: LBlockId) {
        self.block_count = block_count as u16;
    }

    /// Check if the extent is uninitialized
    pub fn is_uninit(&self) -> bool {
        self.block_count > EXT_INIT_MAX_LEN
    }

    /// Mark the extent as uninitialized
    pub fn mark_uninit(&mut self) {
        (*self).block_count |= EXT_INIT_MAX_LEN;
    }

    /// Check whether the `ex2` extent can be appended to the `ex1` extent
    pub fn can_append(ex1: &Ext4Extent, ex2: &Ext4Extent) -> bool {
        if ex1.start_pblock() + ex1.block_count() as u64 != ex2.start_pblock() {
            return false;
        }
        if ex1.is_uninit()
            && ex1.block_count() + ex2.block_count() > EXT_UNWRITTEN_MAX_LEN as LBlockId
        {
            return false;
        } else if ex1.block_count() + ex2.block_count() > EXT_INIT_MAX_LEN as LBlockId {
            return false;
        }
        // 检查逻辑块号是否连续
        if ex1.first_block + ex1.block_count() as u32 != ex2.first_block {
            return false;
        }
        return true;
    }
}

/// Interpret an immutable byte slice as an extent node. Provide methods to
/// access the extent header and the following extents or extent indices.
///
/// The underlying `raw_data` could be of `[u32;15]` (root node) or a
/// data block `[u8;BLOCK_SIZE]` (other node).
pub struct ExtentNode<'a> {
    raw_data: &'a [u8],
}

impl<'a> ExtentNode<'a> {
    /// Interpret a byte slice as an extent node
    pub fn from_bytes(raw_data: &'a [u8]) -> Self {
        Self { raw_data }
    }

    /// Get a immutable reference to the extent header
    pub fn header(&self) -> &Ext4ExtentHeader {
        unsafe { &*(self.raw_data.as_ptr() as *const Ext4ExtentHeader) }
    }

    /// Get a immutable reference to the extent at a given index
    pub fn extent_at(&self, index: usize) -> &Ext4Extent {
        unsafe {
            &*((self.header() as *const Ext4ExtentHeader).add(1) as *const Ext4Extent).add(index)
        }
    }

    /// Get a immmutable reference to the extent indexat a given index
    pub fn extent_index_at(&self, index: usize) -> &Ext4ExtentIndex {
        unsafe {
            &*((self.header() as *const Ext4ExtentHeader).add(1) as *const Ext4ExtentIndex)
                .add(index)
        }
    }

    /// Find the extent that covers the given logical block number.
    ///
    /// Return `Ok(index)` if found, and `eh.extent_at(index)` is the extent that covers
    /// the given logical block number. Return `Err(index)` if not found, and `index` is the
    /// position where the new extent should be inserted.
    pub fn extent_search(&self, lblock: LBlockId) -> core::result::Result<usize, usize> {
        let mut i = 0;
        while i < self.header().entries_count as usize {
            let extent = self.extent_at(i);
            if extent.start_lblock() <= lblock {
                if extent.start_lblock() + (extent.block_count() as LBlockId) > lblock {
                    return if extent.is_uninit() { Err(i) } else { Ok(i) };
                }
                i += 1;
            } else {
                break;
            }
        }
        Err(i)
    }

    /// Find the extent index that covers the given logical block number. The extent index
    /// gives the next lower node to search.
    ///
    /// Return `Ok(index)` if found, and `eh.extent_index_at(index)` is the target extent index.
    /// Return `Err(index)` if not found, and `index` is the position where the new extent index
    /// should be inserted.
    pub fn extent_index_search(&self, lblock: LBlockId) -> core::result::Result<usize, usize> {
        let mut i = 0;
        self.print();
        while i < self.header().entries_count as usize {
            let extent_index = self.extent_index_at(i);
            if extent_index.first_block <= lblock {
                i += 1;
            } else {
                return Ok(i - 1);
            }
        }
        Err(i)
    }

    pub fn print(&self) {
        debug!("Extent header {:?}", self.header());
        let mut i = 0;
        while i < self.header().entries_count() as usize {
            let ext = self.extent_at(i);
            debug!(
                "extent[{}] start_lblock={}, start_pblock={}, len={}",
                i,
                ext.start_lblock(),
                ext.start_pblock(),
                ext.block_count()
            );
            i += 1;
        }
    }
}

/// Interpret a mutable byte slice as an extent node. Provide methods to
/// modify the extent header and the following extents or extent indices.
///
/// The underlying `raw_data` could be of `[u8;15]` (root node) or a
/// data block `[u8;BLOCK_SIZE]` (other node).
pub struct ExtentNodeMut<'a> {
    raw_data: &'a mut [u8],
}

impl<'a> ExtentNodeMut<'a> {
    /// Interpret a byte slice as an extent node
    pub fn from_bytes(raw_data: &'a mut [u8]) -> Self {
        Self { raw_data }
    }

    /// Get a immutable reference to the extent header
    pub fn header(&self) -> &Ext4ExtentHeader {
        unsafe { &*(self.raw_data.as_ptr() as *const Ext4ExtentHeader) }
    }

    /// Get a mutable reference to the extent header
    pub fn header_mut(&mut self) -> &mut Ext4ExtentHeader {
        unsafe { &mut *(self.raw_data.as_mut_ptr() as *mut Ext4ExtentHeader) }
    }

    /// Get a immutable reference to the extent at a given index
    pub fn extent_at(&self, index: usize) -> &'static Ext4Extent {
        unsafe {
            &*((self.header() as *const Ext4ExtentHeader).add(1) as *const Ext4Extent).add(index)
        }
    }

    /// Get a mutable reference to the extent at a given index
    pub fn extent_mut_at(&mut self, index: usize) -> &'static mut Ext4Extent {
        unsafe {
            &mut *((self.header_mut() as *mut Ext4ExtentHeader).add(1) as *mut Ext4Extent)
                .add(index)
        }
    }

    /// Get a immutable reference to the extent index at a given index
    pub fn extent_index_at(&self, index: usize) -> &'static Ext4ExtentIndex {
        unsafe {
            &*((self.header() as *const Ext4ExtentHeader).add(1) as *const Ext4ExtentIndex)
                .add(index)
        }
    }

    /// Get a mutable reference to the extent index at a given index
    pub fn extent_index_mut_at(&mut self, index: usize) -> &'static mut Ext4ExtentIndex {
        unsafe {
            &mut *((self.header_mut() as *mut Ext4ExtentHeader).add(1) as *mut Ext4ExtentIndex)
                .add(index)
        }
    }

    pub fn print(&self) {
        debug!("Extent header {:?}", self.header());
        let mut i = 0;
        while i < self.header().entries_count() as usize {
            let ext = self.extent_at(i);
            debug!(
                "extent[{}] start_lblock={}, start_pblock={}, len={}",
                i,
                ext.start_lblock(),
                ext.start_pblock(),
                ext.block_count()
            );
            i += 1;
        }
    }
}

#[derive(Debug)]
pub struct ExtentSearchPath {
    /// Marks whether the extent search path is for an inner node or a leaf node.
    pub leaf: bool,
    /// The physical block where this extent node is stored if it is not a root node.
    /// For a root node, this field is 0.
    pub pblock: PBlockId,
    /// Index of the found `ExtentIndex` or `Extent` if found, the position where the
    /// `ExtentIndex` or `Extent` should be inserted if not found.
    pub index: core::result::Result<usize, usize>,
}

impl ExtentSearchPath {
    /// Create a new extent search path, which records an inner node
    /// of the extent tree i.e. an `ExtentIndex`.
    pub fn new_inner(pblock: PBlockId, index: core::result::Result<usize, usize>) -> Self {
        Self {
            pblock,
            leaf: false,
            index,
        }
    }

    /// Create a new extent search path, which records a leaf node
    /// of the extent tree i.e. an `Extent`.
    pub fn new_leaf(pblock: PBlockId, index: core::result::Result<usize, usize>) -> Self {
        Self {
            pblock,
            leaf: true,
            index,
        }
    }
}
