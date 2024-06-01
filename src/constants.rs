#![allow(unused)]

use crate::prelude::*;

pub const EXT4_INODE_FLAG_EXTENTS: u32 = 0x00080000; /* Inode uses extents */
pub const EXT4_MIN_BLOCK_GROUP_DESCRIPTOR_SIZE: u16 = 32;
pub const EXT4_MAX_BLOCK_GROUP_DESCRIPTOR_SIZE: u16 = 64;
pub const EXT4_CRC32_INIT: u32 = 0xFFFFFFFF;
pub const EXT4_EXTENT_MAGIC: u16 = 0xF30A;
pub const EXT_INIT_MAX_LEN: u16 = 32768;
pub const EXT_UNWRITTEN_MAX_LEN: u16 = 65535;

pub const EXT4_GOOD_OLD_INODE_SIZE: u16 = 128;

pub const EXT_MAX_BLOCKS: LBlockId = core::u32::MAX;

pub const EXT4_SUPERBLOCK_OS_HURD: u32 = 1;

/// Maximum bytes in a path
pub const PATH_MAX: usize = 4096;

/// Maximum bytes in a file name
pub const NAME_MAX: usize = 255;

/// The upper limit for resolving symbolic links
pub const SYMLINKS_MAX: usize = 40;

/// The inode number of root inode
pub const EXT4_ROOT_INO: InodeId = 1;

pub const BASE_OFFSET: usize = 1024;
pub const BLOCK_SIZE: usize = 4096;
pub const INODE_BLOCK_SIZE: usize = 512;
