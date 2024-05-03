use super::Ext4;
use crate::constants::*;
use crate::ext4_defs::*;

impl Ext4 {
    pub fn ext4_link(
        &self,
        parent: &mut Ext4InodeRef,
        child: &mut Ext4InodeRef,
        name: &str,
        name_len: u32,
    ) -> usize {
        // log::info!("link parent inode {:x?} child inode {:x?} name {:?}", parent.inode_num, child.inode_num, name);
        /* Add entry to parent directory */
        let _r = self.ext4_dir_add_entry(parent, child, name, name_len);
    
        /* Fill new dir -> add '.' and '..' entries.
         * Also newly allocated inode should have 0 link count.
            */
        let mut is_dir = false;
        if child.inode.mode & EXT4_INODE_MODE_TYPE_MASK as u16 == EXT4_INODE_MODE_DIRECTORY as u16
        {
            is_dir = true;
        }
    
        if is_dir {
            // add '.' and '..' entries
            let mut child_inode_ref = Ext4InodeRef::default();
            child_inode_ref.inode_id = child.inode_id;
            child_inode_ref.inode = child.inode.clone();
    
            let _r = self.ext4_dir_add_entry(&mut child_inode_ref, child, ".", 1);
            child.inode.size = child_inode_ref.inode.size;
            child.inode.block = child_inode_ref.inode.block;
            let _r = self.ext4_dir_add_entry(&mut child_inode_ref, parent, "..", 2);
    
            child.inode.links_count = 2;
            parent.inode.links_count += 1;
    
            return EOK;
        }
    
        child.inode.links_count += 1;
        EOK
    }
}
