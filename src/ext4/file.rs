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
        ftype: u8,
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

            let r = self.ext4_dir_find_entry(
                &mut search_parent,
                &search_path[..len as usize],
                len as u32,
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
                    self.ext4_fs_alloc_inode(&mut child_inode_ref, DirEntryType::EXT4_DE_DIR.bits())
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
                    len as u32,
                );

                if r != EOK {
                    /*Fail. Free new inode.*/
                    return_errno_with_message!(Errnum::ELINKFIAL, "link fail");
                }

                self.write_back_inode(&mut search_parent);
                self.write_back_inode(&mut child_inode_ref);
                self.write_back_inode(parent_inode);

                continue;
            }

            let _name = get_name(
                dir_search_result.dentry.name,
                dir_search_result.dentry.name_len as usize,
            )
            .unwrap();
            // log::info!("find de name{:?} de inode {:x?}", name, dir_search_result.dentry.inode);

            if is_goal {
                file.inode = dir_search_result.dentry.inode;
                return Ok(EOK);
            } else {
                search_parent = self.get_inode_ref(dir_search_result.dentry.inode);
                search_path = &search_path[len..];
            }
        }
    }

    pub fn ext4_open(
        &self,
        file: &mut Ext4File,
        path: &str,
        // flags: &str,
        iflags: u32,
        file_expect: bool,
    ) -> Result<usize> {
        // get mount point
        let mut ptr = Box::new(self.mount_point.clone());
        file.mp = Box::as_mut(&mut ptr) as *mut Ext4MountPoint;

        // get open flags
        // let iflags = self.ext4_parse_flags(flags).unwrap();

        // file for dir
        let filetype = if file_expect {
            DirEntryType::EXT4_DE_REG_FILE
        } else {
            DirEntryType::EXT4_DE_DIR
        };

        if iflags & O_CREAT != 0 {
            self.ext4_trans_start();
        }

        let mut root_inode_ref = self.get_root_inode_ref();

        let r = self.ext4_generic_open(file, path, iflags, filetype.bits(), &mut root_inode_ref);

        r
    }

    pub fn ext4_file_read(&self, ext4_file: &mut Ext4File) -> Vec<u8> {
        // 创建一个空的向量，用于存储文件的内容
        let mut file_data: Vec<u8> = Vec::new();

        // 创建一个空的向量，用于存储文件的所有extent信息
        let mut extents: Vec<Ext4Extent> = Vec::new();

        let inode_ref = self.get_inode_ref(ext4_file.inode);

        self.ext4_find_all_extent(&inode_ref, &mut extents);

        // 遍历extents向量，对每个extent，计算它的物理块号，然后调用read_block函数来读取数据块，并将结果追加到file_data向量中
        for extent in extents {
            // 获取extent的起始块号、块数和逻辑块号
            let start_block = extent.start_lo as u64 | ((extent.start_hi as u64) << 32);
            let block_count = extent.block_count as u64;
            let logical_block = extent.first_block as u64;
            // 计算extent的物理块号
            let physical_block = start_block + logical_block;
            // 从file中读取extent的所有数据块，并将结果追加到file_data向量中
            for i in 0..block_count {
                let block_num = physical_block + i;
                let block_data = self
                    .block_device
                    .read_offset(block_num as usize * BLOCK_SIZE);
                file_data.extend(block_data);
            }
        }
        file_data
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
        self.write_back_inode(&mut inode_ref);
        // let mut inode_ref = Ext4InodeRef::get_inode_ref(self.self_ref.clone(), ext4_file.inode);
        let mut root_inode_ref = self.get_root_inode_ref();
        self.write_back_inode(&mut root_inode_ref);
    }

    pub fn ext4_file_remove(&self, _path: &str) -> Result<usize> {
        return_errno_with_message!(Errnum::ENOTSUP, "not support");
    }
}
