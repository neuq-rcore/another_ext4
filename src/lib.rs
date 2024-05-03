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

pub use ext4::*;
pub use ext4_defs::*;
pub use ext4_error::*;

pub const BLOCK_SIZE: usize = 4096;

#[cfg(test)]
mod unit_test {
    // use crate::Ext4;

    #[test]
    fn create_fs() {
        // let ext4 = Ext4::new();
    }
}