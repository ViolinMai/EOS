use core::arch::asm;
use alloc::string::String;
use alloc::vec::Vec;
use crate::{log_info, log_error};

const ATA_PRIMARY_IO_BASE: u16 = 0x1F0;
const ATA_SECONDARY_IO_BASE: u16 = 0x170;

const ATA_REG_DATA: u16 = 0;
#[allow(dead_code)]
const ATA_REG_ERROR: u16 = 1;
const ATA_REG_SECTOR_CNT: u16 = 2;
const ATA_REG_LBA_LO: u16 = 3;
const ATA_REG_LBA_MID: u16 = 4;
const ATA_REG_LBA_HI: u16 = 5;
const ATA_REG_DRIVE: u16 = 6;
const ATA_REG_STATUS: u16 = 7;
const ATA_REG_COMMAND: u16 = 7;

const ATA_CMD_READ_SECTORS: u8 = 0x20;
const ATA_CMD_WRITE_SECTORS: u8 = 0x30;
const ATA_CMD_CACHE_FLUSH: u8 = 0xE7;

const STATUS_BSY: u8 = 0x80;
const STATUS_DRDY: u8 = 0x40;
const STATUS_DRQ: u8 = 0x08;
const STATUS_ERR: u8 = 0x01;
const STATUS_DF: u8 = 0x20;

pub const PERSISTENT_CONFIG_SECTOR: u32 = 500;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MediaType {
    Video,
    Image,
    Executable,
    Text,
    Unknown,
}

#[inline]
fn get_drive_io_base_and_head(drive: u8) -> (u16, u8) {
    match drive {
        0 => (ATA_PRIMARY_IO_BASE, 0xE0),
        1 => (ATA_PRIMARY_IO_BASE, 0xF0),
        2 => (ATA_SECONDARY_IO_BASE, 0xE0),
        3 => (ATA_SECONDARY_IO_BASE, 0xF0),
        _ => (ATA_PRIMARY_IO_BASE, 0xE0),
    }
}

#[derive(Clone, Debug)]
pub struct DiskFileInfo {
    pub name: String,
    pub size: u32,
    pub first_cluster: u32,
    pub dir_entry_lba: u32,
    pub dir_entry_offset: usize,
    pub media_type: MediaType,
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags)); }
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags)); }
}

#[inline]
unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe { asm!("in ax, dx", in("dx") port, out("ax") val, options(nomem, nostack, preserves_flags)); }
    val
}

#[inline]
unsafe fn outw(port: u16, val: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("al") val as u8, options(nomem, nostack, preserves_flags)); }
}

#[inline]
unsafe fn ata_io_wait(io_base: u16) {
    unsafe {
        let _ = inb(io_base + ATA_REG_STATUS);
        let _ = inb(io_base + ATA_REG_STATUS);
        let _ = inb(io_base + ATA_REG_STATUS);
        let _ = inb(io_base + ATA_REG_STATUS);
    }
}

fn poll_drive_ready(io_base: u16, expect_drq: bool) -> Result<(), &'static str> {
    for _ in 0..25_000 {
        let status = unsafe { inb(io_base + ATA_REG_STATUS) };
        if status == 0xFF { return Err("ATA Floating Bus"); }
        if (status & STATUS_BSY) == 0 {
            if (status & (STATUS_ERR | STATUS_DF)) != 0 { return Err("ATA Error"); }
            if expect_drq {
                if (status & STATUS_DRQ) != 0 { return Ok(()); }
            } else if (status & STATUS_DRDY) != 0 {
                return Ok(());
            }
        }
        core::hint::spin_loop();
    }
    Err("ATA Timeout")
}

pub fn read_sectors_drive(drive: u8, lba: u32, count: u8, buffer: &mut [u8]) -> Result<(), &'static str> {
    if lba >= 0x1000_0000 { return Err("LBA out of range"); }
    if buffer.len() < (count as usize) * 512 { return Err("Buffer too small"); }
    let (io_base, drive_head) = get_drive_io_base_and_head(drive);

    unsafe {
        outb(io_base + ATA_REG_DRIVE, drive_head | (((lba >> 24) & 0x0F) as u8));
        ata_io_wait(io_base);
        poll_drive_ready(io_base, false)?;

        outb(io_base + ATA_REG_SECTOR_CNT, count);
        outb(io_base + ATA_REG_LBA_LO, (lba & 0xFF) as u8);
        outb(io_base + ATA_REG_LBA_MID, ((lba >> 8) & 0xFF) as u8);
        outb(io_base + ATA_REG_LBA_HI, ((lba >> 16) & 0xFF) as u8);
        outb(io_base + ATA_REG_COMMAND, ATA_CMD_READ_SECTORS);

        for sec in 0..count as usize {
            ata_io_wait(io_base);
            poll_drive_ready(io_base, true)?;

            let sec_offset = sec * 512;
            for i in 0..256 {
                let word = inw(io_base + ATA_REG_DATA);
                buffer[sec_offset + i * 2] = (word & 0xFF) as u8;
                buffer[sec_offset + i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
            }
        }
    }
    Ok(())
}

