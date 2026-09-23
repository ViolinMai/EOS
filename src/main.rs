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

    arch::x86_64::gdt::init_core(cpu_id);

    unsafe { 
        arch::x86_64::idt::InterruptDescriptorTable::load_raw(core::ptr::addr_of!(arch::x86_64::interrupts::IDT));
    };

    arch::x86_64::syscall::init_core_syscall(cpu_id);
    CORES_ONLINE.fetch_add(1, Ordering::SeqCst);
    
    loop {
        if cpu_id < 8 {
            CORE_HEARTBEAT[cpu_id].fetch_add(1, Ordering::Relaxed);
            task::core_poll_and_execute(cpu_id);
        }
        for _ in 0..200 { core::hint::spin_loop(); }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    serial::SERIAL1.init();
    serial::SERIAL2.init(); 

    // 💡 1. تهيئة إدارة الذاكرة فوراً قبل أي عملية تخصيص (Allocation)
    let hhdm_offset = HHDM_REQUEST.response().expect("Limine HHDM missing").offset;
    
    if let Some(memmap) = MEMMAP_REQUEST.response() {
        unsafe {
            *core::ptr::addr_of_mut!(mm::frame::FRAME_ALLOCATOR) = Some(
                mm::frame::BitmapFrameAllocator::init(memmap.entries(), hhdm_offset)
            );
        }
    }

    unsafe { mm::paging::VirtualMemoryManager::init(hhdm_offset); }

    // 💡 2. حجز مساحة حرة مستمرة بحجم 256MB للـ Heap فوراً عبر الـ Usable Entries
    if let Some(memmap) = MEMMAP_REQUEST.response() {
        let heap_size = mm::heap::HEAP_SIZE;
        let mut heap_initialized = false;

        for entry in memmap.entries() {
            let entry_type: u64 = unsafe { core::ptr::read_unaligned(&entry.type_ as *const _ as *const u64) };
            if entry_type == 0 && entry.length as usize >= heap_size {
                let heap_virt = (entry.base + hhdm_offset) as *mut u8;
                unsafe {
                    mm::heap::HEAP_ALLOCATOR.init(heap_virt, heap_size);
                }
                heap_initialized = true;
                break;
            }
        }

        if !heap_initialized {
            // بديل احتياطي إذا كانت القطع مجزأة
            for entry in memmap.entries() {
                let entry_type: u64 = unsafe { core::ptr::read_unaligned(&entry.type_ as *const _ as *const u64) };
                if entry_type == 0 && entry.length as usize >= 64 * 1024 * 1024 {
                    let heap_virt = (entry.base + hhdm_offset) as *mut u8;
                    unsafe {
                        mm::heap::HEAP_ALLOCATOR.init(heap_virt, entry.length as usize);
                    }
                    break;
                }
            }
        }
    }

    // 💡 3. الآن بعد أن أصبح الـ Heap جاهزاً تماماً، نبدأ ببقية الخدمات
    serial_println!("\n==========================================");
    serial_println!("[EOS] SERIAL CONSOLE INITIALIZED");

    if let Some(fb_response) = FRAMEBUFFER_REQUEST.response() {
        if let Some(&framebuffer) = fb_response.framebuffers().first() {
            let buffer_ptr = framebuffer.address() as *mut u8;
            let fb_w = framebuffer.width as usize;
            let fb_h = framebuffer.height as usize;
            let pitch = framebuffer.pitch as usize;

            unsafe {
                let mut w = FrameWriter::new(buffer_ptr, fb_w, fb_h, pitch);
                w.clear(10, 15, 26);
                *core::ptr::addr_of_mut!(WRITER) = Some(w);
            }
        }
    }

    arch::x86_64::gdt::init_core(0);
    arch::x86_64::interrupts::init();
    unsafe {
        arch::x86_64::pic::remap();
        arch::x86_64::pit::init();
    }
    drivers::mouse::init();
    arch::x86_64::syscall::init();
    drivers::pci::init();
    drivers::ata::init();
    fs::ext2::init(2);

    if let Some(mp_resp) = MP_REQUEST.response() {
        let bsp_id = mp_resp.bsp_lapic_id;
        let cpus = mp_resp.cpus();
        for cpu in cpus {
            if cpu.lapic_id != bsp_id {
                unsafe {
                    let raw_ptr = (*cpu) as *const MpInfo as *const u8;
                    let goto_addr_ptr = raw_ptr.add(16) as *mut usize;
                    core::ptr::write_volatile(goto_addr_ptr, ap_entry as *const () as usize);
                }
            }
        }
    }

    if let Some(mod_resp) = MODULES_REQUEST.response() {
        if let Some(&first_mod) = mod_resp.modules().first() {
            let (mod_ptr, mod_size) = unsafe { extract_limine_file_info(first_mod) };
            unsafe { fs::tar::init(mod_ptr, mod_size); }
        }
    }

    task::init();
    unsafe {
        if let Some(sched) = &mut *core::ptr::addr_of_mut!(task::SCHEDULER) {
            sched.tasks.push(task::Task::new(0, "Kernel Master Task", main_shell_entry));
        }
    }

    unsafe { asm!("sti", options(nomem, nostack)); }

    log_info!("SMP", "Architecture Ready: Core 0 [Kernel], Core 1 [Dedicated GUI], Cores 2-7 [Pool]");
    unsafe {
        if let Some(writer) = &mut *core::ptr::addr_of_mut!(WRITER) {
            writer.save_screen();
        }
    }
    crate::arch::x86_64::interrupts::GUI_ACTIVE.store(true, Ordering::SeqCst);
    if let Err(e) = task::dispatch_job(1, crate::compositor::compositor_core_entry) {
        log_error!("DESKTOP", "Failed to pin GUI to Core 1: {}", e);
    }

    loop {
        CORE_HEARTBEAT[0].fetch_add(1, Ordering::Relaxed);
        if let Some(cmd) = arch::x86_64::interrupts::take_pending_command() {
            arch::x86_64::interrupts::execute_command(&cmd);
            arch::x86_64::interrupts::print_prompt();
        }
        unsafe { asm!("hlt", options(nomem, nostack)); }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("\x1b[31m[PANIC]: {}\x1b[0m", info);
    loop { core::hint::spin_loop(); }
}
