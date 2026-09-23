use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::ata::read_sector_drive;
use crate::{log_error, log_info};

pub const EXT2_MAGIC: u16 = 0xEF53;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext2Superblock {
    pub inodes_count: u32,
    pub blocks_count: u32,
    pub r_blocks_count: u32,
    pub free_blocks_count: u32,
    pub free_inodes_count: u32,
    pub first_data_block: u32,
    pub log_block_size: u32,
    pub log_frag_size: u32,
    pub blocks_per_group: u32,
    pub frags_per_group: u32,
    pub inodes_per_group: u32,
    pub mtime: u32,
    pub wtime: u32,
    pub mnt_count: u16,
    pub max_mnt_count: u16,
    pub magic: u16,
    pub state: u16,
    pub errors: u16,
    pub minor_rev_level: u16,
    pub lastcheck: u32,
    pub checkinterval: u32,
    pub creator_os: u32,
    pub rev_level: u32,
    pub def_resuid: u16,
    pub def_resgid: u16,
    pub first_ino: u32,
    pub inode_size: u16,
    pub block_group_nr: u16,
    pub feature_compat: u32,
    pub feature_incompat: u32,
    pub feature_ro_compat: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext2BlockGroupDesc {
    pub block_bitmap: u32,
    pub inode_bitmap: u32,
    pub inode_table: u32,
    pub free_blocks_count: u16,
    pub free_inodes_count: u16,
    pub used_dirs_count: u16,
    pub pad: u16,
    pub reserved: [u32; 3],
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext2Inode {
    pub mode: u16,
    pub uid: u16,
    pub size: u32,
    pub atime: u32,
    pub ctime: u32,
    pub mtime: u32,
    pub dtime: u32,
    pub gid: u16,
    pub links_count: u16,
    pub blocks: u32,
    pub flags: u32,
    pub osd1: u32,
    pub block: [u32; 15],
    pub generation: u32,
    pub file_acl: u32,
    pub dir_acl: u32,
    pub faddr: u32,
    pub osd2: [u32; 3],
}

pub struct Ext2Driver {
    pub drive: u8,
    pub block_size: usize,
    #[allow(dead_code)]
    pub inodes_per_group: usize,
    pub inode_size: usize,
    pub inode_table_block: u32,
}

pub static mut EXT2_FS: Option<Ext2Driver> = None;

impl Ext2Driver {
    pub fn read_block(&self, block_num: u32, buf: &mut [u8]) -> Result<(), &'static str> {
        let sectors_per_block = self.block_size / 512;
        let start_sector = block_num * sectors_per_block as u32;

        for s in 0..sectors_per_block {
            let offset = s * 512;
            let mut sec_buf = [0u8; 512];
            read_sector_drive(self.drive, start_sector + s as u32, &mut sec_buf)?;
            buf[offset..offset + 512].copy_from_slice(&sec_buf);
        }
        Ok(())
    }

    pub fn read_inode(&self, inode_num: u32) -> Result<Ext2Inode, &'static str> {
        if inode_num == 0 {
            return Err("Invalid Inode 0");
        }

        let index = inode_num - 1;
        let inode_table_offset = (index as usize) * self.inode_size;
        let block_offset = inode_table_offset / self.block_size;
        let byte_in_block = inode_table_offset % self.block_size;

        let target_block = self.inode_table_block + block_offset as u32;
        let mut buf = alloc::vec![0u8; self.block_size];
        self.read_block(target_block, &mut buf)?;

        unsafe {
            let ptr = buf.as_ptr().add(byte_in_block) as *const Ext2Inode;
            Ok(core::ptr::read_unaligned(ptr))
        }
    }

    pub fn read_file_data(&self, inode: &Ext2Inode) -> Result<Vec<u8>, &'static str> {
        let inode_size = { inode.size as usize };
        let mut data = Vec::with_capacity(inode_size);
        let mut remaining = inode_size;
        let mut block_buf = alloc::vec![0u8; self.block_size];

        for i in 0..12 {
            if remaining == 0 { break; }
            let blk = { inode.block[i] };
            if blk == 0 { break; }

            self.read_block(blk, &mut block_buf)?;
            let to_copy = core::cmp::min(remaining, self.block_size);
            data.extend_from_slice(&block_buf[..to_copy]);
            remaining -= to_copy;
        }

        let block_12 = { inode.block[12] };
        if remaining > 0 && block_12 != 0 {
            let mut indirect_buf = alloc::vec![0u8; self.block_size];
            self.read_block(block_12, &mut indirect_buf)?;
            let entries = self.block_size / 4;

            for i in 0..entries {
                if remaining == 0 { break; }
                let blk = u32::from_le_bytes([
                    indirect_buf[i * 4],
                    indirect_buf[i * 4 + 1],
                    indirect_buf[i * 4 + 2],
                    indirect_buf[i * 4 + 3],
                ]);
                if blk == 0 { continue; }

                self.read_block(blk, &mut block_buf)?;
                let to_copy = core::cmp::min(remaining, self.block_size);
                data.extend_from_slice(&block_buf[..to_copy]);
                remaining -= to_copy;
            }
        }

        Ok(data)
    }

    pub fn list_directory(&self, inode_num: u32) -> Result<Vec<(String, u32)>, &'static str> {
        let inode = self.read_inode(inode_num)?;
        let mode = { inode.mode };
        if (mode & 0x4000) == 0 {
            return Err("Inode is not a directory");
        }

        let dir_data = self.read_file_data(&inode)?;
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset + 8 <= dir_data.len() {
            let ino = u32::from_le_bytes([
                dir_data[offset],
                dir_data[offset + 1],
                dir_data[offset + 2],
                dir_data[offset + 3],
            ]);
            let rec_len = u16::from_le_bytes([dir_data[offset + 4], dir_data[offset + 5]]) as usize;
            let name_len = dir_data[offset + 6] as usize;

            if rec_len == 0 { break; }

            if ino != 0 && offset + 8 + name_len <= dir_data.len() {
                if let Ok(name) = core::str::from_utf8(&dir_data[offset + 8..offset + 8 + name_len]) {
                    if name != "." && name != ".." {
                        entries.push((String::from(name), ino));
                    }
                }
            }
            offset += rec_len;
        }

        Ok(entries)
    }
}