pub fn read_sector_drive(drive: u8, lba: u32, buffer: &mut [u8; 512]) -> Result<(), &'static str> {
    read_sectors_drive(drive, lba, 1, buffer)
}

pub fn write_sector_drive(drive: u8, lba: u32, buffer: &[u8; 512]) -> Result<(), &'static str> {
    if lba >= 0x1000_0000 { return Err("LBA out of range"); }
    let (io_base, drive_head) = get_drive_io_base_and_head(drive);

    unsafe {
        outb(io_base + ATA_REG_DRIVE, drive_head | (((lba >> 24) & 0x0F) as u8));
        ata_io_wait(io_base);
        poll_drive_ready(io_base, false)?;

        outb(io_base + ATA_REG_SECTOR_CNT, 1);
        outb(io_base + ATA_REG_LBA_LO, (lba & 0xFF) as u8);
        outb(io_base + ATA_REG_LBA_MID, ((lba >> 8) & 0xFF) as u8);
        outb(io_base + ATA_REG_LBA_HI, ((lba >> 16) & 0xFF) as u8);
        outb(io_base + ATA_REG_COMMAND, ATA_CMD_WRITE_SECTORS);
        ata_io_wait(io_base);
        poll_drive_ready(io_base, true)?;

        for i in 0..256 {
            let word = (buffer[i * 2] as u16) | ((buffer[i * 2 + 1] as u16) << 8);
            outw(io_base + ATA_REG_DATA, word);
        }

        outb(io_base + ATA_REG_COMMAND, ATA_CMD_CACHE_FLUSH);
        ata_io_wait(io_base);
        poll_drive_ready(io_base, false)?;
    }
    Ok(())
}

pub fn save_system_config(theme_idx: u8, wallpaper_name: &str) -> Result<(), &'static str> {
    let mut sector = [0u8; 512];
    sector[0] = 0xAA;
    sector[1] = 0x55;
    sector[2] = theme_idx;
    
    let bytes = wallpaper_name.as_bytes();
    let len = core::cmp::min(bytes.len(), 250);
    sector[3] = len as u8;
    sector[4..4 + len].copy_from_slice(&bytes[..len]);

    let res = write_sector_drive(2, PERSISTENT_CONFIG_SECTOR, &sector);
    if res.is_ok() {
        log_info!("CONFIG", "Configuration written to Persistent Sector. Theme={}, Wall='{}'", theme_idx, wallpaper_name);
        
        let mut cfg_content = String::new();
        cfg_content.push_str("THEME=");
        cfg_content.push((b'0' + theme_idx) as char);
        cfg_content.push_str("\nWALLPAPER=");
        cfg_content.push_str(wallpaper_name);
        cfg_content.push('\n');
        let _ = write_file_content(1, "eos.cfg", cfg_content.as_bytes());
    } else {
        log_error!("CONFIG", "Failed to write persistent configuration sector!");
    }
    res
}

pub fn load_system_config() -> Option<(u8, String)> {
    let mut sector = [0u8; 512];
    if read_sector_drive(2, PERSISTENT_CONFIG_SECTOR, &mut sector).is_ok() {
        if sector[0] == 0xAA && sector[1] == 0x55 {
            let theme = sector[2];
            let len = sector[3] as usize;
            if len > 0 && len <= 250 {
                if let Ok(name) = core::str::from_utf8(&sector[4..4 + len]) {
                    log_info!("CONFIG", "Restored Persistent Config: Theme={}, Wall='{}'", theme, name);
                    return Some((theme, String::from(name)));
                }
            }
            log_info!("CONFIG", "Restored Persistent Config: Theme={}, Wall=(None)", theme);
            return Some((theme, String::new()));
        }
    }
    None
}

#[derive(Clone, Copy, Debug)]
pub enum FatType {
    Fat12,
    Fat16,
    Fat32,
}

pub struct FatLayout {
    pub fat_type: FatType,
    pub spc: u32,
    pub fat_start_lba: u32,
    pub data_start_lba: u32,
    pub root_dir_lba: u32,
    pub root_dir_sectors: u32,
}

