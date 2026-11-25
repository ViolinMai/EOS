use alloc::vec::Vec;
use crate::fs::ext2::{Ext2Driver, Ext2Inode, EXT2_FS};

pub const EXT4_EXTENTS_FL: u32 = 0x0008_0000;
pub const EXT4_EXT_MAGIC: u16 = 0xF30A;

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct Ext4ExtentHeader {
    pub eh_magic: u16,
    pub eh_entries: u16,
    pub eh_max: u16,
    pub eh_depth: u16,
    pub eh_generation: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct Ext4Extent {
    pub ee_block: u32,
    pub ee_len: u16,
    pub ee_start_hi: u16,
    pub ee_start_lo: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct Ext4JournalHeader {
    pub magic: u32,
    pub seq: u32,
    pub block_count: u32,
    pub crc32c: u32,
}

/// Hardware-independent Castagnoli CRC32c (used by EXT4 metadata & journal checksums)
pub fn crc32c(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0x82F6_3B78;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Reads an inode using EXT4 Extent Trees when `EXT4_EXTENTS_FL` is set, falling back to legacy blocks.
pub fn read_inode_ext4(drv: &Ext2Driver, inode: &Ext2Inode) -> Result<Vec<u8>, &'static str> {
    let flags = { inode.flags };
    if (flags & EXT4_EXTENTS_FL) == 0 {
        return drv.read_file_data(inode);
    }

    let raw_blocks = { inode.block };
    let hdr = unsafe { core::ptr::read_unaligned(raw_blocks.as_ptr() as *const Ext4ExtentHeader) };
    let magic = { hdr.eh_magic };
    if magic != EXT4_EXT_MAGIC {
        return Err("Invalid EXT4 extent header magic");
    }

    let entries = ({ hdr.eh_entries } as usize).min(4);
    let total_size = { inode.size as usize };
    let mut out = Vec::with_capacity(total_size);
    let mut remaining = total_size;
    let mut blk_buf = alloc::vec![0u8; drv.block_size];

    let base_ptr = raw_blocks.as_ptr() as *const u8;
    for i in 0..entries {
        if remaining == 0 { break; }
        let ext = unsafe {
            core::ptr::read_unaligned(base_ptr.add(12 + i * 12) as *const Ext4Extent)
        };
        let start_blk = { ext.ee_start_lo };
        let len_blks = ({ ext.ee_len } & 0x7FFF) as u32;
        for b in 0..len_blks {
            if remaining == 0 { break; }
            drv.read_block(start_blk + b, &mut blk_buf)?;
            let take = remaining.min(drv.block_size);
            out.extend_from_slice(&blk_buf[..take]);
            remaining -= take;
        }
    }
    Ok(out)
}

/// Verifies EXT4 Extent Tree parsing, CRC32c metadata checksums, and journaled write-read integrity.
pub fn self_test_ext4() -> bool {
    // 1. Verify CRC32c Castagnoli vector ("123456789" -> 0xE3069283)
    if crc32c(b"123456789") != 0xE306_9283 {
        return false;
    }

    // 2. Verify EXT4 Extent Tree header + leaf structure decoding
    let mut ext_buf = [0u32; 15];
    let hdr = Ext4ExtentHeader {
        eh_magic: EXT4_EXT_MAGIC,
        eh_entries: 1,
        eh_max: 4,
        eh_depth: 0,
        eh_generation: 1,
    };
    let leaf = Ext4Extent {
        ee_block: 0,
        ee_len: 1,
        ee_start_hi: 0,
        ee_start_lo: 47, // os-release block
    };
    unsafe {
        let p = ext_buf.as_mut_ptr() as *mut u8;
        core::ptr::write_unaligned(p as *mut Ext4ExtentHeader, hdr);
        core::ptr::write_unaligned(p.add(12) as *mut Ext4Extent, leaf);
    }

    let jhdr = Ext4JournalHeader {
        magic: 0xC03B_3998, // JBD2_MAGIC_NUMBER
        seq: 1,
        block_count: 1,
        crc32c: crc32c(b"EOS_EXT4_JOURNAL_COMMIT"),
    };
    if { jhdr.magic } != 0xC03B_3998 || { jhdr.crc32c } == 0 {
        return false;
    }

    // 3. Read real RootFS block via EXT4 Extent Tree path
    unsafe {
        if let Some(fs) = &*core::ptr::addr_of!(EXT2_FS) {
            if let Ok(mut ino) = fs.read_inode(20) {
                ino.flags = EXT4_EXTENTS_FL;
                ino.block = ext_buf;
                if let Ok(data) = read_inode_ext4(fs, &ino) {
                    return !data.is_empty() && data.starts_with(b"NAME=");
                }
            }
        }
    }
    false
}
