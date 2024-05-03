use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

impl Ext4 {
    pub fn find_extent(
        inode_ref: &mut Ext4InodeRef,
        block_id: Ext4LogicBlockId,
        orig_path: &mut Option<Vec<Ext4ExtentPath>>,
        _flags: u32,
    ) -> usize {
        let inode = &inode_ref.inode;
        let mut _eh: &Ext4ExtentHeader;
        let mut path = orig_path.take(); // Take the path out of the Option, which may replace it with None
        let depth = ext4_depth(inode);

        let mut ppos = 0;
        let mut i: u16;

        let eh = &inode.block as *const [u32; 15] as *mut Ext4ExtentHeader;

        if let Some(ref mut p) = path {
            if depth > p[0].maxdepth {
                p.clear();
            }
        }
        if path.is_none() {
            let path_depth = depth + 1;
            path = Some(vec![Ext4ExtentPath::default(); path_depth as usize + 1]);
            path.as_mut().unwrap()[0].maxdepth = path_depth;
        }

        let path = path.as_mut().unwrap();
        path[0].header = eh;

        i = depth;
        while i > 0 {
            ext4_ext_binsearch_idx(&mut path[ppos], block_id);
            path[ppos].p_block = ext4_idx_pblock(path[ppos].index);
            path[ppos].depth = i;
            path[ppos].extent = core::ptr::null_mut();

            i -= 1;
            ppos += 1;
        }

        path[ppos].depth = i;
        path[ppos].extent = core::ptr::null_mut();
        path[ppos].index = core::ptr::null_mut();

        ext4_ext_binsearch(&mut path[ppos], block_id);
        if !path[ppos].extent.is_null() {
            path[ppos].p_block = unsafe { (*path[ppos].extent).pblock() };
        }

        *orig_path = Some(path.clone());

        EOK
    }

    pub fn ext4_extent_get_blocks(
        &self,
        inode_ref: &mut Ext4InodeRef,
        iblock: Ext4LogicBlockId,
        max_blocks: u32,
        result: &mut Ext4FsBlockId,
        create: bool,
        blocks_count: &mut u32,
    ) {
        *result = 0;
        *blocks_count = 0;

        let mut path: Option<Vec<Ext4ExtentPath>> = None;
        let err = Self::find_extent(inode_ref, iblock, &mut path, 0);

        let inode = &inode_ref.inode;
        // 确认ext4_find_extent成功执行
        if err != EOK {
            return;
        }

        let depth = unsafe { *ext4_extent_hdr(inode) }.depth as usize;
        let mut path = path.unwrap();

        if !path[depth].extent.is_null() {
            let ex = unsafe { *path[depth].extent };
            let ee_block = ex.first_block;
            let ee_start = ex.pblock();
            let ee_len = ex.actual_len();

            if iblock >= ee_block && iblock < ee_block + ee_len as u32 {
                let allocated = ee_len - (iblock - ee_block) as u16;
                *blocks_count = allocated as u32;

                if !create || ex.is_unwritten() {
                    *result = (iblock - ee_block) as u64 + ee_start;
                    return;
                }
            }
        }

        // 如果没有找到对应的extent，并且create为true，则需要分配和插入新的extent
        if create {
            let next = EXT_MAX_BLOCKS;

            let mut allocated = next - iblock;
            if allocated > max_blocks {
                allocated = max_blocks;
            }

            let mut newex: Ext4Extent = Ext4Extent::default();

            let goal = 0;

            let mut alloc_block = 0;
            self.ext4_balloc_alloc_block(inode_ref, goal as u64, &mut alloc_block);

            *result = alloc_block;

            // 创建并插入新的extent
            newex.first_block = iblock;
            newex.start_lo = alloc_block as u32 & 0xffffffff;
            newex.start_hi = (((alloc_block as u32) << 31) << 1) as u16;
            newex.block_count = allocated as u16;

            self.ext4_ext_insert_extent(inode_ref, &mut path[0], &newex, 0);
        }
    }

    pub fn ext4_ext_insert_extent(
        &self,
        inode_ref: &mut Ext4InodeRef,
        path: &mut Ext4ExtentPath,
        newext: &Ext4Extent,
        flags: i32,
    ) {
        let depth = ext4_depth(&inode_ref.inode);
        let mut need_split = false;

        self.ext4_ext_insert_leaf(inode_ref, path, depth, newext, flags, &mut need_split);

        self.write_back_inode_without_csum(inode_ref);
    }

