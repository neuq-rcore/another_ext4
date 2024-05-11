use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;
use crate::return_errno_with_message;
use core::cmp::min;

impl Ext4 {
    fn split_path(path: &str) -> Vec<String> {
        let _ = path.trim_start_matches("/");
        path.split("/").map(|s| s.to_string()).collect()
    }

    pub(super) fn generic_open(
        &mut self,
        path: &str,
        flag: OpenFlags,
        ftype: FileType,
        parent_inode: &Ext4InodeRef,
    ) -> Result<Ext4File> {
        // Search from the given parent inode
        let mut parent = parent_inode.clone();
        let search_path = Self::split_path(path);
        
        info!("generic open: {}", path);
        for (i, path) in search_path.iter().enumerate() {
            let res = self.dir_find_entry(&parent, path);
            debug!("dir_find_entry: {:?}", res);
            match res {
                Ok(entry) => {
                    parent = self.get_inode_ref(entry.inode());
                }
                Err(e) => {
                    if e.code() != ErrCode::ENOENT {
                        // dir search failed with error other than ENOENT
                        return_errno_with_message!(ErrCode::ENOTSUP, "dir search failed");
                    }
                    if !flag.contains(OpenFlags::O_CREAT) {
                        return_errno_with_message!(ErrCode::ENOENT, "file not found");
                    }
                    // Create file/directory
                    let mut child = if i == search_path.len() - 1 {
                        self.alloc_inode(ftype)
                    } else {
                        self.alloc_inode(FileType::Directory)
                    };
                    // Link the new inode
                    let r = self.ext4_link(&mut parent, &mut child, path);
                    if r != EOK {
                        // Fail. Free new inode
                        return_errno_with_message!(ErrCode::ELINKFIAL, "link fail");
                    }
                    // Write back parent and child
                    self.write_back_inode_with_csum(&mut parent);
                    self.write_back_inode_with_csum(&mut child);
                }
            }
        }
        // Reach the target
        let mut file = Ext4File::default();
        file.inode = parent.inode_id;
        Ok(file)
    }

    #[allow(unused)]
    pub fn ext4_open(&mut self, path: &str, flags: &str, file_expect: bool) -> Result<Ext4File> {
        // open flags
        let iflags = OpenFlags::from_str(flags).unwrap();
        // file type
        let file_type = if file_expect {
            FileType::RegularFile
        } else {
            FileType::Directory
        };
        // TODO:journal
        if iflags.contains(OpenFlags::O_CREAT) {
            self.ext4_trans_start();
        }
        // open file
        let res = self.generic_open(path, iflags, file_type, &self.get_root_inode_ref());
        res.map(|mut file| {
            // set mount point
            let mut ptr = Box::new(self.mount_point.clone());
            file.mp = Box::as_mut(&mut ptr) as *mut Ext4MountPoint;
            file
        })
    }

    #[allow(unused)]
    pub fn ext4_file_read(
        &self,
        file: &mut Ext4File,
        read_buf: &mut [u8],
        read_size: usize,
    ) -> Result<usize> {
        // Read no bytes
        if read_size == 0 {
            return Ok(0);
        }
        // Get the inode of the file
        let mut inode_ref = self.get_inode_ref(file.inode);
        // sync file size
        file.fsize = inode_ref.inode.size();

        // Check if the file is a softlink
        if inode_ref.inode.is_softlink(&self.super_block) {
            // TODO: read softlink
            log::debug!("ext4_read unsupported softlink");
        }

        // Calc the actual size to read
        let size_to_read = min(read_size, file.fsize as usize - file.fpos);
        // Calc the start block of reading
        let start_iblock = (file.fpos / BLOCK_SIZE) as LBlockId;
        // Calc the length that is not aligned to the block size
        let mut misaligned = file.fpos % BLOCK_SIZE;

        let mut cursor = 0;
        let mut iblock = start_iblock;
        // Read first block
        if misaligned > 0 {
            let first_block_read_len = min(BLOCK_SIZE - misaligned, size_to_read);
            let fblock = self.extent_get_pblock(&mut inode_ref, start_iblock);
            if fblock != 0 {
                let block_offset = fblock as usize * BLOCK_SIZE + misaligned;
                let block_data = self.block_device.read_offset(block_offset);
                // Copy data from block to the user buffer
                read_buf[cursor..cursor + first_block_read_len]
                    .copy_from_slice(&block_data[0..first_block_read_len]);
            } else {
                // Handle the unwritten block by zeroing out the respective part of the buffer
                read_buf[cursor..cursor + first_block_read_len].fill(0);
            }
            cursor += first_block_read_len;
            file.fpos += first_block_read_len;
            iblock += 1;
        }
        // Continue with full block reads
        while cursor < size_to_read {
            let read_length = min(BLOCK_SIZE, size_to_read - cursor);
            let fblock = self.extent_get_pblock(&mut inode_ref, iblock);
            if fblock != 0 {
                let block_data = self.block_device.read_offset(fblock as usize * BLOCK_SIZE);
                // Copy data from block to the user buffer
                read_buf[cursor..cursor + read_length].copy_from_slice(&block_data[0..read_length]);
            } else {
                // Handle the unwritten block by zeroing out the respective part of the buffer
                read_buf[cursor..cursor + read_length].fill(0);
            }
            cursor += read_length;
            file.fpos += read_length;
            iblock += 1;
        }

        Ok(cursor)
    }

    pub fn ext4_file_write(&mut self, file: &mut Ext4File, data: &[u8]) {
        let size = data.len();
        let mut inode_ref = self.get_inode_ref(file.inode);
        // Sync ext file
        file.fsize = inode_ref.inode.size();

        // Calc the start and end block of reading
        let start_iblock = (file.fpos / BLOCK_SIZE) as LBlockId;
        let end_iblock = ((file.fpos + size) / BLOCK_SIZE) as LBlockId;
        // Calc the block count of the file
        let block_count = (file.fsize as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        // Append enough block for writing
        let append_block_count = end_iblock + 1 - block_count as LBlockId;
        for _ in 0..append_block_count {
            self.inode_append_block(&mut inode_ref);
        }

        // Write data
        let mut cursor = 0;
        let mut iblock = start_iblock;
        while cursor < size {
            let write_len = min(BLOCK_SIZE, size - cursor);
            let fblock = self.extent_get_pblock(&mut inode_ref, iblock);
            if fblock != 0 {
                self.block_device.write_offset(cursor, &data[cursor..cursor + write_len]);
            } else {
                panic!("Write to unallocated block");
            }
            cursor += write_len;
            file.fpos += write_len;
            iblock += 1;
        }
    }

    pub fn ext4_file_remove(&self, _path: &str) -> Result<usize> {
        return_errno_with_message!(ErrCode::ENOTSUP, "not support");
    }
}
