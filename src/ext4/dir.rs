use super::utils::*;
use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;

impl Ext4 {
    pub fn ext4_dir_mk(&self, path: &str) -> Result<usize> {
        let mut file = Ext4File::new();
        let flags = "w";

        let filetype = DirEntryType::EXT4_DE_DIR;

        // get mount point
        let mut ptr = Box::new(self.mount_point.clone());
        file.mp = Box::as_mut(&mut ptr) as *mut Ext4MountPoint;

        // get open flags
        let iflags = ext4_parse_flags(flags).unwrap();

        if iflags & O_CREAT != 0 {
            self.ext4_trans_start();
        }

        let mut root_inode_ref = self.get_root_inode_ref();

        let r = self.ext4_generic_open(
            &mut file,
            path,
            iflags,
            filetype.bits(),
            &mut root_inode_ref,
        );
        r
    }

    pub fn ext4_dir_add_entry(
        &self,
        parent: &mut Ext4InodeRef,
        child: &mut Ext4InodeRef,
        path: &str,
        len: u32,
    ) -> usize {
        let mut iblock = 0;
        let block_size = self.super_block.block_size();
        let inode_size = parent.inode.size();
        let total_blocks = inode_size as u32 / block_size;

        let mut fblock: Ext4FsBlockId = 0;

        // log::info!("ext4_dir_add_entry parent inode {:x?} inode_size {:x?}", parent.inode_num, inode_size);
        while iblock < total_blocks {
            self.ext4_fs_get_inode_dblk_idx(parent, &mut iblock, &mut fblock, false);

            // load_block
            let mut data = self.block_device.read_offset(fblock as usize * BLOCK_SIZE);
            let mut ext4_block = Ext4Block {
                logical_block_id: iblock,
                disk_block_id: fblock,
                block_data: &mut data,
                dirty: false,
            };
            let r = self.ext4_dir_try_insert_entry(parent, &mut ext4_block, child, path, len);
            if r == EOK {
                return EOK;
            }

            let mut data: Vec<u8> = Vec::with_capacity(BLOCK_SIZE);
            let ext4_blk = Ext4Block {
                logical_block_id: 0,
                disk_block_id: 0,
                block_data: &mut data,
                dirty: true,
            };
            let de = Ext4DirEntry::default();
            let mut dir_search_result = Ext4DirSearchResult::new(ext4_blk, de);
            let r = Self::ext4_dir_find_in_block(&mut ext4_block, path, len, &mut dir_search_result);
            if r {
                return EOK;
            }

            iblock += 1;
        }

        /* No free block found - needed to allocate next data block */
        iblock = 0;
        fblock = 0;

        self.ext4_fs_append_inode_dblk(parent, &mut (iblock as u32), &mut fblock);

        /* Load new block */
        let block_device = self.block_device.clone();
        let mut data = block_device.read_offset(fblock as usize * BLOCK_SIZE);
        let mut ext4_block = Ext4Block {
            logical_block_id: iblock,
            disk_block_id: fblock,
            block_data: &mut data,
            dirty: false,
        };

        let mut new_entry = Ext4DirEntry::default();
        let el = BLOCK_SIZE - size_of::<Ext4DirEntryTail>();
        self.ext4_dir_write_entry(&mut new_entry, el as u16, &child, path, len);

        new_entry.copy_to_byte_slice(&mut ext4_block.block_data, 0);

        // init tail
        let ptr = ext4_block.block_data.as_mut_ptr();
        let mut tail = unsafe {
            *(ptr.add(BLOCK_SIZE - core::mem::size_of::<Ext4DirEntryTail>())
                as *mut Ext4DirEntryTail)
        };
        tail.rec_len = size_of::<Ext4DirEntryTail>() as u16;
        tail.reserved_ft = 0xDE;
        tail.reserved_zero1 = 0;
        tail.reserved_zero2 = 0;

        tail.ext4_dir_set_csum(&self.super_block, &new_entry, &ext4_block.block_data[..]);

        let tail_offset = BLOCK_SIZE - size_of::<Ext4DirEntryTail>();
        tail.copy_to_byte_slice(&mut ext4_block.block_data, tail_offset);

        tail.ext4_dir_set_csum(&self.super_block, &new_entry, &ext4_block.block_data[..]);

        ext4_block.sync_blk_to_disk(block_device.clone());

        // struct ext4_block b;

        EOK
    }

