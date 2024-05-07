use crate::prelude::*;
use crate::constants::*;

/// 检查位图中的某一位是否被设置
/// 参数 bmap 位图缓冲区
/// 参数 bit 要检查的位
pub fn ext4_bmap_is_bit_set(bmap: &[u8], bit: u32) -> bool {
    bmap[(bit >> 3) as usize] & (1 << (bit & 7)) != 0
}

/// 检查位图中的某一位是否被清除
/// 参数 bmap 位图缓冲区
/// 参数 bit 要检查的位
pub fn ext4_bmap_is_bit_clr(bmap: &[u8], bit: u32) -> bool {
    !ext4_bmap_is_bit_set(bmap, bit)
}

/// 设置位图中的某一位
/// 参数 bmap 位图
/// 参数 bit 要设置的位
pub fn ext4_bmap_bit_set(bmap: &mut [u8], bit: u32) {
    bmap[(bit >> 3) as usize] |= 1 << (bit & 7);
}

/// 查找位图中第一个为0的位
pub fn ext4_bmap_bit_find_clr(bmap: &[u8], sbit: u32, ebit: u32, bit_id: &mut u32) -> bool {
    let mut i: u32;
    let mut bcnt = ebit - sbit;

    i = sbit;

    while i & 7 != 0 {
        if bcnt == 0 {
            return false;
        }

        if ext4_bmap_is_bit_clr(bmap, i) {
            *bit_id = sbit;
            return true;
        }

        i += 1;
        bcnt -= 1;
    }

    let mut sbit = i;
    let mut bmap = &bmap[(sbit >> 3) as usize..];
    while bcnt >= 8 {
        if bmap[0] != 0xFF {
            for i in 0..8 {
                if ext4_bmap_is_bit_clr(bmap, i) {
                    *bit_id = sbit + i;
                    return true;
                }
            }
        }

        bmap = &bmap[1..];
        bcnt -= 8;
        sbit += 8;
    }

    for i in 0..bcnt {
        if ext4_bmap_is_bit_clr(bmap, i) {
            *bit_id = sbit + i;
            return true;
        }
    }

    false
}

pub fn ext4_path_skip<'a>(path: &'a str, skip: &str) -> &'a str {
    let path = &path.trim_start_matches(skip);
    path
}

pub fn ext4_path_check(path: &str, is_goal: &mut bool) -> usize {
    for (i, c) in path.chars().enumerate() {
        if c == '/' {
            *is_goal = false;
            return i;
        }
    }
    let path = path.to_string();
    *is_goal = true;
    return path.len();
}

// 使用libc库定义的常量
pub fn ext4_parse_flags(flags: &str) -> Result<u32> {
    match flags {
        "r" | "rb" => Ok(O_RDONLY),
        "w" | "wb" => Ok(O_WRONLY | O_CREAT | O_TRUNC),
        "a" | "ab" => Ok(O_WRONLY | O_CREAT | O_APPEND),
        "r+" | "rb+" | "r+b" => Ok(O_RDWR),
        "w+" | "wb+" | "w+b" => Ok(O_RDWR | O_CREAT | O_TRUNC),
        "a+" | "ab+" | "a+b" => Ok(O_RDWR | O_CREAT | O_APPEND),
        _ => Err(Ext4Error::new(Errnum::EINVAL)),
    }
}
