use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::ata::{read_sector_drive, write_sector_drive};

pub const SETTINGS_DRIVE: u8 = 3;
pub const SETTINGS_MAGIC: [u8; 4] = *b"EOSS";
pub const SETTINGS_VERSION: u16 = 1;
pub const MAX_SETTINGS_SIZE: usize = 32 * 1024;

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SettingsHeader {
    pub magic: [u8; 4],
    pub version: u16,
    pub length: u16,
    pub checksum: u32,
    pub reserved: [u8; 8],
}

fn calc_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

pub fn load_raw_settings() -> Result<Vec<u8>, &'static str> {
    let mut sec0 = [0u8; 512];
    if read_sector_drive(SETTINGS_DRIVE, 0, &mut sec0).is_err() {
        return Err("Drive 3 ATA read error");
    }

    if sec0[0..4] != SETTINGS_MAGIC {
        return Err("Invalid magic on settings drive");
    }

    let version = u16::from_le_bytes([sec0[4], sec0[5]]);
    if version != SETTINGS_VERSION {
        return Err("Unsupported settings version");
    }

    let length = u16::from_le_bytes([sec0[6], sec0[7]]) as usize;
    if length > MAX_SETTINGS_SIZE {
        return Err("Settings size exceeds maximum");
    }

    let stored_csum = u32::from_le_bytes([sec0[8], sec0[9], sec0[10], sec0[11]]);
    let mut data = Vec::with_capacity(length);

    let first_chunk_len = core::cmp::min(length, 512 - 20);
    data.extend_from_slice(&sec0[20..20 + first_chunk_len]);

    let mut remaining = length.saturating_sub(first_chunk_len);
    let mut lba = 1u32;
    let mut sector = [0u8; 512];

    while remaining > 0 {
        if read_sector_drive(SETTINGS_DRIVE, lba, &mut sector).is_err() {
            return Err("Failed reading data sector");
        }
        let to_copy = core::cmp::min(remaining, 512);
        data.extend_from_slice(&sector[..to_copy]);
        remaining -= to_copy;
        lba += 1;
    }

    let csum = calc_crc32(&data);
    if csum != stored_csum {
        return Err("Settings checksum mismatch");
    }

    Ok(data)
}

pub fn save_raw_settings(payload: &[u8]) -> Result<(), &'static str> {
    if payload.len() > MAX_SETTINGS_SIZE {
        return Err("Payload exceeds maximum size");
    }

    crate::log_info!("SETTINGS", "Writing {} bytes to dedicated settings disk (Drive 3)...", payload.len());

    let csum = calc_crc32(payload);
    let mut sec0 = [0u8; 512];
    sec0[0..4].copy_from_slice(&SETTINGS_MAGIC);
    sec0[4..6].copy_from_slice(&SETTINGS_VERSION.to_le_bytes());
    sec0[6..8].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    sec0[8..12].copy_from_slice(&csum.to_le_bytes());

    let first_chunk = core::cmp::min(payload.len(), 512 - 20);
    sec0[20..20 + first_chunk].copy_from_slice(&payload[..first_chunk]);

    if let Err(e) = write_sector_drive(SETTINGS_DRIVE, 0, &sec0) {
        crate::log_error!("SETTINGS", "write_sector_drive LBA 0 failed: {}", e);
        return Err("Failed writing LBA 0");
    }

    let mut remaining = payload.len().saturating_sub(first_chunk);
    let mut offset = first_chunk;
    let mut lba = 1u32;
    let mut sector = [0u8; 512];

    while remaining > 0 {
        sector.fill(0);
        let to_copy = core::cmp::min(remaining, 512);
        sector[..to_copy].copy_from_slice(&payload[offset..offset + to_copy]);
        if let Err(e) = write_sector_drive(SETTINGS_DRIVE, lba, &sector) {
            crate::log_error!("SETTINGS", "write_sector_drive LBA {} failed: {}", lba, e);
            return Err("Failed writing data sector");
        }
        remaining -= to_copy;
        offset += to_copy;
        lba += 1;
    }

    crate::log_info!("SETTINGS", "Data flushed successfully to disk 3.");
    Ok(())
}

pub fn merge_settings(new_content: &[u8]) -> Result<(), &'static str> {
    let mut map: alloc::collections::BTreeMap<String, String> = alloc::collections::BTreeMap::new();

    if let Ok(existing_bytes) = load_raw_settings() {
        if let Ok(existing_str) = core::str::from_utf8(&existing_bytes) {
            for line in existing_str.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let key = k.trim();
                    if !key.is_empty() {
                        map.insert(String::from(key), String::from(v.trim()));
                    }
                }
            }
        }
    }

    if let Ok(new_str) = core::str::from_utf8(new_content) {
        for line in new_str.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim();
                if !key.is_empty() {
                    map.insert(String::from(key), String::from(v.trim()));
                }
            }
        }
    }

    let mut merged = String::new();
    for (k, v) in &map {
        merged.push_str(k);
        merged.push('=');
        merged.push_str(v);
        merged.push('\n');
    }

    save_raw_settings(merged.as_bytes())
}
