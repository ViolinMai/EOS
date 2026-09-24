use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;
use crate::arch::x86_64::syscall::{OVERLAY_ACTIVE, OVERLAY_WIDTH, OVERLAY_HEIGHT, OVERLAY_MAX_W, OVERLAY_MAX_H, OVERLAY_PIXELS};
use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;
use zune_jpeg::JpegDecoder;

pub static mut CUSTOM_WALLPAPER: Option<Vec<u32>> = None;
pub static mut CUSTOM_WALLPAPER_DIM: (usize, usize) = (0, 0);

pub const MAX_CACHE_BYTES: usize = 256 * 1024 * 1024;

pub struct CachedImage {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
    pub last_used_tick: u64,
}

pub struct ImageCacheManager {
    pub entries: Vec<CachedImage>,
    pub current_bytes: usize,
}

impl ImageCacheManager {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            current_bytes: 0,
        }
    }

    pub fn evict_if_needed(&mut self, needed_bytes: usize) {
        while self.current_bytes + needed_bytes > MAX_CACHE_BYTES && !self.entries.is_empty() {
            let mut oldest_idx = 0;
            let mut oldest_tick = u64::MAX;

            for (idx, item) in self.entries.iter().enumerate() {
                if item.last_used_tick < oldest_tick {
                    oldest_tick = item.last_used_tick;
                    oldest_idx = idx;
                }
            }

            let removed = self.entries.remove(oldest_idx);
            let freed = removed.pixels.len() * 4;
            self.current_bytes = self.current_bytes.saturating_sub(freed);
        }
    }

    pub fn insert(&mut self, name: &str, width: usize, height: usize, pixels: Vec<u32>, tick: u64) {
        let size_bytes = pixels.len() * 4;
        self.evict_if_needed(size_bytes);
        self.current_bytes += size_bytes;
        self.entries.push(CachedImage {
            name: String::from(name),
            width,
            height,
            pixels,
            last_used_tick: tick,
        });
    }

    pub fn get(&mut self, name: &str, tick: u64) -> Option<(usize, usize, &[u32])> {
        for item in self.entries.iter_mut() {
            if item.name == name {
                item.last_used_tick = tick;
                return Some((item.width, item.height, &item.pixels));
            }
        }
        None
    }
}

pub static mut CACHE_MANAGER: ImageCacheManager = ImageCacheManager::new();

pub fn cleanup_overlay_cache() {
    OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
    OVERLAY_WIDTH.store(0, Ordering::SeqCst);
    OVERLAY_HEIGHT.store(0, Ordering::SeqCst);
}

#[inline(always)]
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let a_i = a as i32;
    let b_i = b as i32;
    let c_i = c as i32;
    let p = a_i + b_i - c_i;
    let pa = (p - a_i).abs();
    let pb = (p - b_i).abs();
    let pc = (p - c_i).abs();
    if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }
}

pub fn decode_image_to_raw(data: &[u8]) -> Result<(Vec<u32>, usize, usize), &'static str> {
    if data.len() >= 8 && &data[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        decode_png_raw(data)
    } else if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        decode_jpeg_raw(data)
    } else {
        Err("Unsupported image format")
    }
}

pub fn decode_and_display_cached(name: &str, data: &[u8], tick: u64) -> Result<(), &'static str> {
    unsafe {
        let mgr = &mut *addr_of_mut!(CACHE_MANAGER);
        if let Some((w, h, pixels)) = mgr.get(name, tick) {
            display_to_overlay(pixels, w, h);
            return Ok(());
        }
    }

    let (pixels, w, h) = decode_image_to_raw(data)?;
    display_to_overlay(&pixels, w, h);

    unsafe {
        let mgr = &mut *addr_of_mut!(CACHE_MANAGER);
        mgr.insert(name, w, h, pixels, tick);
    }
    Ok(())
}

fn display_to_overlay(pixels: &[u32], w: usize, h: usize) {
    let step = if w > OVERLAY_MAX_W || h > OVERLAY_MAX_H {
        let sw = (w + OVERLAY_MAX_W - 1) / OVERLAY_MAX_W;
        let sh = (h + OVERLAY_MAX_H - 1) / OVERLAY_MAX_H;
        core::cmp::max(sw, sh)
    } else {
        1
    };

    let target_w = core::cmp::min(w / step, OVERLAY_MAX_W);
    let target_h = core::cmp::min(h / step, OVERLAY_MAX_H);

    unsafe {
        let dest = addr_of_mut!(OVERLAY_PIXELS) as *mut u32;
        let mut out_idx = 0usize;

        for y in (0..h).step_by(step) {
            if y / step >= target_h { break; }
            for x in (0..w).step_by(step) {
                if x / step >= target_w { break; }
                *dest.add(out_idx) = pixels[y * w + x];
                out_idx += 1;
            }
        }
    }

    OVERLAY_WIDTH.store(target_w, Ordering::SeqCst);
    OVERLAY_HEIGHT.store(target_h, Ordering::SeqCst);
    OVERLAY_ACTIVE.store(true, Ordering::SeqCst);
}

