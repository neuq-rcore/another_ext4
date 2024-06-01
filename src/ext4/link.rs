use super::Ext4;
use crate::ext4_defs::*;
use crate::prelude::*;

impl Ext4 {
    /// Link a child inode to a parent directory.
    pub(super) fn link_inode(
        &mut self,
        parent: &mut InodeRef,
        child: &mut InodeRef,
        name: &str,
    ) -> Result<()> {
        // Add entry to parent directory
        self.dir_add_entry(parent, child, name)?;
        // Update link count of child
        let link_cnt = child.inode.links_cnt() + 1;
        child.inode.set_links_cnt(link_cnt);
        // Add '.' and '..' entries if child is a newly created directory
        if link_cnt == 1 && child.inode.is_dir() {
            let child_self = child.clone();
            self.dir_add_entry(child, &child_self, ".")?;
            self.dir_add_entry(child, parent, "..")?;
        }
        self.write_inode_with_csum(child);
        Ok(())
    }

    /// Unlink a child inode from a parent directory.
    /// Free the inode if link count is 0.
    pub(super) fn unlink_inode(
        &mut self,
        parent: &mut InodeRef,
        child: &mut InodeRef,
        name: &str,
    ) -> Result<()> {
        // Remove entry from parent directory
        self.dir_remove_entry(parent, name)?;
        // Update link count of child
        let link_cnt = child.inode.links_cnt() - 1;
        if link_cnt == 0 {
            // Free the inode if link count is 0
            return self.free_inode(child);
        }
        child.inode.set_links_cnt(link_cnt);
        self.write_inode_with_csum(child);
        Ok(())
    }
}
