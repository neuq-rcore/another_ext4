// SPDX-License-Identifier: MPL-2.0
extern crate alloc;

use crate::prelude::*;

/// Ext4Error number.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrCode {
    EPERM = 1,       /* Operation not permitted */
    ENOENT = 2,      /* No such file or directory */
    EIO = 5,         /* I/O error */
    ENXIO = 6,       /* No such device or address */
    E2BIG = 7,       /* Argument list too long */
    ENOMEM = 12,     /* Out of memory */
    EACCES = 13,     /* Permission denied */
    EFAULT = 14,     /* Bad address */
    EEXIST = 17,     /* File exists */
    ENODEV = 19,     /* No such device */
    ENOTDIR = 20,    /* Not a directory */
    EISDIR = 21,     /* Is a directory */
    EINVAL = 22,     /* Invalid argument */
    EFBIG = 27,      /* File too large */
    ENOSPC = 28,     /* No space left on device */
    EROFS = 30,      /* Read-only file system */
    EMLINK = 31,     /* Too many links */
    ERANGE = 34,     /* Math result not representable */
    ENOTEMPTY = 39,  /* Directory not empty */
    ENODATA = 61,    /* No data available */
    ENOTSUP = 95,    /* Not supported */
    ELINKFAIL = 97,  /* Link failed */
    EALLOCFIAL = 98, /* Inode alloc failed */
}

/// error used in this crate
pub struct Ext4Error {
    code: ErrCode,
    message: Option<String>,
}

impl Debug for Ext4Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(message) = &self.message {
            write!(
                f,
                "Ext4Error {{ code: {:?}, message: {:?} }}",
                self.code, message
            )
        } else {
            write!(f, "Ext4Error {{ code: {:?} }}", self.code)
        }
    }
}

impl Ext4Error {
    pub const fn new(code: ErrCode) -> Self {
        Ext4Error {
            code,
            message: None,
        }
    }

    pub const fn with_message(code: ErrCode, message: String) -> Self {
        Ext4Error {
            code,
            message: Some(message),
        }
    }

    pub const fn code(&self) -> ErrCode {
        self.code
    }
}

#[macro_export]
macro_rules! format_error {
    ($code: expr, $message: expr) => {
        crate::error::Ext4Error::with_message($code, format!($message))
    };
    ($code: expr, $fmt: expr,  $($args:tt)*) => {
        crate::error::Ext4Error::with_message($code, format!($fmt, $($args)*))
    };
}

#[macro_export]
macro_rules! return_error {
    ($code: expr, $message: expr) => {
        return Err(crate::format_error!($code, $message));
    };
    ($code: expr, $fmt: expr,  $($args:tt)*) => {
        return Err(crate::format_error!($code, $fmt, $($args)*));
    }
}