pub fn get_fat_layout(drive: u8) -> Result<FatLayout, &'static str> {
    let mut sector = [0u8; 512];
    read_sector_drive(drive, 0, &mut sector)?;

    let mut start_lba = 0u32;
    if sector[510] == 0x55 && sector[511] == 0xAA {
        let part_type = sector[446 + 4];
        if part_type != 0 && (sector[446] == 0x80 || sector[446] == 0x00) {
            let p_lba = u32::from_le_bytes([sector[446 + 8], sector[446 + 9], sector[446 + 10], sector[446 + 11]]);
            if p_lba > 0 && p_lba < 0x1000_0000 {
                start_lba = p_lba;
                read_sector_drive(drive, start_lba, &mut sector)?;
            }
        }
    }

    let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]) as u32;
    if bytes_per_sector != 512 { return Err("Unsupported sector size"); }

    let spc = sector[13] as u32;
    if spc == 0 { return Err("Invalid SPC"); }

    let reserved_sectors = u16::from_le_bytes([sector[14], sector[15]]) as u32;
    let num_fats = sector[16] as u32;
    let root_entries = u16::from_le_bytes([sector[17], sector[18]]) as u32;

    let mut total_sectors = u16::from_le_bytes([sector[19], sector[20]]) as u32;
    if total_sectors == 0 {
        total_sectors = u32::from_le_bytes([sector[32], sector[33], sector[34], sector[35]]);
    }

    let mut fat_size = u16::from_le_bytes([sector[22], sector[23]]) as u32;
    let is_fat32 = fat_size == 0 && root_entries == 0;

    let (_fat_sz, root_dir_lba, root_dir_sectors, data_start_lba, fat_type) = if is_fat32 {
        let fat_sz32 = u32::from_le_bytes([sector[36], sector[37], sector[38], sector[39]]);
        let root_cluster = u32::from_le_bytes([sector[44], sector[45], sector[46], sector[47]]);
        let data_start = start_lba + reserved_sectors + (num_fats * fat_sz32);
        let cluster_offset = (root_cluster.saturating_sub(2)).saturating_mul(spc);
        let root_lba = data_start + cluster_offset;
        (fat_sz32, root_lba, spc, data_start, FatType::Fat32)
    } else {
        if fat_size == 0 {
            fat_size = u32::from_le_bytes([sector[36], sector[37], sector[38], sector[39]]);
        }
        let root_sectors = ((root_entries * 32) + 511) / 512;
        let root_lba = start_lba + reserved_sectors + (num_fats * fat_size);
        let data_start = root_lba + root_sectors;
        let data_sectors = total_sectors.saturating_sub(reserved_sectors + (num_fats * fat_size) + root_sectors);
        let total_clusters = data_sectors / spc;
        let f_type = if total_clusters < 4085 { FatType::Fat12 } else { FatType::Fat16 };
        (fat_size, root_lba, root_sectors, data_start, f_type)
    };

    let fat_start_lba = start_lba + reserved_sectors;

    Ok(FatLayout {
        fat_type,
        spc,
        fat_start_lba,
        data_start_lba,
        root_dir_lba,
        root_dir_sectors,
    })
}

static mut FAT_CACHE_SECTOR: u32 = 0xFFFF_FFFF;
static mut FAT_CACHE_DRIVE: u8 = 0xFF;
static mut FAT_CACHE_DATA: [u8; 512] = [0; 512];

pub fn get_next_cluster(drive: u8, layout: &FatLayout, cluster: u32) -> Result<u32, &'static str> {
    match layout.fat_type {
        FatType::Fat32 => {
            let fat_offset = cluster * 4;
            let fat_sector_lba = layout.fat_start_lba + (fat_offset / 512);
            let entry_offset = (fat_offset % 512) as usize;

            unsafe {
                let cache_ptr = core::ptr::addr_of_mut!(FAT_CACHE_DATA);
                if FAT_CACHE_DRIVE != drive || FAT_CACHE_SECTOR != fat_sector_lba {
                    read_sector_drive(drive, fat_sector_lba, &mut *cache_ptr)?;
                    FAT_CACHE_SECTOR = fat_sector_lba;
                    FAT_CACHE_DRIVE = drive;
                }
                let next_c = u32::from_le_bytes([
                    (*cache_ptr)[entry_offset],
                    (*cache_ptr)[entry_offset + 1],
                    (*cache_ptr)[entry_offset + 2],
                    (*cache_ptr)[entry_offset + 3],
                ]) & 0x0FFF_FFFF;
                Ok(next_c)
            }
        }
        FatType::Fat16 => {
            let fat_offset = cluster * 2;
            let fat_sector_lba = layout.fat_start_lba + (fat_offset / 512);
            let entry_offset = (fat_offset % 512) as usize;
            let mut buf = [0u8; 512];
            read_sector_drive(drive, fat_sector_lba, &mut buf)?;
            Ok(u16::from_le_bytes([buf[entry_offset], buf[entry_offset + 1]]) as u32)
        }
        FatType::Fat12 => {
            let fat_offset = cluster + (cluster / 2);
            let fat_sector_lba = layout.fat_start_lba + (fat_offset / 512);
            let entry_offset = (fat_offset % 512) as usize;
            let mut buf = [0u8; 512];
            read_sector_drive(drive, fat_sector_lba, &mut buf)?;
            let val = if entry_offset == 511 {
                let low = buf[511] as u16;
                let mut next_buf = [0u8; 512];
                read_sector_drive(drive, fat_sector_lba + 1, &mut next_buf)?;
                low | ((next_buf[0] as u16) << 8)
            } else {
                u16::from_le_bytes([buf[entry_offset], buf[entry_offset + 1]])
            };
            let next_c = if (cluster & 1) != 0 { val >> 4 } else { val & 0x0FFF } as u32;
            Ok(next_c)
        }
    }
}

