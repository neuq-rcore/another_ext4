use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;
use crate::return_errno_with_message;
use core::cmp::min;

impl Ext4 {
    /// Open a regular file, return a file descriptor
    pub fn open_file(&mut self, path: &str, flags: &str) -> Result<File> {
        // open flags
        let open_flags = OpenFlags::from_str(flags).unwrap();
        // TODO:journal
        if open_flags.contains(OpenFlags::O_CREAT) {
            self.trans_start();
        }
        // open file
        let res = self.generic_open(EXT4_ROOT_INO, path, open_flags, Some(FileType::RegularFile));
        res.map(|inode| {
            File::new(
                self.mount_point.clone(),
                inode.id,
                open_flags.bits(),
                inode.inode.size(),
            )
        })
    }

    /// Read `read_buf.len()` bytes from the file
    pub fn read_file(&self, file: &mut File, read_buf: &mut [u8]) -> Result<usize> {
        let read_size = read_buf.len();
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
            let block = self.read_block(fblock);
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
            let block = self.read_block(fblock);
            // Copy data from block to the user buffer
            read_buf[cursor..cursor + read_len].copy_from_slice(block.read_offset(0, read_len));
            cursor += read_len;
            file.fpos += read_len;
            iblock += 1;
        }

        Ok(cursor)
    }

    /// Write `data` to file
    pub fn write_file(&mut self, file: &mut File, data: &[u8]) -> Result<()> {
        let write_size = data.len();
        let mut inode_ref = self.read_inode(file.inode);
        // Sync ext file
        file.fsize = inode_ref.inode.size();

        // Calc the start and end block of reading
        let start_iblock = (file.fpos / BLOCK_SIZE) as LBlockId;
        let end_iblock = ((file.fpos + write_size) / BLOCK_SIZE) as LBlockId;
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
        while cursor < write_size {
            let write_len = min(BLOCK_SIZE, write_size - cursor);
            let fblock = self.extent_get_pblock(&mut inode_ref, iblock)?;
            let mut block = self.read_block(fblock);
            block.write_offset(file.fpos % BLOCK_SIZE, &data[cursor..cursor + write_len]);
            self.write_block(&block);

            cursor += write_len;
            file.fpos += write_len;
            iblock += 1;
        }
        Ok(())
    }

    /// Remove a regular file
    pub fn remove_file(&mut self, path: &str) -> Result<()> {
        self.generic_remove(EXT4_ROOT_INO, path, Some(FileType::RegularFile))
    }

    /// Open an object of any type in the filesystem. Return the inode
    /// of the object if found.
    ///
    /// ## Params
    /// * `root` - The inode id of the starting directory for the search.
    /// * `path` - The path of the object to be opened.
    /// * `flags` - The open flags. If the flags contains `O_CREAT`, and `expect_type`
    ///    is provided, the function will create a new inode of the specified type.
    /// * `expect_type` - The expect type of object to open, optional. If this
    ///    parameter is provided, the function will check the type of the object
    ///    to open.
    pub(super) fn generic_open(
        &mut self,
        root: InodeId,
        path: &str,
        flags: OpenFlags,
        expect_type: Option<FileType>,
    ) -> Result<InodeRef> {
        // Search from the given parent inode
        info!("generic_open: root {}, path {}", root, path);
        let mut cur = self.read_inode(root);
        let search_path = Self::split_path(path);

        for (i, path) in search_path.iter().enumerate() {
            let res = self.dir_find_entry(&cur, path);
            match res {
                Ok(entry) => {
                    cur = self.read_inode(entry.inode());
                }
                Err(e) => {
                    if e.code() != ErrCode::ENOENT {
                        // dir search failed with error other than ENOENT
                        return_errno_with_message!(ErrCode::ENOTSUP, "dir search failed");
                    }
                    if !flags.contains(OpenFlags::O_CREAT) || expect_type.is_none() {
                        // `O_CREAT` and `expect_type` must be provided together to
                        // create a new object
                        return_errno_with_message!(ErrCode::ENOENT, "file not found");
                    }
                    // Create file/directory
                    let mut child = if i == search_path.len() - 1 {
                        self.create_inode(expect_type.unwrap())
                    } else {
                        self.create_inode(FileType::Directory)
                    }?;
                    // Link the new inode
                    self.link(&mut cur, &mut child, path)
                        .map_err(|_| Ext4Error::with_message(ErrCode::ELINKFAIL, "link fail"))?;
                    // Write back parent and child
                    self.write_inode_with_csum(&mut cur);
                    self.write_inode_with_csum(&mut child);
                    // Update parent
                    cur = child;
                }
            }
        }
        // `cur` is the target inode, check type if `expect_type` os provided
        if let Some(expect_type) = expect_type {
            if inode_mode2file_type(cur.inode.mode()) != expect_type {
                return_errno_with_message!(ErrCode::EISDIR, "inode type mismatch");
            }
        }
        Ok(cur)
    }

    /// Remove an object of any type from the filesystem. Return the inode
    /// of the object if found.
    ///
    /// ## Params
    /// * `root` - The inode id of the starting directory for the search.
    /// * `path` - The path of the object to be removed.
    /// * `expect_type` - The expect type of object to open, optional. If this
    ///    parameter is provided, the function will check the type of the object
    ///    to open.
    pub(super) fn generic_remove(
        &mut self,
        root: InodeId,
        path: &str,
        expect_type: Option<FileType>,
    ) -> Result<()> {
        // Get the parent directory path and the file name
        let mut search_path = Self::split_path(path);
        let file_name = &search_path.split_off(search_path.len() - 1)[0];
        let parent_path = search_path.join("/");
        // Get the parent directory inode
        let mut parent_inode = self.generic_open(
            root,
            &parent_path,
            OpenFlags::O_RDONLY,
            Some(FileType::Directory),
        )?;
        // Get the file inode, check the existence and type
        let mut child_inode =
            self.generic_open(parent_inode.id, file_name, OpenFlags::O_RDONLY, expect_type)?;

        // Remove the file from the parent directory
        self.dir_remove_entry(&mut parent_inode, &file_name)?;
        // Update the link count of inode
        let link_cnt = child_inode.inode.links_cnt() - 1;
        if link_cnt == 0 {
            // Free the inode of the file if link count is 0
            return self.free_inode(&mut child_inode);
        }
        child_inode.inode.set_links_cnt(link_cnt);
        Ok(())
    }

    fn split_path(path: &str) -> Vec<String> {
        let _ = path.trim_start_matches("/");
        if path.is_empty() {
            return vec![]; // root
        }
        path.split("/").map(|s| s.to_string()).collect()
    }
}
