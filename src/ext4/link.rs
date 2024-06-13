use super::Ext4;
use crate::ext4_defs::*;
use crate::prelude::*;

impl Ext4 {
    /// Link a child inode to a parent directory.
    pub(super) fn link_inode(
        &self,
        parent: &mut InodeRef,
        child: &mut InodeRef,
        name: &str,
    ) -> Result<()> {
        // Add entry to parent directory
        self.dir_add_entry(parent, child, name)?;

        let child_link_count = child.inode.link_count();
        if child.inode.is_dir() && child_link_count == 0 {
            // Add '.' and '..' entries if child is a newly created directory
            let child_self = child.clone();
            self.dir_add_entry(child, &child_self, ".")?;
            self.dir_add_entry(child, parent, "..")?;
            // Link child/".."
            parent.inode.set_link_count(parent.inode.link_count() + 1);
            self.write_inode_with_csum(parent);
            // Link parent/child + child/"."
            child.inode.set_link_count(child_link_count + 2);
        } else {
            // Link parent/child
            child.inode.set_link_count(child_link_count + 1);
        }
        self.write_inode_with_csum(child);
        Ok(())
    }

    /// Unlink a child inode from a parent directory.
    /// Free the inode if link count is 0.
    pub(super) fn unlink_inode(
        &self,
        parent: &mut InodeRef,
        child: &mut InodeRef,
        name: &str,
    ) -> Result<()> {
        // Remove entry from parent directory
        self.dir_remove_entry(parent, name)?;

        let child_link_cnt = child.inode.link_count();
        if child.inode.is_dir() && child_link_cnt <= 2 {
            // Child is an empty directory
            // Unlink "child/.."
            parent.inode.set_link_count(parent.inode.link_count() - 1);
            self.write_inode_with_csum(parent);
            // Remove directory
            self.free_inode(child)
        } else if child_link_cnt <= 1 {
            // Child is a file
            // Remove file
            self.free_inode(child)
        } else {
            // Not remove
            child.inode.set_link_count(child_link_cnt - 1);
            self.write_inode_with_csum(child);
            Ok(())
        }
    }
}
