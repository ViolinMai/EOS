use core::arch::asm;
use alloc::string::String;
use alloc::vec::Vec;
use crate::log_info;

const ATA_PRIMARY_DATA_PORT: u16 = 0x1F0;
const ATA_PRIMARY_SECTOR_CNT_PORT: u16 = 0x1F2;
const ATA_PRIMARY_LBA_LO_PORT: u16 = 0x1F3;
const ATA_PRIMARY_LBA_MID_PORT: u16 = 0x1F4;
const ATA_PRIMARY_LBA_HI_PORT: u16 = 0x1F5;
const ATA_PRIMARY_DRIVE_PORT: u16 = 0x1F6;
const ATA_PRIMARY_COMMAND_PORT: u16 = 0x1F7;
const ATA_PRIMARY_STATUS_PORT: u16 = 0x1F7;

const ATA_CMD_READ_SECTORS: u8 = 0x20;

const STATUS_BSY: u8 = 0x80;
const STATUS_DRQ: u8 = 0x08;
const STATUS_ERR: u8 = 0x01;

#[derive(Clone, Debug)]
pub struct DiskFileInfo {
    pub name: String,
    pub size: u32,
    pub first_cluster: u32,
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe {
        asm!("in ax, dx", in("dx") port, out("ax") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn io_wait() {
    unsafe {
        for _ in 0..15 {
            let _ = inb(ATA_PRIMARY_STATUS_PORT);
        }
    }
}

fn wait_ready() -> Result<(), &'static str> {
    for _ in 0..100_000 {
        let status = unsafe { inb(ATA_PRIMARY_STATUS_PORT) };
        if (status & STATUS_BSY) == 0 {
            if (status & STATUS_ERR) != 0 {
                return Err("ATA Drive error detected in status register");
            }
            if (status & STATUS_DRQ) != 0 {
                return Ok(());
            }
        }
    }
    Err("ATA Timeout: Drive did not become ready")
}

pub fn read_sector_drive(drive: u8, lba: u32, buffer: &mut [u8; 512]) -> Result<(), &'static str> {
    if lba >= 0x1000_0000 {
        return Err("LBA out of range for 28-bit addressing");
    }

    unsafe {
        // drive 0 = Master (0xE0), drive 1 = Slave (0xF0)
        let drive_head = if drive == 0 { 0xE0 } else { 0xF0 };
        outb(ATA_PRIMARY_DRIVE_PORT, drive_head | (((lba >> 24) & 0x0F) as u8));
        io_wait();

        outb(ATA_PRIMARY_SECTOR_CNT_PORT, 1);
        outb(ATA_PRIMARY_LBA_LO_PORT, (lba & 0xFF) as u8);
        outb(ATA_PRIMARY_LBA_MID_PORT, ((lba >> 8) & 0xFF) as u8);
        outb(ATA_PRIMARY_LBA_HI_PORT, ((lba >> 16) & 0xFF) as u8);
        outb(ATA_PRIMARY_COMMAND_PORT, ATA_CMD_READ_SECTORS);

        wait_ready()?;

        for i in 0..256 {
            let word = inw(ATA_PRIMARY_DATA_PORT);
            buffer[i * 2] = (word & 0xFF) as u8;
            buffer[i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
        }
    }

    Ok(())
}

pub fn read_sector(lba: u32, buffer: &mut [u8; 512]) -> Result<(), &'static str> {
    read_sector_drive(0, lba, buffer)
}

pub fn scan_shared_disk() -> Result<Vec<DiskFileInfo>, &'static str> {
    let mut sector = [0u8; 512];
    read_sector_drive(1, 0, &mut sector)?;

    let mut start_lba = 0u32;
    if sector[510] == 0x55 && sector[511] == 0xAA {
        let part_type = sector[446 + 4];
        if part_type != 0 && (sector[446] == 0x80 || sector[446] == 0x00) {
            let p_lba = u32::from_le_bytes([sector[446 + 8], sector[446 + 9], sector[446 + 10], sector[446 + 11]]);
            if p_lba > 0 && p_lba < 0x1000_0000 {
                start_lba = p_lba;
                read_sector_drive(1, start_lba, &mut sector)?;
            }
        }
    }

    let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]) as u32;
    if bytes_per_sector != 512 {
        return Err("Unsupported sector size or unformatted share disk");
    }

    let spc = sector[13] as u32;
    let reserved_sectors = u16::from_le_bytes([sector[14], sector[15]]) as u32;
    let num_fats = sector[16] as u32;
    let root_entries = u16::from_le_bytes([sector[17], sector[18]]) as u32;
    let mut fat_size = u16::from_le_bytes([sector[22], sector[23]]) as u32;

    let is_fat32 = root_entries == 0 && fat_size == 0;

    let (root_dir_lba, root_dir_sectors) = if is_fat32 {
        let fat_sz32 = u32::from_le_bytes([sector[36], sector[37], sector[38], sector[39]]);
        let root_cluster = u32::from_le_bytes([sector[44], sector[45], sector[46], sector[47]]);
        let data_start = start_lba + reserved_sectors + (num_fats * fat_sz32);
        let cluster_lba = data_start + ((root_cluster.saturating_sub(2)) * spc);
        (cluster_lba, spc)
    } else {
        if fat_size == 0 {
            fat_size = u32::from_le_bytes([sector[36], sector[37], sector[38], sector[39]]);
        }
        let root_lba = start_lba + reserved_sectors + (num_fats * fat_size);
        let root_secs = ((root_entries * 32) + 511) / 512;
        (root_lba, root_secs)
    };

    let mut files = Vec::new();

    for s in 0..root_dir_sectors {
        read_sector_drive(1, root_dir_lba + s, &mut sector)?;
        for entry_idx in 0..16 {
            let offset = entry_idx * 32;
            let first_byte = sector[offset];
            if first_byte == 0x00 {
                break;
            }
            if first_byte == 0xE5 {
                continue;
            }

            let attr = sector[offset + 11];
            if attr == 0x0F || (attr & 0x08) != 0 || (attr & 0x10) != 0 {
                continue;
            }

            let name_raw = &sector[offset..offset + 8];
            let ext_raw = &sector[offset + 8..offset + 11];

            let mut filename = String::new();
            for &b in name_raw {
                if b != b' ' { filename.push(b as char); }
            }
            let mut ext = String::new();
            for &b in ext_raw {
                if b != b' ' { ext.push(b as char); }
            }

            if !ext.is_empty() {
                filename.push('.');
                filename.push_str(&ext);
            }

            let first_cluster_low = u16::from_le_bytes([sector[offset + 26], sector[offset + 27]]) as u32;
            let first_cluster_high = u16::from_le_bytes([sector[offset + 20], sector[offset + 21]]) as u32;
            let first_cluster = (first_cluster_high << 16) | first_cluster_low;
            let file_size = u32::from_le_bytes([
                sector[offset + 28],
                sector[offset + 29],
                sector[offset + 30],
                sector[offset + 31],
            ]);

            files.push(DiskFileInfo {
                name: filename,
                size: file_size,
                first_cluster,
            });
        }
    }

    Ok(files)
}

pub fn init() {
    log_info!("ATA: Primary IDE Controller (Master/Slave) initialized with FAT16/FAT32 support.");
}
