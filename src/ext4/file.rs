use super::utils::*;
use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;
use crate::return_errno_with_message;

impl Ext4 {
    pub fn ext4_generic_open(
        &self,
        file: &mut Ext4File,
        path: &str,
        iflags: u32,
        ftype: FileType,
        parent_inode: &mut Ext4InodeRef,
    ) -> Result<usize> {
        let mut is_goal = false;

        let mut data: Vec<u8> = Vec::with_capacity(BLOCK_SIZE);
        let ext4_blk = Ext4Block {
            logical_block_id: 0,
            disk_block_id: 0,
            block_data: &mut data,
            dirty: true,
        };
        let de = Ext4DirEntry::default();
        let mut dir_search_result = Ext4DirSearchResult::new(ext4_blk, de);

        file.flags = iflags;

        // load root inode
        let root_inode_ref = self.get_root_inode_ref();

        // if !parent_inode.is_none() {
        //     parent_inode.unwrap().inode_num = root_inode_ref.inode_num;
        // }

        // search dir
        let mut search_parent = root_inode_ref;
        let mut search_path = ext4_path_skip(&path, ".");
        let mut len;
        loop {
            search_path = ext4_path_skip(search_path, "/");
            len = ext4_path_check(search_path, &mut is_goal);

            let r = self.dir_find_entry(
                &mut search_parent,
                &search_path[..len as usize],
                &mut dir_search_result,
            );

            // log::info!("dir_search_result.dentry {:?} r {:?}", dir_search_result.dentry, r);
            if r != EOK {
                // ext4_dir_destroy_result(&mut root_inode_ref, &mut dir_search_result);

                if r != ENOENT {
                    // dir search failed with error other than ENOENT
                    return_errno_with_message!(Errnum::ENOTSUP, "dir search failed");
                }

                if !((iflags & O_CREAT) != 0) {
                    return_errno_with_message!(Errnum::ENOENT, "file not found");
                }

                let mut child_inode_ref = Ext4InodeRef::default();

                let r = if is_goal {
                    self.ext4_fs_alloc_inode(&mut child_inode_ref, ftype)
                } else {
                    self.ext4_fs_alloc_inode(&mut child_inode_ref, FileType::Directory)
                };

                if r != EOK {
                    return_errno_with_message!(Errnum::EALLOCFIAL, "alloc inode fail");
                    // break;
                }

                Self::ext4_fs_inode_blocks_init(&mut child_inode_ref);

                let r = self.ext4_link(
                    &mut search_parent,
                    &mut child_inode_ref,
                    &search_path[..len as usize],
                );

                if r != EOK {
                    /*Fail. Free new inode.*/
                    return_errno_with_message!(Errnum::ELINKFIAL, "link fail");
                }

                self.write_back_inode_with_csum(&mut search_parent);
                self.write_back_inode_with_csum(&mut child_inode_ref);
                self.write_back_inode_with_csum(parent_inode);

                continue;
            }
            // log::info!("find de name{:?} de inode {:x?}", name, dir_search_result.dentry.inode);

            if is_goal {
                file.inode = dir_search_result.dentry.inode();
                return Ok(EOK);
            } else {
                search_parent = self.get_inode_ref(dir_search_result.dentry.inode());
                search_path = &search_path[len..];
            }
        }
    }

    pub fn ext4_open(
        &self,
        file: &mut Ext4File,
        path: &str,
        flags: &str,
        file_expect: bool,
    ) -> Result<usize> {
        // get open flags
        let iflags = ext4_parse_flags(flags).unwrap();

        // get mount point
        let mut ptr = Box::new(self.mount_point.clone());
        file.mp = Box::as_mut(&mut ptr) as *mut Ext4MountPoint;

        // file for dir
        let filetype = if file_expect {
            FileType::RegularFile
        } else {
            FileType::Directory
        };

        if iflags & O_CREAT != 0 {
            self.ext4_trans_start();
        }

        let mut root_inode_ref = self.get_root_inode_ref();

        let r = self.ext4_generic_open(file, path, iflags, filetype, &mut root_inode_ref);

        r
    }

