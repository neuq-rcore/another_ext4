//! High-level operations of Ext4 filesystem.
//!
//! This module provides path-based operations. An object can be
//! located in the filesystem by its relative or absolute path.
//!
//! Some operations such as `read`, `write`, `setattr` do not involve
//! file location. They are implemented in the `low_level` module.
//! High-level and low-level operations can be used together to
//! implement more complex operations.

use super::Ext4;
use crate::ext4_defs::*;
use crate::prelude::*;
use crate::return_err_with_msg_str;

impl Ext4 {
    /// Look up an object in the filesystem.
    ///
    /// ## Params
    ///
    /// * `root` - The inode id of the root directory for search.
    /// * `path` - The path of the object to be opened.
    ///
    /// ## Return
    ///
    /// `Ok(inode)` - Inode id of the object
    pub fn generic_lookup(&mut self, root: InodeId, path: &str) -> Result<InodeId> {
        // Search from the given parent inode
        let mut cur = root;
        let search_path = Self::split_path(path);
        // Search recursively
        for path in search_path.iter() {
            cur = self.lookup(cur, path)?;
        }
        Ok(cur)
    }

    /// Open a file in the filesystem. Return error if the file does not exist.
    ///
    /// ## Params
    /// 
    /// * `root` - The inode id of the root directory for search.
    /// * `path` - The path of the object to be opened.
    /// * `flags` - The open flags. Creation (O_CREAT, O_EXCL, O_NOCTTY) flags
    ///             will be ignored.
    ///
    /// ## Return
    ///
    /// `Ok(fh)` - File handler
    pub fn generic_open(
        &mut self,
        root: InodeId,
        path: &str,
        flags: OpenFlags,
    ) -> Result<FileHandler> {
        let inode_id = self.generic_lookup(root, path)?;
        let inode = self.inode(inode_id);
        // Check file type
        if !inode.inode.is_file() {
            return_err_with_msg_str!(ErrCode::ENOENT, "Not a file");
        }
        Ok(FileHandler::new(inode.id, flags, inode.inode.size()))
    }

    /// Create an object in the filesystem. Return error if the object already exists.
    ///
    /// This function will perform recursive-creation i.e. if the parent
    /// directory does not exist, it will be created as well.
    ///
    /// ## Params
    ///
    /// * `root` - The inode id of the starting directory for search.
    /// * `path` - The path of the object to create.
    /// * `mode` - file mode and type to create
    ///
    /// ## Return
    ///
    /// `Ok(inode)` - Inode id of the created object
    pub fn generic_create(
        &mut self,
        root: InodeId,
        path: &str,
        mode: InodeMode,
    ) -> Result<InodeId> {
        // Search from the given parent inode
        let mut cur = self.read_inode(root);
        let search_path = Self::split_path(path);

        // Search recursively
        for (i, path) in search_path.iter().enumerate() {
            if !cur.inode.is_dir() {
                return_err_with_msg_str!(ErrCode::ENOTDIR, "Not a directory");
            }
            match self.dir_find_entry(&cur, &path) {
                Ok(de) => {
                    // If the object exists, check the type
                    cur = self.read_inode(de.inode());
                }
                Err(e) => {
                    if e.code() != ErrCode::ENOENT {
                        return Err(e);
                    }
                    // If the object does not exist, create it
                    let mut child = if i == search_path.len() - 1 {
                        // Create the file
                        self.create_inode(mode)?
                    } else {
                        // Create the directory
                        self.create_inode(InodeMode::DIRECTORY | InodeMode::ALL_RWX)?
                    };
                    self.link_inode(&mut cur, &mut child, path)
                        .map_err(|_| Ext4Error::with_msg_str(ErrCode::ELINKFAIL, "link fail"))?;
                    cur = child;
                }
            }
        }

        Ok(cur.id)
    }

    /// Remove an object from the filesystem. Return error if the object is a
    /// directory and is not empty.
    ///
    /// ## Params
    ///
    /// * `root` - The inode id of the starting directory for search.
    /// * `path` - The path of the object to remove.
    pub fn generic_remove(&mut self, root: InodeId, path: &str) -> Result<()> {
        // Get the parent directory path and the file name
        let mut search_path = Self::split_path(path);
        let file_name = &search_path.split_off(search_path.len() - 1)[0];
        let parent_path = search_path.join("/");
        // Get the parent directory inode
        let parent_id = self.generic_lookup(root, &parent_path)?;
        // Get the child inode
        let child_id = self.generic_lookup(parent_id, &file_name)?;
        let mut parent = self.read_inode(parent_id);
        let mut child = self.read_inode(child_id);
        if child.inode.is_dir() {
            // Check if the directory is empty
            if self.dir_get_all_entries(&child)?.len() > 2 {
                return_err_with_msg_str!(ErrCode::ENOTEMPTY, "Directory not empty");
            }
        }
        // Unlink the file
        self.unlink_inode(&mut parent, &mut child, file_name)
    }

    /// A helper function to split a path by '/'
    fn split_path(path: &str) -> Vec<String> {
        let _ = path.trim_start_matches("/");
        if path.is_empty() {
            return vec![]; // root
        }
        path.split("/").map(|s| s.to_string()).collect()
    }
}
