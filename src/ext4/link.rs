use super::Ext4;
use crate::prelude::*;
use crate::ext4_defs::*;

impl Ext4 {
    pub fn link(
        &mut self,
        parent: &mut InodeRef,
        child: &mut InodeRef,
        name: &str,
    ) -> Result<()> {
        // Add entry to parent directory
        let _r = self.dir_add_entry(parent, child, name);
        child.inode.links_count += 1;

        if child.inode.is_dir() {
            // add '.' and '..' entries
            let child_self = child.clone();
            self.dir_add_entry(child, &child_self, ".")?;
            self.dir_add_entry(child, parent, "..")?;
        }
        Ok(())
    }
}
