use super::Ext4MountPoint;
use crate::prelude::*;

/// 文件描述符
pub struct Ext4File {
    /// 挂载点句柄
    pub mp: *mut Ext4MountPoint,
    /// 文件 inode id
    pub inode: u32,
    /// 打开标志
    pub flags: u32,
    /// 文件大小
    pub fsize: u64,
    /// 实际文件位置
    pub fpos: usize,
}

impl Ext4File {
    pub fn new() -> Self {
        Self {
            mp: core::ptr::null_mut(),
            inode: 0,
            flags: 0,
            fsize: 0,
            fpos: 0,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Ext4OpenFlags {
    ReadOnly,
    WriteOnly,
    WriteCreateTrunc,
    WriteCreateAppend,
    ReadWrite,
    ReadWriteCreateTrunc,
    ReadWriteCreateAppend,
}

// 实现一个从字符串转换为标志的函数
// 使用core::str::FromStr特性[^1^][1]
impl core::str::FromStr for Ext4OpenFlags {
    type Err = String;

    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        match s {
            "r" | "rb" => Ok(Ext4OpenFlags::ReadOnly),
            "w" | "wb" => Ok(Ext4OpenFlags::WriteOnly),
            "a" | "ab" => Ok(Ext4OpenFlags::WriteCreateAppend),
            "r+" | "rb+" | "r+b" => Ok(Ext4OpenFlags::ReadWrite),
            "w+" | "wb+" | "w+b" => Ok(Ext4OpenFlags::ReadWriteCreateTrunc),
            "a+" | "ab+" | "a+b" => Ok(Ext4OpenFlags::ReadWriteCreateAppend),
            _ => Err(alloc::format!("Unknown open mode: {}", s)),
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