pub fn read_entire_file(drive: u8, file_info: &DiskFileInfo) -> Result<Vec<u8>, &'static str> {
    let layout = get_fat_layout(drive)?;
    let mut data = Vec::with_capacity(file_info.size as usize);
    let mut current_cluster = file_info.first_cluster;
    let mut remaining = file_info.size as usize;
    let cluster_bytes = (layout.spc as usize) * 512;
    let mut cluster_buf = alloc::vec![0u8; cluster_bytes];

    let end_marker = match layout.fat_type {
        FatType::Fat12 => 0x0FF8,
        FatType::Fat16 => 0xFFF8,
        FatType::Fat32 => 0x0FFF_FFF8,
    };

    while current_cluster >= 2 && current_cluster < end_marker && remaining > 0 {
        let cluster_offset = (current_cluster.saturating_sub(2)).saturating_mul(layout.spc);
        let cluster_lba = layout.data_start_lba + cluster_offset;

        read_sectors_drive(drive, cluster_lba, layout.spc as u8, &mut cluster_buf)?;
        let to_copy = core::cmp::min(remaining, cluster_bytes);
        data.extend_from_slice(&cluster_buf[..to_copy]);
        remaining -= to_copy;

        current_cluster = get_next_cluster(drive, &layout, current_cluster)?;
    }

    Ok(data)
}

pub fn detect_media_type(drive: u8, first_cluster: u32) -> MediaType {
    if first_cluster < 2 { return MediaType::Unknown; }
    if let Ok(layout) = get_fat_layout(drive) {
        let cluster_offset = (first_cluster.saturating_sub(2)).saturating_mul(layout.spc);
        let cluster_lba = layout.data_start_lba + cluster_offset;
        let mut buf = [0u8; 512];
        if read_sector_drive(drive, cluster_lba, &mut buf).is_ok() {
            if buf.len() >= 4 && &buf[0..4] == &[0x1A, 0x45, 0xDF, 0xA3] {
                return MediaType::Video;
            }
            if buf.len() >= 8 && &buf[4..8] == b"ftyp" {
                return MediaType::Video;
            }
            if (buf[0..4] == [0, 0, 0, 1] && (buf[4] & 0x1F) <= 23) 
                || (buf[0..3] == [0, 0, 1] && (buf[3] & 0x1F) <= 23) {
                return MediaType::Video;
            }
            if &buf[0..4] == b"DKIF" {
                return MediaType::Video;
            }
            if &buf[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] || (&buf[0..3] == &[0xFF, 0xD8, 0xFF]) {
                return MediaType::Image;
            }
            if &buf[0..4] == &[0x7F, b'E', b'L', b'F'] {
                return MediaType::Executable;
            }
        }
    }
    MediaType::Unknown
}

