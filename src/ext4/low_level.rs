//! Low-level operations of Ext4 filesystem.
//!
//! These interfaces are designed and arranged coresponding to FUSE low-level ops.
//! Ref: https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html

use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;
use crate::prelude::*;
use crate::return_error;
use core::cmp::min;

impl Ext4 {
    /// Get file attributes.
    ///
    /// # Params
    ///
    /// * `id` - inode id
    ///
    /// # Return
    ///
    /// A file attribute struct.
    ///
    /// # Error
    ///
    /// `EINVAL` if the inode is invalid (mode == 0).
    pub fn getattr(&self, id: InodeId) -> Result<FileAttr> {
        let inode = self.read_inode(id);
        if inode.inode.mode().bits() == 0 {
            return_error!(ErrCode::EINVAL, "Invalid inode");
        }
        Ok(FileAttr {
            ino: id,
            size: inode.inode.size(),
            blocks: inode.inode.block_count(),
            atime: inode.inode.atime(),
            mtime: inode.inode.mtime(),
            ctime: inode.inode.ctime(),
            crtime: inode.inode.crtime(),
            ftype: inode.inode.file_type(),
            perm: inode.inode.perm(),
            links: inode.inode.link_count(),
            uid: inode.inode.uid(),
            gid: inode.inode.gid(),
        })
    }

    /// Set file attributes.
    ///
    /// # Params
    ///
    /// * `id` - inode id
    /// * `mode` - file mode
    /// * `uid` - 32-bit user id
    /// * `gid` - 32-bit group id
    /// * `size` - 64-bit file size
    /// * `atime` - 32-bit access time in seconds
    /// * `mtime` - 32-bit modify time in seconds
    /// * `ctime` - 32-bit change time in seconds
    /// * `crtime` - 32-bit create time in seconds
    ///
    /// # Error
    ///
    /// `EINVAL` if the inode is invalid (mode == 0).
    pub fn setattr(
        &mut self,
        id: InodeId,
        mode: Option<InodeMode>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<u32>,
        mtime: Option<u32>,
        ctime: Option<u32>,
        crtime: Option<u32>,
    ) -> Result<()> {
        let mut inode = self.read_inode(id);
        if inode.inode.mode().bits() == 0 {
            return_error!(ErrCode::EINVAL, "Invalid inode");
        }
        if let Some(mode) = mode {
            inode.inode.set_mode(mode);
        }
        if let Some(uid) = uid {
            inode.inode.set_uid(uid);
        }
        if let Some(gid) = gid {
            inode.inode.set_gid(gid);
        }
        if let Some(size) = size {
            inode.inode.set_size(size);
        }
        if let Some(atime) = atime {
            inode.inode.set_atime(atime);
        }
        if let Some(mtime) = mtime {
            inode.inode.set_mtime(mtime);
        }
        if let Some(ctime) = ctime {
            inode.inode.set_ctime(ctime);
        }
        if let Some(crtime) = crtime {
            inode.inode.set_crtime(crtime);
        }
        self.write_inode_with_csum(&mut inode);
        Ok(())
    }

    /// Create and open a file. This function will not check the existence
    /// of the file. Call `lookup` to check beforehand.
    ///
    /// # Params
    ///
    /// * `parent` - parent directory inode id
    /// * `name` - file name
    /// * `mode` - file type and mode with which to create the new file
    /// * `flags` - open flags
    ///
    /// # Return
    ///
    /// `Ok(inode)` - Inode id of the new file
    ///
    /// # Error
    ///
    /// * `ENOTDIR` - `parent` is not a directory
    /// * `ENOSPC` - No space left on device
    pub fn create(&mut self, parent: InodeId, name: &str, mode: InodeMode) -> Result<InodeId> {
        let mut parent = self.read_inode(parent);
        // Can only create a file in a directory
        if !parent.inode.is_dir() {
            return_error!(ErrCode::ENOTDIR, "Inode {} is not a directory", parent.id);
        }
        // Create child inode and link it to parent directory
        let mut child = self.create_inode(mode)?;
        self.link_inode(&mut parent, &mut child, name)?;
        // Create file handler
        Ok(child.id)
    }