pub fn init(drive: u8) {
    let mut sb_buf = [0u8; 1024];
    let mut sec0 = [0u8; 512];
    let mut sec1 = [0u8; 512];

    if read_sector_drive(drive, 2, &mut sec0).is_err() || read_sector_drive(drive, 3, &mut sec1).is_err() {
        return;
    }
    sb_buf[..512].copy_from_slice(&sec0);
    sb_buf[512..].copy_from_slice(&sec1);

    unsafe {
        let sb = core::ptr::read_unaligned(sb_buf.as_ptr() as *const Ext2Superblock);
        let magic = sb.magic;
        if magic != EXT2_MAGIC {
            log_error!("EXT2", "No valid ext2 filesystem on drive {} (Magic: {:#06x})", drive, magic);
            return;
        }

        let block_size = 1024usize << sb.log_block_size;
        let inode_size = if sb.rev_level >= 1 { sb.inode_size as usize } else { 128 };
        let inodes_per_group = sb.inodes_per_group as usize;
        let inodes_count = sb.inodes_count;

        let bgd_block = if block_size == 1024 { 2 } else { 1 };
        let mut bgd_buf = alloc::vec![0u8; block_size];
        
        let driver = Ext2Driver {
            drive,
            block_size,
            inodes_per_group,
            inode_size,
            inode_table_block: 5,
        };

        if driver.read_block(bgd_block, &mut bgd_buf).is_ok() {
            let bgd = core::ptr::read_unaligned(bgd_buf.as_ptr() as *const Ext2BlockGroupDesc);
            let active_driver = Ext2Driver {
                drive,
                block_size,
                inodes_per_group,
                inode_size,
                inode_table_block: bgd.inode_table,
            };

            log_info!("EXT2", "Linux EXT2 Filesystem mounted on drive {}. BlockSize: {} B, Inodes: {}",
                drive, block_size, inodes_count
            );
            EXT2_FS = Some(active_driver);
        }
    }
}