    #[allow(unused)]
    pub fn ext4_file_read(
        &self,
        ext4_file: &mut Ext4File,
        read_buf: &mut [u8],
        size: usize,
        read_cnt: &mut usize,
    ) -> Result<usize> {
        if size == 0 {
            return Ok(EOK);
        }

        let mut inode_ref = self.get_inode_ref(ext4_file.inode);

        // sync file size
        ext4_file.fsize = inode_ref.inode.size();

        let is_softlink =
            inode_ref.inode.inode_type(&self.super_block) == EXT4_INODE_MODE_SOFTLINK as u32;

        if is_softlink {
            log::debug!("ext4_read unsupported softlink");
        }

        let block_size = BLOCK_SIZE;

        // 计算读取大小
        let size_to_read = if size > (ext4_file.fsize as usize - ext4_file.fpos) {
            ext4_file.fsize as usize - ext4_file.fpos
        } else {
            size
        };

        let mut iblock_idx = (ext4_file.fpos / block_size) as u32;
        let iblock_last = ((ext4_file.fpos + size_to_read) / block_size) as u32;
        let mut unalg = (ext4_file.fpos % block_size) as u32;

        let mut offset = 0;
        let mut total_bytes_read = 0;

        if unalg > 0 {
            let first_block_read_len = core::cmp::min(block_size - unalg as usize, size_to_read);
            let mut fblock = 0;

            self.ext4_fs_get_inode_dblk_idx(&mut inode_ref, &mut iblock_idx, &mut fblock, false);

            // if r != EOK {
            //     return Err(Ext4Error::new(r));
            // }

            if fblock != 0 {
                let block_offset = fblock * block_size as u64 + unalg as u64;
                let block_data = self.block_device.read_offset(block_offset as usize);

                // Copy data from block to the user buffer
                read_buf[offset..offset + first_block_read_len]
                    .copy_from_slice(&block_data[0..first_block_read_len]);
            } else {
                // Handle the unwritten block by zeroing out the respective part of the buffer
                for x in &mut read_buf[offset..offset + first_block_read_len] {
                    *x = 0;
                }
            }

            offset += first_block_read_len;
            total_bytes_read += first_block_read_len;
            ext4_file.fpos += first_block_read_len;
            *read_cnt += first_block_read_len;
            iblock_idx += 1;
        }

        // Continue with full block reads
        while total_bytes_read < size_to_read {
            let read_length = core::cmp::min(block_size, size_to_read - total_bytes_read);
            let mut fblock = 0;

            self.ext4_fs_get_inode_dblk_idx(&mut inode_ref, &mut iblock_idx, &mut fblock, false);

            // if r != EOK {
            //     return Err(Ext4Error::new(r));
            // }

            if fblock != 0 {
                let block_data = self
                    .block_device
                    .read_offset((fblock * block_size as u64) as usize);
                read_buf[offset..offset + read_length].copy_from_slice(&block_data[0..read_length]);
            } else {
                // Handle the unwritten block by zeroing out the respective part of the buffer
                for x in &mut read_buf[offset..offset + read_length] {
                    *x = 0;
                }
            }

            offset += read_length;
            total_bytes_read += read_length;
            ext4_file.fpos += read_length;
            *read_cnt += read_length;
            iblock_idx += 1;
        }

        return Ok(EOK);
    }

    pub fn ext4_file_write(&self, ext4_file: &mut Ext4File, data: &[u8], size: usize) {
        let super_block_data = self.block_device.read_offset(BASE_OFFSET);
        let super_block = Ext4Superblock::try_from(super_block_data).unwrap();
        let mut inode_ref = self.get_inode_ref(ext4_file.inode);
        let block_size = super_block.block_size() as usize;
        let iblock_last = ext4_file.fpos as usize + size / block_size;
        let mut iblk_idx = ext4_file.fpos as usize / block_size;
        let ifile_blocks = ext4_file.fsize as usize + block_size - 1 / block_size;

        let mut fblk = 0;
        let mut fblock_start = 0;
        let mut fblock_count = 0;

        let mut size = size;
        while size >= block_size {
            while iblk_idx < iblock_last {
                if iblk_idx < ifile_blocks {
                    self.ext4_fs_append_inode_dblk(
                        &mut inode_ref,
                        &mut (iblk_idx as u32),
                        &mut fblk,
                    );
                }

                iblk_idx += 1;

                if fblock_start == 0 {
                    fblock_start = fblk;
                }
                fblock_count += 1;
            }
            size -= block_size;
        }

        for i in 0..fblock_count {
            let idx = i * BLOCK_SIZE as usize;
            let offset = (fblock_start as usize + i as usize) * BLOCK_SIZE;
            self.block_device
                .write_offset(offset, &data[idx..(idx + BLOCK_SIZE as usize)]);
        }
        // inode_ref.inner.inode.size = fblock_count as u32 * BLOCK_SIZE as u32;
        self.write_back_inode_with_csum(&mut inode_ref);
        // let mut inode_ref = Ext4InodeRef::get_inode_ref(self.self_ref.clone(), ext4_file.inode);
        let mut root_inode_ref = self.get_root_inode_ref();
        self.write_back_inode_with_csum(&mut root_inode_ref);
    }

    pub fn ext4_file_remove(&self, _path: &str) -> Result<usize> {
        return_errno_with_message!(Errnum::ENOTSUP, "not support");
    }
}
