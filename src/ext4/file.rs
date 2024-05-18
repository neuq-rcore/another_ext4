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
        root: &InodeRef,
    ) -> Result<File> {
        info!("generic open: {}", path);
        // Search from the given parent inode
        let mut parent = root.clone();
        let search_path = Self::split_path(path);

        for (i, path) in search_path.iter().enumerate() {
            let res = self.dir_find_entry(&parent, path);
            match res {
                Ok(entry) => {
                    parent = self.read_inode(entry.inode());
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
                    }?;
                    // Link the new inode
                    self.link(&mut parent, &mut child, path)
                        .map_err(|_| Ext4Error::with_message(ErrCode::ELINKFIAL, "link fail"))?;
                    // Write back parent and child
                    self.write_inode_with_csum(&mut parent);
                    self.write_inode_with_csum(&mut child);
                    // Update parent
                    parent = child;
                }
            }
        }
        // `parent` is the target inode
        let mut file = File::default();
        file.inode = parent.id;
        Ok(file)
    }

    pub fn open(&mut self, path: &str, flags: &str, file_expect: bool) -> Result<File> {
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
            self.trans_start();
        }
        // open file
        let res = self.generic_open(path, iflags, file_type, &self.read_root_inode());
        res.map(|mut file| {
            // set mount point
            let mut ptr = Box::new(self.mount_point.clone());
            file.mp = Box::as_mut(&mut ptr) as *mut MountPoint;
            file
        })
    }

    pub fn read(&self, file: &mut File, read_buf: &mut [u8], read_size: usize) -> Result<usize> {
        // Read no bytes
        if read_size == 0 {
            return Ok(0);
        }
        // Get the inode of the file
        let mut inode_ref = self.read_inode(file.inode);
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
        let misaligned = file.fpos % BLOCK_SIZE;

        let mut cursor = 0;
        let mut iblock = start_iblock;
        // Read first block
        if misaligned > 0 {
            let read_len = min(BLOCK_SIZE - misaligned, size_to_read);
            let fblock = self.extent_get_pblock(&mut inode_ref, start_iblock)?;
            let block = self.block_device.read_block(fblock);
            // Copy data from block to the user buffer
            read_buf[cursor..cursor + read_len]
                .copy_from_slice(block.read_offset(misaligned, read_len));
            cursor += read_len;
            file.fpos += read_len;
            iblock += 1;
        }
        // Continue with full block reads
        while cursor < size_to_read {
            let read_len = min(BLOCK_SIZE, size_to_read - cursor);
            let fblock = self.extent_get_pblock(&mut inode_ref, iblock)?;
            let block = self.block_device.read_block(fblock);
            // Copy data from block to the user buffer
            read_buf[cursor..cursor + read_len].copy_from_slice(block.read_offset(0, read_len));
            cursor += read_len;
            file.fpos += read_len;
            iblock += 1;
        }

        Ok(cursor)
    }

    pub fn write(&mut self, file: &mut File, data: &[u8]) -> Result<()> {
        let size = data.len();
        let mut inode_ref = self.read_inode(file.inode);
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
            self.inode_append_block(&mut inode_ref)?;
        }

        // Write data
        let mut cursor = 0;
        let mut iblock = start_iblock;
        while cursor < size {
            let write_len = min(BLOCK_SIZE, size - cursor);
            let fblock = self.extent_get_pblock(&mut inode_ref, iblock)?;
            let mut block = self.block_device.read_block(fblock);
            block.write_offset(file.fpos % BLOCK_SIZE, &data[cursor..cursor + write_len]);
            block.sync_to_disk(self.block_device.clone());

            cursor += write_len;
            file.fpos += write_len;
            iblock += 1;
        }
        Ok(())
    }

    pub fn ext4_file_remove(&self, _path: &str) -> Result<usize> {
        return_errno_with_message!(ErrCode::ENOTSUP, "not support");
    }
}