    pub fn ext4_dir_try_insert_entry(
        &self,
        _parent: &Ext4InodeRef,
        dst_blk: &mut Ext4Block,
        child: &mut Ext4InodeRef,
        name: &str,
        name_len: u32,
    ) -> usize {
        let mut required_len = core::mem::size_of::<Ext4DirEntry>() + name_len as usize;

        if required_len % 4 != 0 {
            required_len += 4 - required_len % 4;
        }

        let mut offset = 0;

        while offset < dst_blk.block_data.len() {
            let mut de = Ext4DirEntry::try_from(&dst_blk.block_data[offset..]).unwrap();
            if de.inode == 0 {
                continue;
            }
            let inode = de.inode;
            let rec_len = de.entry_len;

            // 如果是有效的目录项，尝试分割它
            if inode != 0 {
                let used_len = de.name_len as usize;
                let mut sz = core::mem::size_of::<Ext4FakeDirEntry>() + used_len as usize;

                if used_len % 4 != 0 {
                    sz += 4 - used_len % 4;
                }

                let free_space = rec_len as usize - sz;

                // 如果有足够的空闲空间
                if free_space >= required_len {
                    let mut new_entry = Ext4DirEntry::default();

                    de.entry_len = sz as u16;
                    self.ext4_dir_write_entry(
                        &mut new_entry,
                        free_space as u16,
                        &child,
                        name,
                        name_len,
                    );

                    // update parent new_de to blk_data
                    de.copy_to_byte_slice(&mut dst_blk.block_data, offset);
                    new_entry.copy_to_byte_slice(&mut dst_blk.block_data, offset + sz);

                    // set tail csum
                    let mut tail =
                        Ext4DirEntryTail::from(&mut dst_blk.block_data, BLOCK_SIZE).unwrap();
                    let block_device = self.block_device.clone();
                    tail.ext4_dir_set_csum(&self.super_block, &de, &dst_blk.block_data[offset..]);

                    let parent_de = Ext4DirEntry::try_from(&dst_blk.block_data[..]).unwrap();
                    tail.ext4_dir_set_csum(&self.super_block, &parent_de, &dst_blk.block_data[..]);

                    let tail_offset = BLOCK_SIZE - size_of::<Ext4DirEntryTail>();
                    tail.copy_to_byte_slice(&mut dst_blk.block_data, tail_offset);

                    // sync to disk
                    dst_blk.sync_blk_to_disk(block_device.clone());

                    return EOK;
                }
            }
            offset = offset + de.entry_len as usize;
        }

        ENOSPC
    }

    // 写入一个ext4目录项
    pub fn ext4_dir_write_entry(
        &self,
        en: &mut Ext4DirEntry,
        entry_len: u16,
        child: &Ext4InodeRef,
        name: &str,
        name_len: u32,
    ) {
        let file_type = (child.inode.mode & EXT4_INODE_MODE_TYPE_MASK) as usize;

        // 设置目录项的类型
        match file_type {
            EXT4_INODE_MODE_FILE => en.inner.inode_type = DirEntryType::EXT4_DE_REG_FILE.bits(),
            EXT4_INODE_MODE_DIRECTORY => en.inner.inode_type = DirEntryType::EXT4_DE_DIR.bits(),
            EXT4_INODE_MODE_CHARDEV => en.inner.inode_type = DirEntryType::EXT4_DE_CHRDEV.bits(),
            EXT4_INODE_MODE_BLOCKDEV => en.inner.inode_type = DirEntryType::EXT4_DE_BLKDEV.bits(),
            EXT4_INODE_MODE_FIFO => en.inner.inode_type = DirEntryType::EXT4_DE_FIFO.bits(),
            EXT4_INODE_MODE_SOCKET => en.inner.inode_type = DirEntryType::EXT4_DE_SOCK.bits(),
            EXT4_INODE_MODE_SOFTLINK => en.inner.inode_type = DirEntryType::EXT4_DE_SYMLINK.bits(),
            _ => log::info!("{}: unknown type", file_type),
        }

        en.inode = child.inode_id;
        en.entry_len = entry_len;
        en.name_len = name_len as u8;

        let en_name_ptr = en.name.as_mut_ptr();
        unsafe {
            en_name_ptr.copy_from_nonoverlapping(name.as_ptr(), name_len as usize);
        }
        let _name = get_name(en.name, en.name_len as usize).unwrap();
        // log::info!("ext4_dir_write_entry name {:?}", name);
    }

    pub fn ext4_dir_destroy_result(
        _inode_ref: &mut Ext4InodeRef,
        result: &mut Ext4DirSearchResult,
    ) {
        result.block.logical_block_id = 0;
        result.block.disk_block_id = 0;
        result.dentry = Ext4DirEntry::default();
    }

    pub fn ext4_dir_find_entry(
        &self,
        parent: &mut Ext4InodeRef,
        name: &str,
        name_len: u32,
        result: &mut Ext4DirSearchResult,
    ) -> usize {
        // log::info!("ext4_dir_find_entry parent {:x?} {:?}",parent.inode_num,  name);
        let mut iblock = 0;
        let mut fblock: Ext4FsBlockId = 0;

        let inode_size: u32 = parent.inode.size;
        let total_blocks: u32 = inode_size / BLOCK_SIZE as u32;

        while iblock < total_blocks {
            self.ext4_fs_get_inode_dblk_idx(parent, &mut iblock, &mut fblock, false);

            // load_block
            let mut data = self.block_device.read_offset(fblock as usize * BLOCK_SIZE);
            let mut ext4_block = Ext4Block {
                logical_block_id: iblock,
                disk_block_id: fblock,
                block_data: &mut data,
                dirty: false,
            };

            let r = Self::ext4_dir_find_in_block(&mut ext4_block, name, name_len, result);
            if r {
                return EOK;
            }

            iblock += 1
        }

        ENOENT
    }

    pub fn ext4_dir_find_in_block(
        block: &Ext4Block,
        name: &str,
        name_len: u32,
        result: &mut Ext4DirSearchResult,
    ) -> bool {
        let mut offset = 0;
    
        while offset < block.block_data.len() {
            let de = Ext4DirEntry::try_from(&block.block_data[offset..]).unwrap();
    
            offset = offset + de.entry_len as usize;
            if de.inode == 0 {
                continue;
            }
    
            let s = get_name(de.name, de.name_len as usize);
    
            if let Ok(s) = s {
                if name_len == de.name_len as u32 {
                    if name.to_string() == s {
                        result.dentry = de;
                        return true;
                    }
                }
            }
        }
    
        false
    }
}
