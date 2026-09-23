#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod mem;
mod qoi;
pub mod syscall;

use alloc::vec;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;
use syscall::{
    print_num, print_str, sys_blit_image_ptr, sys_close, sys_exit,
    sys_lseek, sys_mmap, sys_open, sys_read, sys_clock_gettime, TimeSpec,
    SEEK_END, SEEK_SET,
};
use zune_jpeg::JpegDecoder;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

fn init_heap() {
    let heap_size = 48 * 1024 * 1024;
    let heap_start = sys_mmap(heap_size);

    if heap_start.is_null() {
        print_str("\n[CRITICAL ERROR] Failed to mmap heap memory!\n");
        sys_exit(1);
    }

    unsafe {
        ALLOCATOR.lock().init(heap_start, heap_size);
    }
}

#[alloc_error_handler]
fn alloc_error_handler(_layout: Layout) -> ! {
    print_str("\n[CRITICAL ERROR] Userspace Out-Of-Memory!\n");
    sys_exit(137);
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "xor rbp, rbp",
        "mov rsp, 0x203FE000",
        "and rsp, -16",
        "sub rsp, 8",
        "call {entry}",
        "mov rdi, 0",
        "mov rax, 1",
        "syscall",
        "2: hlt",
        "jmp 2b",
        entry = sym app_main,
    );
}

#[inline(always)]
fn get_time_ms() -> u64 {
    let mut ts = TimeSpec { tv_sec: 0, tv_nsec: 0 };
    sys_clock_gettime(1, &mut ts);
    (ts.tv_sec * 1000) + (ts.tv_nsec / 1_000_000)
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

fn try_render_jpeg(data: &[u8]) -> Result<(), &'static str> {
    let t_start = get_time_ms();
    let mut decoder = JpegDecoder::new(data);
    let pixels_u8 = decoder.decode().map_err(|_| "Failed to decode JPEG")?;
    let info = decoder.info().ok_or("Failed to fetch JPEG metadata")?;
    let t_decode = get_time_ms();

    let width = info.width as usize;
    let height = info.height as usize;

    print_str("[Viewer] JPEG Dimensions: ");
    print_num(width);
    print_str("x");
    print_num(height);
    print_str(" (Decode: ");
    print_num((t_decode - t_start) as usize);
    print_str(" ms)\n");

    let step = if width > 600 || height > 400 { 2 } else { 1 };
    let out_w = width / step;
    let out_h = height / step;
    let total_pixels = out_w * out_h;

    let mut pixels = alloc::vec![0u32; total_pixels];
    let mut out_idx = 0usize;

    for y in (0..height).step_by(step) {
        if out_idx >= total_pixels { break; }
        let row_start = y * width * 3;
        for x in (0..width).step_by(step) {
            if out_idx >= total_pixels { break; }
            let off = row_start + (x * 3);
            if off + 2 < pixels_u8.len() {
                let r = pixels_u8[off] as u32;
                let g = pixels_u8[off + 1] as u32;
                let b = pixels_u8[off + 2] as u32;
                pixels[out_idx] = (0xFF << 24) | (r << 16) | (g << 8) | b;
                out_idx += 1;
            }
        }
    }

    sys_blit_image_ptr(pixels.as_ptr(), 0, 0, out_w, out_h);
    Ok(())
}

fn try_render_png(data: &[u8]) -> Result<(), &'static str> {
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as usize;
    let color_type = data[25];

    let bpp = match color_type {
        2 => 3,
        6 => 4,
        _ => return Err("Unsupported color type"),
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
        .map_err(|_| "Decompression failed")?;

    let stride = width * bpp;
    let line_stride = stride + 1;

    if decompressed.len() < line_stride * height {
        return Err("Truncated stream");
    }

    let step = if width > 600 || height > 400 { 2 } else { 1 };
    let out_w = width / step;
    let out_h = height / step;
    let total_pixels = out_w * out_h;

    let mut pixels = alloc::vec![0u32; total_pixels];
    let mut out_idx = 0usize;

    let mut prev_row = vec![0u8; stride];
    let mut curr_row = vec![0u8; stride];

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
            _ => return Err("Unknown filter"),
        }

        if y % step == 0 && out_idx < total_pixels {
            for x in (0..width).step_by(step) {
                if out_idx >= total_pixels { break; }
                let off = x * bpp;
                let r = curr_row[off] as u32;
                let g = curr_row[off + 1] as u32;
                let b = curr_row[off + 2] as u32;
                let a = if bpp == 4 { curr_row[off + 3] as u32 } else { 255u32 };
                pixels[out_idx] = (a << 24) | (r << 16) | (g << 8) | b;
                out_idx += 1;
            }
        }

        prev_row.copy_from_slice(&curr_row);
    }

    sys_blit_image_ptr(pixels.as_ptr(), 0, 0, out_w, out_h);
    Ok(())
}

fn try_render_image(path: &str) -> Result<(), &'static str> {
    let t0 = get_time_ms();
    let fd = sys_open(path);
    if fd < 0 {
        return Err("File not found");
    }

    let file_size = sys_lseek(fd as u64, 0, SEEK_END);
    sys_lseek(fd as u64, 0, SEEK_SET);

    if file_size <= 0 {
        sys_close(fd as u64);
        return Err("Empty or invalid file");
    }

    let mut data = alloc::vec![0u8; file_size as usize];
    sys_read(fd as u64, &mut data);
    sys_close(fd as u64);

    let t_io = get_time_ms();
    print_str("[Benchmark] File Read via VFS: ");
    print_num((t_io - t0) as usize);
    print_str(" ms (");
    print_num(file_size as usize);
    print_str(" bytes)\n");

    // الكشف التلقائي عن نوع الصورة بواسطة الـ Magic Signature
    if data.len() >= 8 && &data[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        print_str("[Viewer] Detected Format: PNG\n");
        try_render_png(&data)
    } else if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        print_str("[Viewer] Detected Format: JPEG / JPG\n");
        try_render_jpeg(&data)
    } else {
        Err("Unsupported image signature (only PNG and JPEG are supported)")
    }
}

fn get_passed_argument() -> &'static str {
    let ipc_ptr = 0x0000_0000_2050_0000 as *const u8;
    unsafe {
        let mut len = 0usize;
        while len < 255 && *ipc_ptr.add(len) != 0 {
            len += 1;
        }
        if len > 0 {
            let slice = core::slice::from_raw_parts(ipc_ptr, len);
            if let Ok(s) = core::str::from_utf8(slice) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return trimmed;
                }
            }
        }
    }
    "icon.png"
}

extern "C" fn app_main() {
    init_heap();

    let target_file = get_passed_argument();
    match try_render_image(target_file) {
        Ok(()) => print_str("[Viewer] Image Rendered Successfully in Overlay.\n"),
        Err(err) => {
            print_str("[Viewer] Error: ");
            print_str(err);
            print_str("\n");
        }
    }

    sys_exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print_str("\n[PANIC] User app aborted.\n");
    sys_exit(1);
}