    /// Read data from a file. This function will read exactly `buf.len()`
    /// bytes unless the end of the file is reached.
    ///
    /// # Params
    ///
    /// * `file` - the file handler, acquired by `open` or `create`
    /// * `offset` - offset to read from
    /// * `buf` - the buffer to store the data
    ///
    /// # Return
    ///
    /// `Ok(usize)` - the actual number of bytes read
    ///
    /// # Error
    ///
    /// * `EISDIR` - `file` is not a regular file
    pub fn read(&mut self, file: InodeId, offset: usize, buf: &mut [u8]) -> Result<usize> {
        // Get the inode of the file
        let mut file = self.read_inode(file);
        if !file.inode.is_file() {
            return_error!(ErrCode::EISDIR, "Inode {} is not a file", file.id);
        }

        // Read no bytes
        if buf.len() == 0 {
            return Ok(0);
        }
        // Calc the actual size to read
        let read_size = min(buf.len(), file.inode.size() as usize - offset);
        // Calc the start block of reading
        let start_iblock = (offset / BLOCK_SIZE) as LBlockId;
        // Calc the length that is not aligned to the block size
        let misaligned = offset % BLOCK_SIZE;

        let mut cursor = 0;
        let mut iblock = start_iblock;
        // Read first block
        if misaligned > 0 {
            let read_len = min(BLOCK_SIZE - misaligned, read_size);
            let fblock = self.extent_query(&mut file, start_iblock).unwrap();
            let block = self.read_block(fblock);
            // Copy data from block to the user buffer
            buf[cursor..cursor + read_len].copy_from_slice(block.read_offset(misaligned, read_len));
            cursor += read_len;
            iblock += 1;
        }
        // Continue with full block reads
        while cursor < read_size {
            let read_len = min(BLOCK_SIZE, read_size - cursor);
            let fblock = self.extent_query(&mut file, iblock).unwrap();
            let block = self.read_block(fblock);
            // Copy data from block to the user buffer
            buf[cursor..cursor + read_len].copy_from_slice(block.read_offset(0, read_len));
            cursor += read_len;
            iblock += 1;
        }

        Ok(cursor)
    }

    /// Write data to a file. This function will write exactly `data.len()` bytes.
    ///
    /// # Params
    ///
    /// * `file` - the file handler, acquired by `open` or `create`
    /// * `offset` - offset to write to
    /// * `data` - the data to write
    ///
    /// # Return
    ///
    /// `Ok(usize)` - the actual number of bytes written
    ///
    /// # Error
    ///
    /// * `EISDIR` - `file` is not a regular file
    /// * `ENOSPC` - no space left on device
    pub fn write(&mut self, file: InodeId, offset: usize, data: &[u8]) -> Result<usize> {
        // Get the inode of the file
        let mut file = self.read_inode(file);
        if !file.inode.is_file() {
            return_error!(ErrCode::EISDIR, "Inode {} is not a file", file.id);
        }

        let write_size = data.len();
        // Calc the start and end block of writing
        let start_iblock = (offset / BLOCK_SIZE) as LBlockId;
        let end_iblock = ((offset + write_size) / BLOCK_SIZE) as LBlockId;
        // Append enough block for writing
        let append_block_count = end_iblock as i64 + 1 - file.inode.block_count() as i64;
        for _ in 0..append_block_count {
            self.inode_append_block(&mut file)?;
        }

        // Write data
        let mut cursor = 0;
        let mut iblock = start_iblock;
        while cursor < write_size {
            let write_len = min(BLOCK_SIZE, write_size - cursor);
            let fblock = self.extent_query(&mut file, iblock)?;
            let mut block = self.read_block(fblock);
            block.write_offset(
                (offset + cursor) % BLOCK_SIZE,
                &data[cursor..cursor + write_len],
            );
            self.write_block(&block);
            cursor += write_len;
            iblock += 1;
        }
        if offset + cursor > file.inode.size() as usize {
            file.inode.set_size((offset + cursor) as u64);
        }
        self.write_inode_with_csum(&mut file);

        Ok(cursor)
    }

    /// Create a hard link. This function will not check name conflict.
    /// Call `lookup` to check beforehand.
    ///
    /// # Params
    ///
    /// * `child` - the inode of the file to link
    /// * `parent` - the inode of the directory to link to
    ///
    /// # Error
    ///
    /// * `ENOTDIR` - `parent` is not a directory
    /// * `ENOSPC` - no space left on device
    pub fn link(&mut self, child: InodeId, parent: InodeId, name: &str) -> Result<()> {
        let mut parent = self.read_inode(parent);
        // Can only link to a directory
        if !parent.inode.is_dir() {
            return_error!(ErrCode::ENOTDIR, "Inode {} is not a directory", parent.id);
        }
        let mut child = self.read_inode(child);
        self.link_inode(&mut parent, &mut child, name)?;
        Ok(())
    }

    /// Unlink a file.
    ///
    /// # Params
    ///
    /// * `parent` - the inode of the directory to unlink from
    /// * `name` - the name of the file to unlink
    ///
    /// # Error
    ///
    /// * `ENOTDIR` - `parent` is not a directory
    /// * `ENOENT` - `name` does not exist in `parent`
    pub fn unlink(&mut self, parent: InodeId, name: &str) -> Result<()> {
        let mut parent = self.read_inode(parent);
        // Can only unlink from a directory
        if !parent.inode.is_dir() {
            return_error!(ErrCode::ENOTDIR, "Inode {} is not a directory", parent.id);
        }
        let child_id = self.dir_find_entry(&parent, name)?.inode();
        let mut child = self.read_inode(child_id);
        self.unlink_inode(&mut parent, &mut child, name)
    }