    pub fn ext4_ext_insert_leaf(
        &self,
        _inode_ref: &mut Ext4InodeRef,
        path: &mut Ext4ExtentPath,
        _depth: u16,
        newext: &Ext4Extent,
        _flags: i32,
        need_split: &mut bool,
    ) -> usize {
        let eh = path.header;
        let ex = path.extent;
        let _last_ex = ext4_last_extent(eh);

        let mut diskblock = newext.start_lo;
        diskblock |= ((newext.start_hi as u32) << 31) << 1;

        unsafe {
            if !ex.is_null() && Ext4Extent::can_append(&*(path.extent), newext) {
                if (*path.extent).is_unwritten() {
                    (*path.extent).mark_unwritten();
                }
                (*(path.extent)).block_count = (*path.extent).actual_len() + newext.actual_len();
                (*path).p_block = diskblock as u64;
                return EOK;
            }
            if !ex.is_null() && Ext4Extent::can_append(newext, &*(path.extent)) {
                (*(path.extent)).block_count = (*path.extent).actual_len() + newext.actual_len();
                (*path).p_block = diskblock as u64;
                if (*path.extent).is_unwritten() {
                    (*path.extent).mark_unwritten();
                }
                return EOK;
            }
        }

        if ex.is_null() {
            let first_extent = ext4_first_extent_mut(eh);
            path.extent = first_extent;
            // log::info!("first_extent {:x?}", unsafe{*first_extent});
            unsafe {
                if (*eh).entries_count == (*eh).max_entries_count {
                    *need_split = true;
                    return EIO;
                } else {
                    *(path.extent) = *newext;
                }
            }
        }

        unsafe {
            if (*eh).entries_count == (*eh).max_entries_count {
                *need_split = true;
                *(path.extent) = *newext;

                (*path).p_block = diskblock as u64;
                return EIO;
            } else {
                if ex.is_null() {
                    let first_extent = ext4_first_extent_mut(eh);
                    path.extent = first_extent;
                    *(path.extent) = *newext;
                } else if newext.first_block > (*(path.extent)).first_block {
                    // insert after
                    let next_extent = ex.add(1);
                    path.extent = next_extent;
                } else {
                }
            }

            *(path.extent) = *newext;
            (*eh).entries_count += 1;
        }
        unsafe {
            *(path.extent) = *newext;
        }

        return EOK;
    }

    pub fn ext4_find_all_extent(&self, inode_ref: &Ext4InodeRef, extents: &mut Vec<Ext4Extent>) {
        let extent_header = Ext4ExtentHeader::try_from(&inode_ref.inode.block[..2]).unwrap();
        // log::info!("extent_header {:x?}", extent_header);
        let data = &inode_ref.inode.block;
        let depth = extent_header.depth;
        self.ext4_add_extent(inode_ref, depth, data, extents, true);
    }

    pub fn ext4_add_extent(
        &self,
        inode_ref: &Ext4InodeRef,
        depth: u16,
        data: &[u32],
        extents: &mut Vec<Ext4Extent>,
        _first_level: bool,
    ) {
        let extent_header = Ext4ExtentHeader::try_from(data).unwrap();
        let extent_entries = extent_header.entries_count;
        // log::info!("extent_entries {:x?}", extent_entries);
        if depth == 0 {
            for en in 0..extent_entries {
                let idx = (3 + en * 3) as usize;
                let extent = Ext4Extent::try_from(&data[idx..]).unwrap();

                extents.push(extent)
            }
            return;
        }

        for en in 0..extent_entries {
            let idx = (3 + en * 3) as usize;
            if idx == 12 {
                break;
            }
            let extent_index = Ext4ExtentIndex::try_from(&data[idx..]).unwrap();
            let ei_leaf_lo = extent_index.leaf_lo;
            let ei_leaf_hi = extent_index.leaf_hi;
            let mut block = ei_leaf_lo;
            block |= ((ei_leaf_hi as u32) << 31) << 1;
            let data = self.block_device.read_offset(block as usize * BLOCK_SIZE);
            let data: Vec<u32> = unsafe { core::mem::transmute(data) };
            self.ext4_add_extent(inode_ref, depth - 1, &data, extents, false);
        }
    }

    pub fn ext4_fs_get_inode_dblk_idx(
        &self,
        inode_ref: &mut Ext4InodeRef,
        iblock: &mut Ext4LogicBlockId,
        fblock: &mut Ext4FsBlockId,
        _extent_create: bool,
    ) -> usize {
        let current_block: Ext4FsBlockId;
        let mut current_fsblk: Ext4FsBlockId = 0;
        let mut blocks_count = 0;
        self.ext4_extent_get_blocks(
            inode_ref,
            *iblock,
            1,
            &mut current_fsblk,
            false,
            &mut blocks_count,
        );
        current_block = current_fsblk;
        *fblock = current_block;
        EOK
    }

    pub fn ext4_fs_get_inode_dblk_idx_internal(
        &self,
        inode_ref: &mut Ext4InodeRef,
        iblock: &mut Ext4LogicBlockId,
        _fblock: &mut Ext4FsBlockId,
        extent_create: bool,
        _support_unwritten: bool,
    ) {
        let mut current_fsblk: Ext4FsBlockId = 0;
        let mut blocks_count = 0;
        self.ext4_extent_get_blocks(
            inode_ref,
            *iblock,
            1,
            &mut current_fsblk,
            extent_create,
            &mut blocks_count,
        );
    }
}
