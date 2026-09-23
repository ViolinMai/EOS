#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod mem;
pub mod png;
mod qoi;
pub mod syscall;

use alloc::vec;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;
use png::decode_png;
use syscall::{print_num, print_str, sys_blit_image_ptr, sys_exit, sys_read_file, sys_sleep};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

const HEAP_SIZE: usize = 48 * 1024 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

fn init_heap() {
    unsafe {
        let heap_start = core::ptr::addr_of_mut!(HEAP_MEM) as *mut u8;
        ALLOCATOR.lock().init(heap_start, HEAP_SIZE);
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
        "mov rsp, 0x043FF000",
        "call {entry}",
        "mov rdi, 0",
        "mov rax, 60",
        "syscall",
        "2: hlt",
        "jmp 2b",
        entry = sym app_main,
    );
}

const FILE_BUF_SIZE: usize = 4 * 1024 * 1024;
static mut FILE_BUF: [u8; FILE_BUF_SIZE] = [0; FILE_BUF_SIZE];

fn scale_image(src: &[u32], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Vec<u32> {
    let mut out = vec![0u32; dst_w * dst_h];
    for dy in 0..dst_h {
        let sy = (dy * src_h) / dst_h;
        for dx in 0..dst_w {
            let sx = (dx * src_w) / dst_w;
            out[dy * dst_w + dx] = src[sy * src_w + sx];
        }
    }
    out
}

fn try_render_png(filename: &str, pos_x: usize, pos_y: usize) -> Result<(), &'static str> {
    unsafe {
        let fbuf = &mut *core::ptr::addr_of_mut!(FILE_BUF);
        let res = sys_read_file(filename, fbuf);

        if res <= 0 {
            return Err("File not found or read error\n");
        }

        let file_size = res as usize;

        print_str("[User App] PNG loaded (");
        print_num(file_size);
        print_str(" bytes). Parsing PNG structure...\n");

        let image = decode_png(&fbuf[..file_size])?;

        // تحجيم الصورة إلى 320x266 لعرضها بأمان داخل أي دقة شاشة
        let target_w = 320usize;
        let target_h = (image.height * target_w) / image.width;

        print_str("[User App] Scaling image to: ");
        print_num(target_w);
        print_str("x");
        print_num(target_h);
        print_str("...\n");

        let scaled_pixels = scale_image(&image.pixels, image.width, image.height, target_w, target_h);

        print_str("[User App] Blitting to screen at position (");
        print_num(pos_x);
        print_str(", ");
        print_num(pos_y);
        print_str(")...\n");

        sys_blit_image_ptr(scaled_pixels.as_ptr(), pos_x, pos_y, target_w, target_h);
        Ok(())
    }
}

extern "C" fn app_main() {
    init_heap();

    print_str("\n[User App] Desktop Engine Online (48MB Heap Active)\n");

    // رسم الصورة في الإحداثيات (450, 80) لضمان ظهورها داخل الشاشة أياً كانت الدقة
    match try_render_png("icon.png", 450, 80) {
        Ok(()) => {
            print_str("[User App] PNG Icon rendered successfully!\n");
        }
        Err(err) => {
            print_str("[User App] PNG Load Error: ");
            print_str(err);
        }
    }

    print_str("[User App] Session running. Sleeping 4s...\n");
    sys_sleep(4000);

    print_str("[User App] Done.\n");
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print_str("\n[PANIC] User app aborted.\n");
    sys_exit(1);
}
