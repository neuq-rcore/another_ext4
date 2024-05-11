#![allow(unused)]

use crate::prelude::*;
use bitflags::bitflags;

pub const EXT4_INODE_FLAG_EXTENTS: u32 = 0x00080000; /* Inode uses extents */
pub const EXT4_MIN_BLOCK_GROUP_DESCRIPTOR_SIZE: u16 = 32;
pub const EXT4_MAX_BLOCK_GROUP_DESCRIPTOR_SIZE: u16 = 64;
pub const EXT4_CRC32_INIT: u32 = 0xFFFFFFFF;
pub const EXT4_EXTENT_MAGIC: u16 = 0xF30A;
pub const EXT_INIT_MAX_LEN: u16 = 32768;
pub const EXT_UNWRITTEN_MAX_LEN: u16 = 65535;

pub const EXT4_GOOD_OLD_INODE_SIZE: u16 = 128;

pub const EXT4_INODE_MODE_FIFO: u16 = 0x1000;
pub const EXT4_INODE_MODE_CHARDEV: u16 = 0x2000;
pub const EXT4_INODE_MODE_DIRECTORY: u16 = 0x4000;
pub const EXT4_INODE_MODE_BLOCKDEV: u16 = 0x6000;
pub const EXT4_INODE_MODE_FILE: u16 = 0x8000;
pub const EXT4_INODE_MODE_SOFTLINK: u16 = 0xA000;
pub const EXT4_INODE_MODE_SOCKET: u16 = 0xC000;
pub const EXT4_INODE_MODE_TYPE_MASK: u16 = 0xF000;

pub const EXT_MAX_BLOCKS: LBlockId = core::u32::MAX;

pub const EXT4_SUPERBLOCK_OS_HURD: u32 = 1;

/// Maximum bytes in a path
pub const PATH_MAX: usize = 4096;

/// Maximum bytes in a file name
pub const NAME_MAX: usize = 255;

/// The upper limit for resolving symbolic links
pub const SYMLINKS_MAX: usize = 40;

/// The inode number of root inode
pub const EXT4_ROOT_INO: u32 = 2;

pub const EOK: usize = 0;
pub const EPERM: usize = 1; /* Operation not permitted */
pub const ENOENT: usize = 2; /* No such file or directory */
pub const ESRCH: usize = 3; /* No such process */
pub const EINTR: usize = 4; /* Interrupted system call */
pub const EIO: usize = 5; /* I/O error */
pub const ENXIO: usize = 6; /* No such device or address */
pub const E2BIG: usize = 7; /* Argument list too long */
pub const ENOEXEC: usize = 8; /* Exec format error */
pub const EBADF: usize = 9; /* Bad file number */
pub const ECHILD: usize = 10; /* No child processes */
pub const EAGAIN: usize = 11; /* Try again */
pub const ENOMEM: usize = 12; /* Out of memory */
pub const EACCES: usize = 13; /* Permission denied */
pub const EFAULT: usize = 14; /* Bad address */
pub const ENOTBLK: usize = 15; /* Block device required */
pub const EBUSY: usize = 16; /* Device or resource busy */
pub const EEXIST: usize = 17; /* File exists */
pub const EXDEV: usize = 18; /* Cross-device link */
pub const ENODEV: usize = 19; /* No such device */
pub const ENOTDIR: usize = 20; /* Not a directory */
pub const EISDIR: usize = 21; /* Is a directory */
pub const EINVAL: usize = 22; /* Invalid argument */
pub const ENFILE: usize = 23; /* File table overflow */
pub const EMFILE: usize = 24; /* Too many open files */
pub const ENOTTY: usize = 25; /* Not a typewriter */
pub const ETXTBSY: usize = 26; /* Text file busy */
pub const EFBIG: usize = 27; /* File too large */
pub const ENOSPC: usize = 28; /* No space left on device */
pub const ESPIPE: usize = 29; /* Illegal seek */
pub const EROFS: usize = 30; /* Read-only file system */
pub const EMLINK: usize = 31; /* Too many links */
pub const EPIPE: usize = 32; /* Broken pipe */
pub const EDOM: usize = 33; /* Math argument out of domain of func */
pub const ERANGE: usize = 34; /* Math result not representable */

pub const BASE_OFFSET: usize = 1024;
pub const BLOCK_SIZE: usize = 4096;
