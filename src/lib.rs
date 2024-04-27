//! The Ext4 filesystem implementation in Rust.

#![feature(error_in_core)]
#![no_std]

extern crate alloc;

mod constants;
mod ext4;
mod ext4_defs;
mod ext4_error;
mod jbd2;
mod prelude;
mod utils;

pub use ext4::*;
pub use ext4_defs::*;
pub use ext4_error::*;
pub use utils::*;

pub const BLOCK_SIZE: usize = 4096;