    /// Move a file. This function will not check name conflict.
    /// Call `lookup` to check beforehand.
    ///
    /// # Params
    ///
    /// * `parent` - the inode of the directory to move from
    /// * `name` - the name of the file to move
    /// * `new_parent` - the inode of the directory to move to
    /// * `new_name` - the new name of the file
    ///
    /// # Error
    ///
    /// * `ENOTDIR` - `parent` or `new_parent` is not a directory
    /// * `ENOENT` - `name` does not exist in `parent`
    /// * `ENOSPC` - no space left on device
    pub fn rename(
        &mut self,
        parent: InodeId,
        name: &str,
        new_parent: InodeId,
        new_name: &str,
    ) -> Result<()> {
        let mut parent = self.read_inode(parent);
        if !parent.inode.is_dir() {
            return_error!(ErrCode::ENOTDIR, "Inode {} is not a directory", parent.id);
        }
        let mut new_parent = self.read_inode(new_parent);
        if !new_parent.inode.is_dir() {
            return_error!(
                ErrCode::ENOTDIR,
                "Inode {} is not a directory",
                new_parent.id
            );
        }

        let child_id = self.dir_find_entry(&parent, name)?;
        let mut child = self.read_inode(child_id.inode());

        self.link_inode(&mut new_parent, &mut child, new_name)?;
        self.unlink_inode(&mut parent, &mut child, name)
    }

    /// Create a directory. This function will not check name conflict.
    /// Call `lookup` to check beforehand.
    ///
    /// # Params
    ///
    /// * `parent` - the inode of the directory to create in
    /// * `name` - the name of the directory to create
    /// * `mode` - the mode of the directory to create, type field will be ignored
    ///
    /// # Return
    ///
    /// `Ok(child)` - the inode id of the created directory
    ///
    /// # Error
    ///
    /// * `ENOTDIR` - `parent` is not a directory
    /// * `ENOSPC` - no space left on device
    pub fn mkdir(&mut self, parent: InodeId, name: &str, mode: InodeMode) -> Result<InodeId> {
        let mut parent = self.read_inode(parent);
        // Can only create a directory in a directory
        if !parent.inode.is_dir() {
            return_error!(ErrCode::ENOTDIR, "Inode {} is not a directory", parent.id);
        }
        // Create file/directory
        let mode = mode & InodeMode::PERM_MASK | InodeMode::DIRECTORY;
        let mut child = self.create_inode(mode)?;
        // Link the new inode
        self.link_inode(&mut parent, &mut child, name)?;
        Ok(child.id)
    }

    /// Look up a directory entry by name.
    ///
    /// # Params
    ///
    /// * `parent` - the inode of the directory to look in
    /// * `name` - the name of the entry to look for
    ///
    /// # Return
    ///
    /// `Ok(child)`- the inode id to which the directory entry points.
    ///
    /// # Error
    ///
    /// * `ENOTDIR` - `parent` is not a directory
    /// * `ENOENT` - `name` does not exist in `parent`
    pub fn lookup(&mut self, parent: InodeId, name: &str) -> Result<InodeId> {
        let parent = self.read_inode(parent);
        // Can only lookup in a directory
        if !parent.inode.is_dir() {
            return_error!(ErrCode::ENOTDIR, "Inode {} is not a directory", parent.id);
        }
        self.dir_find_entry(&parent, name)
            .map(|entry| entry.inode())
    }

    /// List all directory entries in a directory.
    ///
    /// # Params
    ///
    /// * `inode` - the inode of the directory to list
    ///
    /// # Return
    ///
    /// `Ok(entries)` - a vector of directory entries in the directory.
    ///
    /// # Error
    ///
    /// `ENOTDIR` - `inode` is not a directory
    pub fn list(&self, inode: InodeId) -> Result<Vec<DirEntry>> {
        let inode_ref = self.read_inode(inode);
        // Can only list a directory
        if inode_ref.inode.file_type() != FileType::Directory {
            return_error!(ErrCode::ENOTDIR, "Inode {} is not a directory", inode);
        }
        Ok(self.dir_get_all_entries(&inode_ref))
    }

    /// Remove an empty directory.
    ///
    /// # Params
    ///
    /// * `parent` - the parent directory where the directory is located
    /// * `name` - the name of the directory to remove
    ///
    /// # Error
    ///
    /// * `ENOTDIR` - `parent` or `child` is not a directory
    /// * `ENOENT` - `name` does not exist in `parent`
    /// * `ENOTEMPTY` - `child` is not empty
    pub fn rmdir(&mut self, parent: InodeId, name: &str) -> Result<()> {
        let mut parent = self.read_inode(parent);
        // Can only remove a directory in a directory
        if !parent.inode.is_dir() {
            return_error!(ErrCode::ENOTDIR, "Inode {} is not a directory", parent.id);
        }
        let mut child = self.read_inode(self.dir_find_entry(&parent, name)?.inode());
        // Child must be a directory
        if !child.inode.is_dir() {
            return_error!(ErrCode::ENOTDIR, "Inode {} is not a directory", child.id);
        }
        // Child must be empty
        if self.dir_get_all_entries(&child).len() > 2 {
            return_error!(ErrCode::ENOTEMPTY, "Directory {} is not empty", child.id);
        }
        // Remove directory entry
        self.unlink_inode(&mut parent, &mut child, name)
    }
}
