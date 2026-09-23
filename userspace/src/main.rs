#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

mod mem;
pub mod png;
mod qoi;
pub mod syscall;

use alloc::vec::Vec;
use core::alloc::Layout;
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;
use png::decode_png;
use syscall::{print_num, print_str, sys_blit_image_ptr, sys_clear_screen, sys_exit, sys_read_file, sys_sleep};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

// توسيع الذاكرة لـ 40MB للتعامل مع صور الـ 4K ولقطات الشاشة الضخمة
const HEAP_SIZE: usize = 40 * 1024 * 1024;
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
        "mov rsp, 0x203FE000",
        "and rsp, -16",
        "sub rsp, 8",
        "call {entry}",
        "mov rdi, 0",
        "mov rax, 60",
        "syscall",
        "2: hlt",
        "jmp 2b",
        entry = sym app_main,
    );
}

// 💡 مخزن 10MB لقراءة الملفات الضخمة من القرص
const FILE_BUF_SIZE: usize = 10 * 1024 * 1024;
static mut FILE_BUF: [u8; FILE_BUF_SIZE] = [0; FILE_BUF_SIZE];

fn try_render_png(path: &str, pos_x: usize, pos_y: usize) -> Result<(), &'static str> {
    unsafe {
        let fbuf = &mut *core::ptr::addr_of_mut!(FILE_BUF);
        let res = sys_read_file(path, fbuf);

        if res <= 0 {
            return Err("File not found or read error\n");
        }

        let file_size = res as usize;
        print_str("[Viewer] File loaded (");
        print_num(file_size);
        print_str(" bytes). Parsing PNG...\n");

        let image = decode_png(&fbuf[..file_size])?;

        print_str("[Viewer] Blitting decoded image to screen...\n");
        sys_clear_screen();
        sys_blit_image_ptr(image.pixels.as_ptr(), pos_x, pos_y, image.width, image.height);
        Ok(())
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
                return s.trim();
            }
        }
    }
    "icon.png"
}

extern "C" fn app_main() {
    init_heap();

    let target_file = get_passed_argument();

    print_str("\n[Viewer] Opening: ");
    print_str(target_file);
    print_str("\n");

    match try_render_png(target_file, 80, 40) {
        Ok(()) => {
            print_str("[Viewer] Image displayed successfully!\n");
        }
        Err(err) => {
            print_str("[Viewer] Render Error: ");
            print_str(err);
        }
    }

    print_str("[Viewer] Displaying image for 5s...\n");
    sys_sleep(5000);
    sys_clear_screen();
    sys_exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print_str("\n[PANIC] User app aborted.\n");
    sys_exit(1);
}