pub fn set_as_wallpaper(data: &[u8]) -> Result<(), &'static str> {
    let (pixels, w, h) = decode_image_to_raw(data)?;
    unsafe {
        *addr_of_mut!(CUSTOM_WALLPAPER) = Some(pixels);
        *addr_of_mut!(CUSTOM_WALLPAPER_DIM) = (w, h);
    }
    Ok(())
}

#[allow(dead_code)]
pub fn set_wallpaper_from_raw(pixels: Vec<u32>, w: usize, h: usize) {
    unsafe {
        *addr_of_mut!(CUSTOM_WALLPAPER) = Some(pixels);
        *addr_of_mut!(CUSTOM_WALLPAPER_DIM) = (w, h);
    }
}

fn decode_jpeg_raw(data: &[u8]) -> Result<(Vec<u32>, usize, usize), &'static str> {
    let mut decoder = JpegDecoder::new(data);
    let pixels_u8 = decoder.decode().map_err(|_| "JPEG decode failed")?;
    let info = decoder.info().ok_or("Invalid JPEG info")?;

    let width = info.width as usize;
    let height = info.height as usize;
    let mut out_pixels = alloc::vec![0u32; width * height];

    for i in 0..(width * height) {
        let off = i * 3;
        if off + 2 < pixels_u8.len() {
            let r = pixels_u8[off] as u32;
            let g = pixels_u8[off + 1] as u32;
            let b = pixels_u8[off + 2] as u32;
            out_pixels[i] = (0xFF << 24) | (r << 16) | (g << 8) | b;
        }
    }
    Ok((out_pixels, width, height))
}

fn decode_png_raw(data: &[u8]) -> Result<(Vec<u32>, usize, usize), &'static str> {
    if data.len() < 33 { return Err("PNG too short"); }

    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as usize;
    let color_type = data[25];

    let bpp = match color_type {
        2 => 3,
        6 => 4,
        _ => return Err("Unsupported PNG color type"),
    };

    let mut idat_data = Vec::new();
    let mut offset = 33;
    while offset + 8 <= data.len() {
        let len = u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
        let ctype = &data[offset + 4..offset + 8];
        let doff = offset + 8;
        if doff + len > data.len() { break; }
        if ctype == b"IDAT" {
            idat_data.extend_from_slice(&data[doff..doff + len]);
        } else if ctype == b"IEND" {
            break;
        }
        offset = doff + len + 4;
    }

    let decompressed = miniz_oxide::inflate::decompress_to_vec_zlib(&idat_data)
        .map_err(|_| "Zlib inflate failed")?;

    let stride = width * bpp;
    let line_stride = stride + 1;
    if decompressed.len() < line_stride * height {
        return Err("Truncated scanlines");
    }

    let mut out_pixels = alloc::vec![0u32; width * height];
    let mut prev_row = vec![0u8; stride];
    let mut curr_row = vec![0u8; stride];
    let mut out_idx = 0usize;

    for y in 0..height {
        let line_start = y * line_stride;
        let filter = decompressed[line_start];
        let data_start = line_start + 1;
        let raw_line = &decompressed[data_start..data_start + stride];

        match filter {
            0 => curr_row.copy_from_slice(raw_line),
            1 => {
                for x in 0..stride {
                    let left = if x >= bpp { curr_row[x - bpp] } else { 0 };
                    curr_row[x] = raw_line[x].wrapping_add(left);
                }
            }
            2 => {
                for x in 0..stride {
                    curr_row[x] = raw_line[x].wrapping_add(prev_row[x]);
                }
            }
            3 => {
                for x in 0..stride {
                    let left = if x >= bpp { curr_row[x - bpp] } else { 0 };
                    let up = prev_row[x];
                    curr_row[x] = raw_line[x].wrapping_add(((left as u16 + up as u16) >> 1) as u8);
                }
            }
            4 => {
                for x in 0..stride {
                    let left = if x >= bpp { curr_row[x - bpp] } else { 0 };
                    let up = prev_row[x];
                    let up_left = if x >= bpp { prev_row[x - bpp] } else { 0 };
                    curr_row[x] = raw_line[x].wrapping_add(paeth(left, up, up_left));
                }
            }
            _ => return Err("Unknown PNG filter"),
        }

        for x in 0..width {
            let off = x * bpp;
            let r = curr_row[off] as u32;
            let g = curr_row[off + 1] as u32;
            let b = curr_row[off + 2] as u32;
            let a = if bpp == 4 { curr_row[off + 3] as u32 } else { 255u32 };
            out_pixels[out_idx] = (a << 24) | (r << 16) | (g << 8) | b;
            out_idx += 1;
        }

        prev_row.copy_from_slice(&curr_row);
    }

    Ok((out_pixels, width, height))
}
