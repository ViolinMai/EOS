#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod arch;
mod compositor;
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
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use limine::request::{FramebufferRequest, HhdmRequest, MemmapRequest, ModulesRequest, MpRequest};
use limine::mp::MpInfo;
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
#[unsafe(link_section = ".requests")]
static MP_REQUEST: MpRequest = MpRequest::new(0);

#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: () = ();

pub static CORES_ONLINE: AtomicUsize = AtomicUsize::new(1);
pub static CORE_HEARTBEAT: [AtomicU64; 8] = [
    AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
    AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
];

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

extern "C" fn ap_entry(info: &MpInfo) -> ! {
    let cpu_id = info.lapic_id as usize;
    
    unsafe { 
        arch::x86_64::idt::InterruptDescriptorTable::load_raw(
            core::ptr::addr_of!(arch::x86_64::interrupts::IDT)
        );
    };

    CORES_ONLINE.fetch_add(1, Ordering::SeqCst);

    loop {
        if cpu_id < 8 {
            CORE_HEARTBEAT[cpu_id].fetch_add(1, Ordering::Relaxed);
            task::core_poll_and_execute(cpu_id);
        }
        for _ in 0..500 {
            core::hint::spin_loop();
        }
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

            unsafe {
                let mut w = FrameWriter::new(buffer_ptr, fb_w, fb_h, pitch);
                w.clear(15, 23, 42);
                *core::ptr::addr_of_mut!(WRITER) = Some(w);
            }
        }
    }

    log_info!("KERNEL", "EOS KERNEL (x86_64) ONLINE");
    log_info!("KERNEL", "Framebuffer Initialized: {}x{} (Pitch: {})", fb_w, fb_h, fb_w * 4);

    arch::x86_64::gdt::init();
    log_info!("KERNEL", "GDT & TSS loaded with Ring 3 User Segments.");

    arch::x86_64::interrupts::init();
    log_info!("KERNEL", "IDT initialized (Timer, Keyboard, Mouse).");

    unsafe {
        arch::x86_64::pic::remap();
        arch::x86_64::pit::init();
    }
    log_info!("KERNEL", "PIT Timer configured @ 100Hz.");

    drivers::mouse::init();
    log_info!("KERNEL", "PS/2 Mouse driver initialized (IRQ 12 active).");

    arch::x86_64::syscall::init();

    let hhdm_offset = HHDM_REQUEST.response().expect("Limine HHDM missing").offset;
    
    if let Some(memmap) = MEMMAP_REQUEST.response() {
        unsafe {
            *core::ptr::addr_of_mut!(mm::frame::FRAME_ALLOCATOR) = Some(mm::frame::BitmapFrameAllocator::init(memmap.entries(), hhdm_offset));
        }
    }

    unsafe {
        mm::paging::VirtualMemoryManager::init(hhdm_offset);
    }

    drivers::pci::init();
    drivers::ata::init();

    fs::ext2::init(2);

    if let Some(mp_resp) = MP_REQUEST.response() {
        let bsp_id = mp_resp.bsp_lapic_id;
        let cpus = mp_resp.cpus();
        let cpu_count = cpus.len();
        log_info!("SMP", "Symmetric Multiprocessing: Detected {} Cores (BSP ID: {}).", cpu_count, bsp_id);

        for cpu in cpus {
            if cpu.lapic_id != bsp_id {
                unsafe {
                    let raw_ptr = (*cpu) as *const MpInfo as *const u8;
                    let goto_addr_ptr = raw_ptr.add(16) as *mut usize;
                    core::ptr::write_volatile(goto_addr_ptr, ap_entry as usize);
                }
            }
        }
    }

    if let Some(mod_resp) = MODULES_REQUEST.response() {
        let modules = mod_resp.modules();
        if let Some(&first_mod) = modules.first() {
            let (mod_ptr, mod_size) = unsafe { extract_limine_file_info(first_mod) };
            log_info!("KERNEL", "Initrd module found: Base={:p}, Size={} bytes", mod_ptr, mod_size);
            unsafe {
                fs::tar::init(mod_ptr, mod_size);
            }
        } else {
            log_warn!("KERNEL", "No modules found in Limine response.");
        }
    }

    task::init();

    unsafe {
        if let Some(sched) = &mut *core::ptr::addr_of_mut!(task::SCHEDULER) {
            let main_task = task::Task::new(0, "Main Kernel Shell", main_shell_entry);
            sched.tasks.push(main_task);
        }
    }

    task::spawn(1, "Background Worker 1", background_worker_one);
    task::spawn(2, "Background Worker 2", background_worker_two);

    log_info!("KERNEL", "Enabling Hardware Interrupts (STI)...");
    unsafe {
        asm!("sti", options(nomem, nostack));
    }

    log_info!("KERNEL", "Kernel Interactive Terminal Active. Type 'gui' to launch Genesis.");
    arch::x86_64::interrupts::print_prompt();

    loop {
        CORE_HEARTBEAT[0].fetch_add(1, Ordering::Relaxed);
        
        if let Some(cmd) = arch::x86_64::interrupts::take_pending_command() {
            arch::x86_64::interrupts::execute_command(&cmd);
            arch::x86_64::interrupts::print_prompt();
        }

        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

// 💡 نظام Panic مفصل يسجل كل معلومات الانهيار بدقة
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let location_str = if let Some(loc) = info.location() {
        serial_println!("\n\x1b[31;1m[PANIC OCCURRED]\x1b[0m File: {}:{}:{}", loc.file(), loc.line(), loc.column());
    } else {
        serial_println!("\n\x1b[31;1m[PANIC OCCURRED]\x1b[0m (Unknown Location)");
    };
    let _ = location_str;

    serial_println!("\x1b[31m[REASON]: {}\x1b[0m", info.message());
    log_fatal!("PANIC", "CRASH: {}", info);

    loop {
        core::hint::spin_loop();
    }
}