pub fn scan_shared_disk() -> Result<Vec<DiskFileInfo>, &'static str> {
    let layout = get_fat_layout(1)?;
    let mut sector = [0u8; 512];
    let mut files = Vec::new();
    let mut lfn_parts: Vec<(u8, Vec<u16>)> = Vec::new();

    for s in 0..layout.root_dir_sectors {
        let dir_lba = layout.root_dir_lba + s;
        if read_sector_drive(1, dir_lba, &mut sector).is_err() {
            break;
        }
        for entry_idx in 0..16 {
            let offset = entry_idx * 32;
            let first_byte = sector[offset];
            if first_byte == 0x00 { break; }
            if first_byte == 0xE5 {
                lfn_parts.clear();
                continue;
            }

            let attr = sector[offset + 11];

            if attr == 0x0F {
                let order = sector[offset] & 0x3F;
                let mut chunk = [0u16; 13];
                for i in 0..5 {
                    chunk[i] = u16::from_le_bytes([sector[offset + 1 + i*2], sector[offset + 2 + i*2]]);
                }
                for i in 0..6 {
                    chunk[5 + i] = u16::from_le_bytes([sector[offset + 14 + i*2], sector[offset + 15 + i*2]]);
                }
                for i in 0..2 {
                    chunk[11 + i] = u16::from_le_bytes([sector[offset + 28 + i*2], sector[offset + 29 + i*2]]);
                }

                let mut valid_chunk = Vec::new();
                for &ch in &chunk {
                    if ch == 0x0000 || ch == 0xFFFF { break; }
                    valid_chunk.push(ch);
                }
                lfn_parts.push((order, valid_chunk));
                continue;
            }

            if (attr & 0x08) != 0 {
                lfn_parts.clear();
                continue;
            }

            let filename = if !lfn_parts.is_empty() {
                lfn_parts.sort_by_key(|&(order, _)| order);
                let mut full_utf16 = Vec::new();
                for (_, part) in &lfn_parts {
                    full_utf16.extend_from_slice(part);
                }
                lfn_parts.clear();
                String::from_utf16_lossy(&full_utf16)
            } else {
                let name_raw = &sector[offset..offset + 8];
                let ext_raw = &sector[offset + 8..offset + 11];
                let mut name = String::new();
                for &b in name_raw { if b != b' ' { name.push(b as char); } }
                let mut ext = String::new();
                for &b in ext_raw { if b != b' ' { ext.push(b as char); } }
                if !ext.is_empty() {
                    name.push('.');
                    name.push_str(&ext);
                }
                name
            };

            let first_cluster_low = u16::from_le_bytes([sector[offset + 26], sector[offset + 27]]) as u32;
            let first_cluster_high = match layout.fat_type {
                FatType::Fat32 => u16::from_le_bytes([sector[offset + 20], sector[offset + 21]]) as u32,
                _ => 0,
            };
            let first_cluster = (first_cluster_high << 16) | first_cluster_low;
            let file_size = u32::from_le_bytes([
                sector[offset + 28],
                sector[offset + 29],
                sector[offset + 30],
                sector[offset + 31],
            ]);

            let mut detected_type = detect_media_type(1, first_cluster);
            let lower = filename.to_ascii_lowercase();
            if detected_type == MediaType::Unknown {
                if lower.ends_with(".mkv") || lower.ends_with(".mp4") || lower.ends_with(".h264") || lower.ends_with(".264") {
                    detected_type = MediaType::Video;
                } else if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                    detected_type = MediaType::Image;
                } else if lower.ends_with(".elf") {
                    detected_type = MediaType::Executable;
                } else {
                    detected_type = MediaType::Text;
                }
            }

            files.push(DiskFileInfo {
                name: filename,
                size: file_size,
                first_cluster,
                dir_entry_lba: dir_lba,
                dir_entry_offset: offset,
                media_type: detected_type,
            });
        }
    }

    Ok(files)
}

pub fn write_file_content(drive: u8, filename: &str, content: &[u8]) -> Result<(), &'static str> {
    let layout = get_fat_layout(drive)?;
    let files = scan_shared_disk()?;
    let target = files.iter().find(|f| f.name.eq_ignore_ascii_case(filename))
        .ok_or("File not found on drive")?;

    let cluster_offset = (target.first_cluster.saturating_sub(2)).saturating_mul(layout.spc);
    let cluster_lba = layout.data_start_lba + cluster_offset;

    let mut sector_buf = [0u8; 512];
    let to_write = core::cmp::min(content.len(), 512);
    sector_buf[..to_write].copy_from_slice(&content[..to_write]);
    write_sector_drive(drive, cluster_lba, &sector_buf)?;

    let mut dir_buf = [0u8; 512];
    read_sector_drive(drive, target.dir_entry_lba, &mut dir_buf)?;
    let size_bytes = (content.len() as u32).to_le_bytes();
    dir_buf[target.dir_entry_offset + 28..target.dir_entry_offset + 32].copy_from_slice(&size_bytes);
    write_sector_drive(drive, target.dir_entry_lba, &dir_buf)?;

    log_info!("FS", "File '{}' updated on Disk {} ({} bytes written).", filename, drive, content.len());
    Ok(())
}

pub fn init() {
    log_info!("ATA", "Primary & Secondary IDE Channels initialized (4 Drives Active).");
}
