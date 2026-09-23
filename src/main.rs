#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod arch;
mod drivers;
mod font;
mod fs;
mod logger;
mod mm;
mod serial;
mod task;
mod writer;

use core::arch::asm;
use core::panic::PanicInfo;
use limine::request::{FramebufferRequest, HhdmRequest, MemmapRequest, ModulesRequest};
use limine::BaseRevision;
use writer::{FrameWriter, WRITER};

#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: () = ();

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(2);

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMMAP_REQUEST: MemmapRequest = MemmapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MODULES_REQUEST: ModulesRequest = ModulesRequest::new();

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: () = ();

extern "C" fn main_shell_entry() {}

extern "C" fn background_worker_one() {
    loop {
        unsafe {
            if let Some(sched) = &mut *core::ptr::addr_of_mut!(task::SCHEDULER) {
                if sched.tasks.len() > 1 {
                    sched.tasks[1].counter += 1;
                }
            }
        }
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }
    }
}

extern "C" fn background_worker_two() {
    loop {
        unsafe {
            if let Some(sched) = &mut *core::ptr::addr_of_mut!(task::SCHEDULER) {
                if sched.tasks.len() > 2 {
                    sched.tasks[2].counter += 1;
                }
            }
        }
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }
    }
}

unsafe fn extract_limine_file_info(file_ref: &limine::file::File) -> (*const u8, usize) {
    let raw_ptr = file_ref as *const _ as *const u8;
    unsafe {
        let addr_val = *(raw_ptr.add(8) as *const usize);
        let size_val = *(raw_ptr.add(16) as *const u64) as usize;
        (addr_val as *const u8, size_val)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    serial::SERIAL1.init();
    serial_println!("\n==========================================");
    serial_println!("[EOS] SERIAL CONSOLE INITIALIZED");

    let mut fb_w = 0usize;
    let mut fb_h = 0usize;

    if let Some(fb_response) = FRAMEBUFFER_REQUEST.response() {
        if let Some(&framebuffer) = fb_response.framebuffers().first() {
            let buffer_ptr = framebuffer.address() as *mut u8;
            fb_w = framebuffer.width as usize;
            fb_h = framebuffer.height as usize;
            let pitch = framebuffer.pitch as usize;
            let _bpp = framebuffer.bpp as usize;

            unsafe {
                let mut w = FrameWriter::new(buffer_ptr, fb_w, fb_h, pitch);
                w.clear(15, 23, 42);
                *core::ptr::addr_of_mut!(WRITER) = Some(w);
            }
        }
    }

    log_info!("EOS KERNEL (x86_64) ONLINE");
    log_info!("Framebuffer Initialized: {}x{} (Pitch: {})", fb_w, fb_h, fb_w * 4);

    // 1. الأساسيات المعمارية
    arch::x86_64::gdt::init();
    log_info!("GDT & TSS loaded with Ring 3 User Segments.");

    arch::x86_64::interrupts::init();
    log_info!("IDT initialized (Timer, Keyboard, Mouse).");

    // 2. متحكم المقاطعات والمؤقت
    unsafe {
        arch::x86_64::pic::remap();
        arch::x86_64::pit::init();
    }
    log_info!("PIT Timer configured @ 100Hz.");

    // 3. تهيئة الفأرة PS/2
    drivers::mouse::init();
    log_info!("PS/2 Mouse driver initialized (IRQ 12 active).");

    // 4. تهيئة تعليمات syscall / sysret
    arch::x86_64::syscall::init();

    // 5. إدارة الذاكرة الافتراضية
    let hhdm_offset = HHDM_REQUEST.response().expect("Limine HHDM missing").offset;
    unsafe {
        mm::paging::VirtualMemoryManager::init(hhdm_offset);
    }

    // 6. مشغلات العتاد: فحص الـ PCI
    drivers::pci::init();

    // 7. تحميل وتركيب الـ Ramdisk (TarFS)
    if let Some(mod_resp) = MODULES_REQUEST.response() {
        let modules = mod_resp.modules();
        if let Some(&first_mod) = modules.first() {
            let (mod_ptr, mod_size) = unsafe { extract_limine_file_info(first_mod) };
            log_info!("Initrd module found: Base={:p}, Size={} bytes", mod_ptr, mod_size);
            unsafe {
                fs::tar::init(mod_ptr, mod_size);
            }
        } else {
            log_warn!("No modules found in Limine response.");
        }
    }

    // 8. تهيئة نظام تعدد المهام
    task::init();

    unsafe {
        if let Some(sched) = &mut *core::ptr::addr_of_mut!(task::SCHEDULER) {
            let main_task = task::Task::new(0, "Main Kernel Shell", main_shell_entry);
            sched.tasks.push(main_task);
        }
    }

    task::spawn(1, "Background Worker 1", background_worker_one);
    task::spawn(2, "Background Worker 2", background_worker_two);

    // 9. تفعيل المقاطعات العتادية (sti)
    log_info!("Enabling Hardware Interrupts (STI)...");
    unsafe {
        asm!("sti", options(nomem, nostack));
    }

    log_info!("Kernel Interactive Terminal Active.");
    arch::x86_64::interrupts::print_prompt();

    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("\n[KERNEL PANIC]: {}", info);
    log_error!("KERNEL PANIC: {}", info);
    loop {
        core::hint::spin_loop();
    }
}
