use super::Ext4;
use crate::prelude::*;
use crate::ext4_defs::*;

impl Ext4 {
    pub fn ext4_link(
        &mut self,
        parent: &mut Ext4InodeRef,
        child: &mut Ext4InodeRef,
        name: &str,
    ) -> Result<()> {
        // Add entry to parent directory
        let _r = self.dir_add_entry(parent, child, name);
        child.inode.links_count += 1;

        if child.inode.is_dir(&self.super_block) {
            // add '.' and '..' entries
            let child_self = child.clone();
            self.dir_add_entry(child, &child_self, ".")?;
            child.inode.links_count += 1;
            self.dir_add_entry(child, parent, "..")?;
            parent.inode.links_count += 1;
        }
        Ok(())
    }
}
