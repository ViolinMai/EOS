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
    pub name: String, pub width: usize, pub height: usize,
    pub pixels: Vec<u32>, pub last_used_tick: u64,
}

pub struct ImageCacheManager {
    pub entries: Vec<CachedImage>, pub current_bytes: usize,
}

impl ImageCacheManager {
    pub const fn new() -> Self { Self { entries: Vec::new(), current_bytes: 0 } }
    pub fn insert(&mut self, name: &str, width: usize, height: usize, pixels: Vec<u32>, tick: u64) {
        let size = pixels.len() * 4;
        while self.current_bytes + size > MAX_CACHE_BYTES && !self.entries.is_empty() {
            let mut o_idx = 0; let mut o_tick = u64::MAX;
            for (i, e) in self.entries.iter().enumerate() { if e.last_used_tick < o_tick { o_tick = e.last_used_tick; o_idx = i; } }
            let rem = self.entries.remove(o_idx); self.current_bytes -= rem.pixels.len() * 4;
        }
        self.current_bytes += size;
        self.entries.push(CachedImage { name: String::from(name), width, height, pixels, last_used_tick: tick });
    }
    pub fn get(&mut self, name: &str, tick: u64) -> Option<(usize, usize, &[u32])> {
        for e in self.entries.iter_mut() {
            if e.name == name { e.last_used_tick = tick; return Some((e.width, e.height, &e.pixels)); }
        }
        None
    }
}

pub static mut CACHE_MANAGER: ImageCacheManager = ImageCacheManager::new();

#[inline(always)]
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs(); let pb = (p - b as i32).abs(); let pc = (p - c as i32).abs();
    if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }
}

pub fn decode_image_to_raw(data: &[u8]) -> Result<(Vec<u32>, usize, usize), &'static str> {
    if data.len() >= 8 && &data[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] { decode_png_raw(data) }
    else if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF { decode_jpeg_raw(data) }
    else { Err("Unsupported image format") }
}

fn decode_jpeg_raw(data: &[u8]) -> Result<(Vec<u32>, usize, usize), &'static str> {
    let mut decoder = JpegDecoder::new(data);
    let px = decoder.decode().map_err(|_| "JPEG decode failed")?;
    let info = decoder.info().ok_or("Invalid JPEG info")?;
    let w = info.width as usize; let h = info.height as usize;
    let mut out = alloc::vec![0u32; w * h];
    for i in 0..(w * h) {
        let o = i * 3;
        if o + 2 < px.len() { out[i] = (0xFF << 24) | ((px[o] as u32) << 16) | ((px[o+1] as u32) << 8) | (px[o+2] as u32); }
    }
    Ok((out, w, h))
}

fn decode_png_raw(data: &[u8]) -> Result<(Vec<u32>, usize, usize), &'static str> {
    if data.len() < 33 { return Err("PNG too short"); }
    let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as usize;
    let bpp = match data[25] { 2 => 3, 6 => 4, _ => return Err("Unsupported color type") };
    let mut idat = Vec::new(); let mut off = 33;
    while off + 8 <= data.len() {
        let len = u32::from_be_bytes([data[off], data[off+1], data[off+2], data[off+3]]) as usize;
        let ctype = &data[off+4..off+8];
        let doff = off + 8;
        if doff + len > data.len() { break; }
        if ctype == b"IDAT" { idat.extend_from_slice(&data[doff..doff+len]); }
        else if ctype == b"IEND" { break; }
        off = doff + len + 4;
    }
    let decomp = miniz_oxide::inflate::decompress_to_vec_zlib(&idat).map_err(|_| "Zlib error")?;
    let strd = w * bpp; let mut out = alloc::vec![0u32; w * h];
    let mut prev = vec![0u8; strd]; let mut curr = vec![0u8; strd];
    let mut idx = 0usize;
    for y in 0..h {
        let ls = y * (strd + 1); let f = decomp[ls]; let rs = ls + 1;
        let rl = &decomp[rs..rs + strd];
        match f {
            0 => curr.copy_from_slice(rl),
            1 => for x in 0..strd { curr[x] = rl[x].wrapping_add(if x >= bpp { curr[x-bpp] } else { 0 }); },
            2 => for x in 0..strd { curr[x] = rl[x].wrapping_add(prev[x]); },
            3 => for x in 0..strd { let l = if x >= bpp { curr[x-bpp] } else { 0 }; curr[x] = rl[x].wrapping_add(((l as u16 + prev[x] as u16) >> 1) as u8); },
            4 => for x in 0..strd { let l = if x >= bpp { curr[x-bpp] } else { 0 }; let u = prev[x]; let ul = if x >= bpp { prev[x-bpp] } else { 0 }; curr[x] = rl[x].wrapping_add(paeth(l, u, ul)); },
            _ => return Err("Unknown filter"),
        }
        for x in 0..w {
            let o = x * bpp; let a = if bpp == 4 { curr[o+3] as u32 } else { 255 };
            out[idx] = (a << 24) | ((curr[o] as u32) << 16) | ((curr[o+1] as u32) << 8) | (curr[o+2] as u32);
            idx += 1;
        }
        prev.copy_from_slice(&curr);
    }
    Ok((out, w, h))
}
