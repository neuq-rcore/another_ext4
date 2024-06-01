use crate::prelude::*;

#[derive(Debug)]
pub struct FileHandler {
    /// The inode number of the file
    pub inode: InodeId,
    /// The open flags for the file
    pub flags: OpenFlags,
    /// The size of the file
    pub fsize: u64,
    /// The current position in the file
    pub fpos: usize,
}

impl FileHandler {
    pub fn new(inode: InodeId, flags: OpenFlags, fsize: u64) -> Self {
        FileHandler {
            inode,
            flags,
            fsize,
            fpos: 0,
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct OpenFlags: u32 {
        const O_ACCMODE = 0o0003;
        const O_RDONLY = 0o00;
        const O_WRONLY = 0o01;
        const O_RDWR = 0o02;
        const O_CREAT = 0o0100;
        const O_EXCL = 0o0200;
        const O_NOCTTY = 0o0400;
        const O_TRUNC = 0o01000;
        const O_APPEND = 0o02000;
        const O_NONBLOCK = 0o04000;
        const O_NDELAY = Self::O_NONBLOCK.bits();
        const O_SYNC = 0o4010000;
        const O_FSYNC = Self::O_SYNC.bits();
        const O_ASYNC = 0o020000;
        const O_LARGEFILE = 0o0100000;
        const O_DIRECTORY = 0o0200000;
        const O_NOFOLLOW = 0o0400000;
        const O_CLOEXEC = 0o2000000;
        const O_DIRECT = 0o040000;
        const O_NOATIME = 0o1000000;
        const O_PATH = 0o10000000;
        const O_DSYNC = 0o010000;
        const O_TMPFILE = 0o20000000 | Self::O_DIRECTORY.bits();
    }
}

impl OpenFlags {
    pub fn from_str(flags: &str) -> Result<Self> {
        match flags {
            "r" | "rb" => Ok(Self::O_RDONLY),
            "w" | "wb" => Ok(Self::O_WRONLY | Self::O_CREAT | Self::O_TRUNC),
            "a" | "ab" => Ok(Self::O_WRONLY | Self::O_CREAT | Self::O_APPEND),
            "r+" | "rb+" | "r+b" => Ok(Self::O_RDWR),
            "w+" | "wb+" | "w+b" => Ok(Self::O_RDWR | Self::O_CREAT | Self::O_TRUNC),
            "a+" | "ab+" | "a+b" => Ok(Self::O_RDWR | Self::O_CREAT | Self::O_APPEND),
            _ => Err(Ext4Error::new(ErrCode::EINVAL)),
        }
    }
}

#[derive(Copy, PartialEq, Eq, Clone, Debug)]
#[allow(unused)]
pub enum SeekFrom {
    Start(usize),
    End(isize),
    Current(isize),
}
