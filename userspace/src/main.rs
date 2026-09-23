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
    print_num, print_str, sys_blit_image_ptr, sys_clear_screen, sys_close, sys_exit,
    sys_lseek, sys_mmap, sys_open, sys_read, sys_sleep, sys_clock_gettime, TimeSpec,
    SEEK_END, SEEK_SET,
};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

fn init_heap() {
    // تخصيص 64MB لتغطية الصورة بدقتها الكاملة ومخازن فك الضغط
    let heap_size = 64 * 1024 * 1024;
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

fn try_render_png_full_res(path: &str, pos_x: usize, pos_y: usize) -> Result<(), &'static str> {
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

    if data.len() < 33 || &data[0..8] != &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Err("Invalid PNG signature");
    }

    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as usize;
    let color_type = data[25];

    let bpp = match color_type {
        2 => 3,
        6 => 4,
        _ => return Err("Unsupported color type"),
    };

    print_str("[Viewer] Image Resolution: ");
    print_num(width);
    print_str("x");
    print_num(height);
    print_str(" (Full Native Resolution, No Downsampling)\n");

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

    let t_extract = get_time_ms();
    let decompressed = miniz_oxide::inflate::decompress_to_vec_zlib(&idat_data)
        .map_err(|_| "Decompression failed")?;
    let t_inflate = get_time_ms();

    print_str("[Benchmark] Deflate Decompression: ");
    print_num((t_inflate - t_extract) as usize);
    print_str(" ms (Decompressed: ");
    print_num(decompressed.len());
    print_str(" bytes)\n");

    let stride = width * bpp;
    let line_stride = stride + 1;

    if decompressed.len() < line_stride * height {
        return Err("Truncated stream");
    }

    let total_pixels = width * height;
    let mut pixels = alloc::vec![0u32; total_pixels];

    let mut prev_row = vec![0u8; stride];
    let mut curr_row = vec![0u8; stride];

    for y in 0..height {
        let line_start = y * line_stride;
        let filter = decompressed[line_start];
        let data_start = line_start + 1;
        let raw_line = &decompressed[data_start..data_start + stride];

        match filter {
            0 => {
                curr_row.copy_from_slice(raw_line);
            }
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

        let row_out_start = y * width;
        if bpp == 4 {
            for x in 0..width {
                let off = x * 4;
                let r = curr_row[off] as u32;
                let g = curr_row[off + 1] as u32;
                let b = curr_row[off + 2] as u32;
                let a = curr_row[off + 3] as u32;
                pixels[row_out_start + x] = (a << 24) | (r << 16) | (g << 8) | b;
            }
        } else {
            for x in 0..width {
                let off = x * 3;
                let r = curr_row[off] as u32;
                let g = curr_row[off + 1] as u32;
                let b = curr_row[off + 2] as u32;
                pixels[row_out_start + x] = (0xFF << 24) | (r << 16) | (g << 8) | b;
            }
        }

        prev_row.copy_from_slice(&curr_row);
    }

    let t_filter = get_time_ms();
    print_str("[Benchmark] Scanline Unfiltering (Full Res): ");
    print_num((t_filter - t_inflate) as usize);
    print_str(" ms\n");

    sys_clear_screen();
    sys_blit_image_ptr(pixels.as_ptr(), pos_x, pos_y, width, height);

    let t_blit = get_time_ms();
    print_str("[Benchmark] Blit to Screen: ");
    print_num((t_blit - t_filter) as usize);
    print_str(" ms\n");

    Ok(())
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
                return s.trim();
            }
        }
    }
    "icon.png"
}

extern "C" fn app_main() {
    init_heap();

    let target_file = get_passed_argument();
    match try_render_png_full_res(target_file, 10, 10) {
        Ok(()) => print_str("[Viewer] Render Complete.\n"),
        Err(err) => {
            print_str("[Viewer] Error: ");
            print_str(err);
            print_str("\n");
        }
    }

    print_str("[Viewer] Auto-exiting in 3 seconds...\n");
    sys_sleep(3000);
    sys_clear_screen();
    sys_exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print_str("\n[PANIC] User app aborted.\n");
    sys_exit(1);
}
