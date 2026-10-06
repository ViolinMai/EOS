#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

pub mod config;
pub mod input;
pub mod profiler;
pub mod net;
mod arch;
mod drivers;
mod fs;
mod logger;
mod mm;
mod serial;
mod task;
mod writer;

use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use limine::request::{FramebufferRequest, HhdmRequest, MemmapRequest, ModulesRequest, MpRequest, StackSizeRequest};

#[used]
#[unsafe(link_section = ".limine_requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static MEMORY_MAP_REQUEST: MemmapRequest = MemmapRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static MODULES_REQUEST: ModulesRequest = ModulesRequest::new();
#[used]
#[unsafe(link_section = ".limine_requests")]
static MP_REQUEST: MpRequest = MpRequest::new(0);
#[used]
#[unsafe(link_section = ".limine_requests")]
static STACK_SIZE_REQUEST: StackSizeRequest = StackSizeRequest::new(128 * 1024);

pub static CORES_ONLINE: AtomicU64 = AtomicU64::new(1);
pub static CORE_READY: [AtomicBool; 8] = [
    AtomicBool::new(true),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];
pub static CORE_HEARTBEAT: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
static AP_BOOT_LATCH: AtomicBool = AtomicBool::new(false);

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    serial::SERIAL1.init();
    serial::SERIAL2.init();
    crate::log_info!("KERNEL", "SERIAL CONSOLE INITIALIZED");

    arch::x86_64::smp::enable_sse();

    let hhdm_resp = HHDM_REQUEST.response().expect("HHDM failed");
    let hhdm_offset = hhdm_resp.offset;
    let mmap_resp = MEMORY_MAP_REQUEST.response().expect("MemMap failed");

    unsafe {
        mm::frame::BitmapFrameAllocator::init(mmap_resp.entries(), hhdm_offset);
        mm::paging::VirtualMemoryManager::init(hhdm_offset);
    }

    if let Some(fb_resp) = FRAMEBUFFER_REQUEST.response() {
        if let Some(fb) = fb_resp.framebuffers().first() {
            unsafe {
                let w = writer::FramebufferWriter::new(
                    fb.address() as *mut u8,
                    fb.width as usize,
                    fb.height as usize,
                    fb.pitch as usize,
                );
                let slice = core::slice::from_raw_parts_mut(fb.address() as *mut u32, (fb.pitch as usize / 4) * fb.height as usize);
                slice.fill(0xFF0F172A);
                *core::ptr::addr_of_mut!(writer::WRITER) = Some(w);
            }
            crate::log_info!("FRAMEBUFFER", "Initialized: {}x{} (Pitch: {})", fb.width, fb.height, fb.pitch);
        }
    }

    unsafe {
        mm::heap::HEAP_ALLOCATOR.init(
            (0x10000000 + hhdm_offset) as *mut u8,
            config::CONFIG.default_heap_size_mb * 1024 * 1024
        );
    }

    arch::x86_64::gdt::init_core(0);
    arch::x86_64::interrupts::init();
    drivers::mouse::init();
    arch::x86_64::syscall::init();

    drivers::pci::init();
    drivers::e1000::init();
    net::init();

    drivers::ata::init();
    fs::ext2::init(2);

    if let Some(mod_resp) = MODULES_REQUEST.response() {
        for module in mod_resp.modules() {
            unsafe { fs::tar::init(module.data().as_ptr(), module.data().len()); }
            break;
        }
    }

    config::CONFIG.load_from_disk();
    task::init();

    if let Some(mp_resp) = MP_REQUEST.response() {
        let cpus = mp_resp.cpus();
        for cpu in cpus {
            if cpu.lapic_id != 0 {
                unsafe {
                    let cpu_ptr = *cpu as *const limine::mp::MpInfo as *mut u8;
                    let goto_addr_ptr = cpu_ptr.add(16) as *mut u64;
                    core::ptr::write_volatile(goto_addr_ptr, ap_startup_entry as *const () as u64);
                }
            }
        }
        AP_BOOT_LATCH.store(true, Ordering::SeqCst);
    }

    for _ in 0..200_000 {
        core::hint::spin_loop();
    }

    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }

    crate::log_info!("DESKTOP", "Delegating Desktop & GUI to Userspace process 'user_app.elf' on Core 2...");
    match task::request_elf_execution("user_app.elf", "") {
        Ok(core_id) => crate::log_info!("DESKTOP", "ELF task assigned to Core {}", core_id),
        Err(e) => crate::log_error!("DESKTOP", "FATAL: Could not dispatch user_app.elf: {}", e),
    }

    loop {
        net::poll();

        while let Some(ev) = input::poll_event() {
            arch::x86_64::syscall::push_user_event(ev);
        }

        if let Some(cmd) = arch::x86_64::interrupts::take_pending_command() {
            arch::x86_64::interrupts::execute_command(&cmd);
            arch::x86_64::interrupts::print_prompt();
        }

        crate::arch::x86_64::pit::sleep_ms(2);
    }
}

extern "C" fn ap_startup_entry(info: &limine::mp::MpInfo) -> ! {
    while !AP_BOOT_LATCH.load(Ordering::Acquire) { core::hint::spin_loop(); }
    let core_id = info.lapic_id as usize;
    if core_id < 8 {
        arch::x86_64::smp::enable_sse();
        arch::x86_64::gdt::init_core(core_id);
        unsafe {
            crate::arch::x86_64::idt::InterruptDescriptorTable::load_raw(
                core::ptr::addr_of_mut!(crate::arch::x86_64::interrupts::IDT)
            );
            core::arch::asm!("sti", options(nomem, nostack));
        }
        arch::x86_64::syscall::init_core_syscall(core_id);
    }
    CORES_ONLINE.fetch_add(1, Ordering::SeqCst);
    if core_id < 8 {
        CORE_READY[core_id].store(true, Ordering::SeqCst);
    }
    crate::log_info!("SMP", "Core {} online and active.", core_id);

    loop {
        if core_id < 8 { CORE_HEARTBEAT[core_id].fetch_add(1, Ordering::Relaxed); }
        let did_work = task::core_poll_and_execute(core_id);
        if !did_work {
            for _ in 0..64 {
                core::hint::spin_loop();
            }
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe { core::arch::asm!("cli", options(nomem, nostack)); }
    crate::serial_println!("\n\x1b[31;1m[KERNEL PANIC]\x1b[0m {}", info);
    loop { core::hint::spin_loop(); }
}
