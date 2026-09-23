use crate::arch::x86_64::gdt::KERNEL_CODE_SELECTOR;
use crate::arch::x86_64::idt::InterruptDescriptorTable;
use crate::arch::x86_64::keyboard::{handle_scancode, read_scancode, KeyEvent};
use crate::arch::x86_64::pic;
use crate::arch::x86_64::pit;
use crate::drivers::ata::write_file_content;
use crate::drivers::mouse::on_mouse_interrupt;
use crate::drivers::pci::{class_name, PCI_DEVICES};
use crate::font::FONT_WIDTH;
use crate::fs::elf::load_and_run_elf;
use crate::fs::{vfs_list_all, vfs_read_bytes, VfsNode};
use crate::mm::heap::{HEAP_ALLOCATOR, HEAP_SIZE};
use crate::task::{dispatch_job, get_job_state, yield_now, JobState, SCHEDULER};
use crate::writer::WRITER;
use crate::{log_error, log_fatal, log_info, log_warn, CORES_ONLINE, CORE_HEARTBEAT};
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::{asm, naked_asm};
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};

pub static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();
static mut INPUT_BUFFER: Option<String> = None;
static mut CURSOR_INDEX: usize = 0;
static mut HISTORY: Option<Vec<String>> = None;
static mut HISTORY_INDEX: usize = 0;
static mut CLIPBOARD: Option<String> = None;

static mut PENDING_COMMAND: Option<String> = None;

pub static IS_RUNNING_PROGRAM: AtomicBool = AtomicBool::new(false);
pub static GUI_ACTIVE: AtomicBool = AtomicBool::new(false);
static MOUSE_BLINK_STATE: AtomicBool = AtomicBool::new(true);

// 💡 قناة الذاكرة الذرية المشتركة بين Core 0 و Core 1 Compositor
pub static SHARED_MOUSE_X: AtomicIsize = AtomicIsize::new(200);
pub static SHARED_MOUSE_Y: AtomicIsize = AtomicIsize::new(200);
pub static SHARED_MOUSE_BUTTONS: AtomicU64 = AtomicU64::new(0);

static mut HANA_ACTIVE: bool = false;
static mut HANA_FILENAME: Option<String> = None;
static mut HANA_CONTENT: Option<Vec<String>> = None;
static mut HANA_CX: usize = 0;
static mut HANA_CY: usize = 0;

static PARALLEL_COMPLETED: AtomicU64 = AtomicU64::new(0);

fn heavy_computation_worker() {
    let mut sum: u64 = 0;
    for i in 1..=5_000_000 {
        sum = sum.wrapping_add(i ^ (i >> 3));
    }
    core::hint::black_box(sum);
    PARALLEL_COMPLETED.fetch_add(1, Ordering::SeqCst);
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

pub fn init() {
    unsafe {
        *addr_of_mut!(INPUT_BUFFER) = Some(String::with_capacity(128));
        *addr_of_mut!(CURSOR_INDEX) = 0;
        *addr_of_mut!(HISTORY) = Some(Vec::new());
        *addr_of_mut!(CLIPBOARD) = Some(String::new());
        *addr_of_mut!(PENDING_COMMAND) = None;

        let idt_ptr = addr_of_mut!(IDT);

        (*idt_ptr).entries[0].set_handler(divide_by_zero_handler as *const () as usize, KERNEL_CODE_SELECTOR, 0, 0x8E);
        (*idt_ptr).entries[3].set_handler(breakpoint_handler as *const () as usize, KERNEL_CODE_SELECTOR, 0, 0x8E);
        (*idt_ptr).entries[6].set_handler(invalid_opcode_handler as *const () as usize, KERNEL_CODE_SELECTOR, 0, 0x8E);
        (*idt_ptr).entries[8].set_handler(double_fault_handler as *const () as usize, KERNEL_CODE_SELECTOR, 1, 0x8E);
        (*idt_ptr).entries[13].set_handler(general_protection_fault_handler as *const () as usize, KERNEL_CODE_SELECTOR, 0, 0x8E);
        (*idt_ptr).entries[14].set_handler(page_fault_handler as *const () as usize, KERNEL_CODE_SELECTOR, 0, 0x8E);
        (*idt_ptr).entries[32].set_handler(timer_interrupt_preempt_entry as *const () as usize, KERNEL_CODE_SELECTOR, 0, 0x8E);
        (*idt_ptr).entries[33].set_handler(keyboard_interrupt_handler as *const () as usize, KERNEL_CODE_SELECTOR, 0, 0x8E);
        (*idt_ptr).entries[44].set_handler(mouse_interrupt_handler as *const () as usize, KERNEL_CODE_SELECTOR, 0, 0x8E);

        InterruptDescriptorTable::load_raw(idt_ptr);
    }
}

pub fn take_pending_command() -> Option<String> {
    unsafe {
        if let Some(cmd) = (*addr_of_mut!(PENDING_COMMAND)).take() {
            Some(cmd)
        } else {
            None
        }
    }
}

pub extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        let current_text = (*addr_of_mut!(INPUT_BUFFER)).as_deref().unwrap_or("");
        let c_idx = *addr_of_mut!(CURSOR_INDEX);
        on_mouse_interrupt(current_text, c_idx);
        pic::send_eoi(12);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn on_timer_tick_visual() {
    pit::on_tick();

    let ticks = pit::get_ticks();
    if ticks % 50 == 0 {
        let visible = MOUSE_BLINK_STATE.fetch_xor(true, Ordering::Relaxed);
        unsafe {
            if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                if !HANA_ACTIVE && !IS_RUNNING_PROGRAM.load(Ordering::Relaxed) && !GUI_ACTIVE.load(Ordering::Relaxed) {
                    writer.draw_cursor(visible);
                }
            }
        }
    }
}

#[unsafe(naked)]
extern "C" fn timer_interrupt_preempt_entry() {
    naked_asm!(
        "push r15", "push r14", "push r13", "push r12", "push r11", "push r10", "push r9", "push r8",
        "push rdi", "push rsi", "push rbp", "push rdx", "push rcx", "push rbx", "push rax",
        "call {on_tick_visual}",
        "mov al, 0x20", "out 0x20, al",
        "mov rdi, rsp",
        "call {schedule}",
        "mov rsp, rax",
        "pop rax", "pop rbx", "pop rcx", "pop rdx", "pop rbp", "pop rsi", "pop rdi",
        "pop r8", "pop r9", "pop r10", "pop r11", "pop r12", "pop r13", "pop r14", "pop r15",
        "iretq",
        on_tick_visual = sym on_timer_tick_visual,
        schedule = sym crate::task::schedule_preemptive,
    );
}

fn hana_render() {
    unsafe {
        let writer = match &mut *addr_of_mut!(WRITER) {
            Some(w) => w,
            None => return,
        };

        writer.clear(15, 23, 42);
        let char_w = FONT_WIDTH * writer.scale;
        let char_h = 16 * writer.scale;

        writer.cursor_x = 24;
        writer.cursor_y = 12;
        writer.write_str("== HANA EDITOR v1.1 == [File: ", 56, 189, 248);
        if let Some(name) = &*addr_of_mut!(HANA_FILENAME) {
            writer.write_str(name, 255, 255, 255);
        }
        writer.write_str("]", 56, 189, 248);

        if let Some(lines) = &*addr_of_mut!(HANA_CONTENT) {
            for (idx, line) in lines.iter().enumerate() {
                writer.cursor_x = 24;
                writer.cursor_y = 12 + ((idx + 2) * char_h);
                writer.write_str(line, 241, 245, 249);
            }
        }

        writer.cursor_x = 24;
        writer.cursor_y = writer.height - 32;
        writer.write_str("[Ctrl+S] Save  |  [Ctrl+Q / Esc] Exit  |  Arrow Keys: Navigate", 148, 163, 184);

        let cx = *addr_of_mut!(HANA_CX);
        let cy = *addr_of_mut!(HANA_CY);
        let cur_x = 24 + (cx * char_w);
        let cur_y = 12 + ((cy + 2) * char_h);
        for y in (cur_y + char_h - 4)..(cur_y + char_h) {
            for x in cur_x..(cur_x + char_w) {
                writer.put_pixel(x, y, 56, 189, 248);
            }
        }
    }
}

fn hana_start(filename: &str) {
    unsafe {
        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
            writer.save_screen();
        }

        let initial_lines = match vfs_read_bytes(filename) {
            Ok(bytes) => {
                if let Ok(s) = core::str::from_utf8(&bytes) {
                    s.lines().map(String::from).collect()
                } else {
                    alloc::vec![String::from("")]
                }
            }
            Err(_) => alloc::vec![String::from("")],
        };

        *addr_of_mut!(HANA_FILENAME) = Some(String::from(filename));
        *addr_of_mut!(HANA_CONTENT) = Some(initial_lines);
        *addr_of_mut!(HANA_CX) = 0;
        *addr_of_mut!(HANA_CY) = 0;
        *addr_of_mut!(HANA_ACTIVE) = true;

        hana_render();
    }
}

fn hana_save() {
    unsafe {
        if let (Some(name), Some(lines)) = (&*addr_of_mut!(HANA_FILENAME), &*addr_of_mut!(HANA_CONTENT)) {
            let mut full = String::new();
            for l in lines {
                full.push_str(l);
                full.push_str("\r\n");
            }
            let _ = write_file_content(1, name, full.as_bytes());
            if let Some(w) = &mut *addr_of_mut!(WRITER) {
                w.cursor_x = 24;
                w.cursor_y = w.height - 32;
                w.write_str("[✓] SAVED! (Bytes updated on Disk)                   ", 74, 222, 128);
            }
        }
    }
}

fn hana_exit() {
    unsafe {
        *addr_of_mut!(HANA_ACTIVE) = false;
        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
            writer.restore_screen();
        }
    }
}

pub extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        let scancode = read_scancode();

        // 💡 إتاحة إغلاق الـ Compositor والعودة للشل بمفتاح Escape
        if GUI_ACTIVE.load(Ordering::Relaxed) {
            if scancode == 0x01 { // Escape key
                GUI_ACTIVE.store(false, Ordering::SeqCst);
                if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                    writer.restore_screen();
                }
                print_prompt();
            }
            pic::send_eoi(1);
            return;
        }

        if HANA_ACTIVE {
            match handle_scancode(scancode) {
                KeyEvent::Char(c) => {
                    let lines = (*addr_of_mut!(HANA_CONTENT)).as_mut().unwrap();
                    let cx = addr_of_mut!(HANA_CX);
                    let cy = addr_of_mut!(HANA_CY);

                    match c {
                        '\x08' => {
                            if *cx > 0 {
                                lines[*cy].remove(*cx - 1);
                                *cx -= 1;
                            } else if *cy > 0 {
                                let prev = lines.remove(*cy);
                                *cy -= 1;
                                *cx = lines[*cy].len();
                                lines[*cy].push_str(&prev);
                            }
                        }
                        '\n' => {
                            let rest = lines[*cy].split_off(*cx);
                            lines.insert(*cy + 1, rest);
                            *cy += 1;
                            *cx = 0;
                        }
                        _ => {
                            lines[*cy].insert(*cx, c);
                            *cx += 1;
                        }
                    }
                    hana_render();
                }
                KeyEvent::LeftArrow => {
                    let cx = addr_of_mut!(HANA_CX);
                    if *cx > 0 { *cx -= 1; hana_render(); }
                }
                KeyEvent::RightArrow => {
                    let lines = (*addr_of_mut!(HANA_CONTENT)).as_ref().unwrap();
                    let cx = addr_of_mut!(HANA_CX);
                    let cy = *addr_of_mut!(HANA_CY);
                    if *cx < lines[cy].len() { *cx += 1; hana_render(); }
                }
                KeyEvent::UpArrow => {
                    let cy = addr_of_mut!(HANA_CY);
                    if *cy > 0 {
                        *cy -= 1;
                        let lines = (*addr_of_mut!(HANA_CONTENT)).as_ref().unwrap();
                        let cx = addr_of_mut!(HANA_CX);
                        *cx = core::cmp::min(*cx, lines[*cy].len());
                        hana_render();
                    }
                }
                KeyEvent::DownArrow => {
                    let lines = (*addr_of_mut!(HANA_CONTENT)).as_ref().unwrap();
                    let cy = addr_of_mut!(HANA_CY);
                    if *cy + 1 < lines.len() {
                        *cy += 1;
                        let cx = addr_of_mut!(HANA_CX);
                        *cx = core::cmp::min(*cx, lines[*cy].len());
                        hana_render();
                    }
                }
                KeyEvent::CtrlS => hana_save(),
                KeyEvent::CtrlQ | KeyEvent::Escape => hana_exit(),
                _ => {}
            }

            pic::send_eoi(1);
            return;
        }

        match handle_scancode(scancode) {
            KeyEvent::Char(c) => {
                if !IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
                    let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_mut().unwrap();
                    let c_idx = addr_of_mut!(CURSOR_INDEX);

                    match c {
                        '\x08' => {
                            if *c_idx > 0 && !buffer.is_empty() {
                                buffer.remove(*c_idx - 1);
                                *c_idx -= 1;
                                if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                                    writer.redraw_line_text(buffer.as_str(), *c_idx);
                                }
                                crate::serial_print!("\x08 \x08");
                            }
                        }
                        '\n' => {
                            if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                                writer.write_char('\n', 255, 255, 255);
                            }
                            crate::serial_println!();

                            let cmd_str = buffer.clone();
                            buffer.clear();
                            *c_idx = 0;

                            if !cmd_str.trim().is_empty() {
                                if let Some(hist) = &mut *addr_of_mut!(HISTORY) {
                                    hist.push(cmd_str.clone());
                                    *addr_of_mut!(HISTORY_INDEX) = hist.len();
                                }
                            }

                            *addr_of_mut!(PENDING_COMMAND) = Some(cmd_str);

                            pic::send_eoi(1);
                            return;
                        }
                        _ => {
                            buffer.insert(*c_idx, c);
                            *c_idx += 1;

                            if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                                writer.redraw_line_text(buffer.as_str(), *c_idx);
                            }
                            crate::serial_print!("{}", c);
                        }
                    }
                }
            }
            KeyEvent::CtrlC => {
                let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_ref().unwrap();
                if let Some(clip) = &mut *addr_of_mut!(CLIPBOARD) {
                    *clip = buffer.clone();
                    log_info!("SHELL", "Copied to clipboard: '{}'", clip);
                }
            }
            KeyEvent::CtrlV => {
                if let Some(clip) = &*addr_of_mut!(CLIPBOARD) {
                    if !clip.is_empty() {
                        let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_mut().unwrap();
                        let c_idx = addr_of_mut!(CURSOR_INDEX);
                        buffer.insert_str(*c_idx, clip);
                        *c_idx += clip.len();

                        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                            writer.redraw_line_text(buffer.as_str(), *c_idx);
                        }
                        crate::serial_print!("{}", clip);
                    }
                }
            }
            KeyEvent::CtrlX => {
                let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_mut().unwrap();
                if let Some(clip) = &mut *addr_of_mut!(CLIPBOARD) {
                    *clip = buffer.clone();
                }
                buffer.clear();
                *addr_of_mut!(CURSOR_INDEX) = 0;
                if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                    writer.clear_current_line(5);
                }
            }
            KeyEvent::UpArrow => {
                if !IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
                    if let Some(hist) = &*addr_of_mut!(HISTORY) {
                        let h_idx = addr_of_mut!(HISTORY_INDEX);
                        if !hist.is_empty() && *h_idx > 0 {
                            *h_idx -= 1;
                            let prev_cmd = &hist[*h_idx];
                            let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_mut().unwrap();
                            buffer.clear();
                            buffer.push_str(prev_cmd);
                            *addr_of_mut!(CURSOR_INDEX) = buffer.len();

                            if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                                writer.redraw_line_text(buffer.as_str(), *addr_of_mut!(CURSOR_INDEX));
                            }
                        }
                    }
                }
            }
            KeyEvent::DownArrow => {
                if !IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
                    if let Some(hist) = &*addr_of_mut!(HISTORY) {
                        let h_idx = addr_of_mut!(HISTORY_INDEX);
                        let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_mut().unwrap();
                        if *h_idx + 1 < hist.len() {
                            *h_idx += 1;
                            let next_cmd = &hist[*h_idx];
                            buffer.clear();
                            buffer.push_str(next_cmd);
                            *addr_of_mut!(CURSOR_INDEX) = buffer.len();
                        } else {
                            *h_idx = hist.len();
                            buffer.clear();
                            *addr_of_mut!(CURSOR_INDEX) = 0;
                        }

                        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                            writer.redraw_line_text(buffer.as_str(), *addr_of_mut!(CURSOR_INDEX));
                        }
                    }
                }
            }
            KeyEvent::LeftArrow => {
                let c_idx = addr_of_mut!(CURSOR_INDEX);
                if *c_idx > 0 {
                    *c_idx -= 1;
                    let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_ref().unwrap();
                    if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                        writer.redraw_line_text(buffer.as_str(), *c_idx);
                    }
                }
            }
            KeyEvent::RightArrow => {
                let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_ref().unwrap();
                let c_idx = addr_of_mut!(CURSOR_INDEX);
                if *c_idx < buffer.len() {
                    *c_idx += 1;
                    if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                        writer.redraw_line_text(buffer.as_str(), *c_idx);
                    }
                }
            }
            _ => {}
        }

        pic::send_eoi(1);
    }
}

pub fn print_prompt() {
    unsafe {
        if !HANA_ACTIVE && !GUI_ACTIVE.load(Ordering::Relaxed) {
            if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                writer.write_str("eos> ", 56, 189, 248);
                writer.draw_cursor(true);
            }
            crate::serial_print!("eos> ");
        }
    }
}

pub fn execute_command(cmd: &str) {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return;
    }

    match trimmed {
        "help" => {
            log_info!("SHELL", "Commands: gui, bench, cores, ls, cat <file>, view <file>, hana <file>, history, top, ps, uptime, reboot, clear");
        }
        // 💡 أمر إطلاق سطح المكتب المنفصل على النواة المخصصة (Core 1)
        "gui" | "desktop" => {
            log_info!("DESKTOP", "Dispatching Dedicated GUI Compositor to Core 1...");
            unsafe {
                if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                    writer.save_screen();
                }
            }
            GUI_ACTIVE.store(true, Ordering::SeqCst);
            if let Err(e) = dispatch_job(1, crate::compositor::compositor_core_entry) {
                log_error!("DESKTOP", "Failed to pin compositor to Core 1: {}", e);
                GUI_ACTIVE.store(false, Ordering::SeqCst);
            }
        }
        "history" => unsafe {
            if let Some(hist) = &*addr_of_mut!(HISTORY) {
                log_info!("SHELL", "Command History ({} commands):", hist.len());
                for (i, c) in hist.iter().enumerate() {
                    log_info!("SHELL", "  {:2}: {}", i + 1, c);
                }
            }
        },
        "bench" | "parallel" => {
            log_info!("BENCH", "================ MULTI-CORE PERFORMANCE BENCHMARK ================");
            log_info!("BENCH", "[1/2] Running 7 heavy jobs sequentially on Core 0 (Single-Core)...");
            let start_single = pit::read_tsc();
            for _ in 0..7 {
                heavy_computation_worker();
            }
            let end_single = pit::read_tsc();
            let single_duration = end_single.saturating_sub(start_single);
            log_info!("BENCH", "  -> Core 0 Duration: {} TSC cycles.", single_duration);

            log_info!("BENCH", "[2/2] Dispatching 7 heavy jobs simultaneously to Cores 1..7 (Parallel)...");
            PARALLEL_COMPLETED.store(0, Ordering::SeqCst);
            let start_multi = pit::read_tsc();

            for core_id in 1..=7 {
                if let Err(e) = dispatch_job(core_id, heavy_computation_worker) {
                    log_error!("BENCH", "Failed to dispatch to Core {}: {}", core_id, e);
                }
            }

            while PARALLEL_COMPLETED.load(Ordering::SeqCst) < 7 {
                core::hint::spin_loop();
            }
            let end_multi = pit::read_tsc();
            let multi_duration = end_multi.saturating_sub(start_multi);
            log_info!("BENCH", "  -> Multi-Core Duration: {} TSC cycles.", multi_duration);

            if multi_duration > 0 {
                let speedup = (single_duration * 10) / multi_duration;
                log_info!("BENCH", "  -> Parallel Speedup: {}.{}x faster!", speedup / 10, speedup % 10);
            }
            log_info!("BENCH", "==================================================================");
        }
        "cores" | "smp" => {
            let online = CORES_ONLINE.load(Ordering::SeqCst);
            log_info!("SMP", "=== MULTI-CORE LIVE PROCESSOR STATUS ===");
            log_info!("SMP", "Total Online Cores: {} / 8", online);
            for i in 0..8 {
                let ticks = CORE_HEARTBEAT[i].load(Ordering::Relaxed);
                let role = match i {
                    0 => "BSP (Master / Shell / IRQ)",
                    1 => "AP  (Dedicated GUI Compositor)",
                    _ => "AP  (Worker Core)",
                };
                let queue_state = if i == 0 {
                    "ONLINE"
                } else {
                    match get_job_state(i) {
                        JobState::Idle => "IDLE",
                        JobState::Submitted => "QUEUED",
                        JobState::Running => "BUSY",
                        JobState::Finished => "DONE",
                    }
                };
                log_info!("SMP", "  Core #{}: [{:<6}] | Queue: [{:<7}] | Ticks: {:<12} | Role: {}", 
                    i, if ticks > 0 { "ACTIVE" } else { "OFF" }, queue_state, ticks, role
                );
            }
            log_info!("SMP", "========================================");
        }
        "ls" => {
            let nodes = vfs_list_all();
            log_info!("VFS", "Listing All System Files across Mounted Filesystems (Total: {}):", nodes.len());
            for node in nodes {
                match node {
                    VfsNode::Ext2(name, sz)    => log_info!("VFS", "  [EXT2-ROOT] {:<18} | {:>8} bytes", name, sz),
                    VfsNode::Disk(name, sz)    => log_info!("VFS", "  [FAT-DISK]  {:<18} | {:>8} bytes", name, sz),
                    VfsNode::Ramdisk(name, sz) => log_info!("VFS", "  [INITRD]    {:<18} | {:>8} bytes", name, sz),
                }
            }
        }
        "top" | "tm" | "tasks" => {
            log_info!("SHELL", "================== EOS TASK & RESOURCE MANAGER ==================");
            let kernel_heap_used = HEAP_ALLOCATOR.used();
            let kernel_heap_total = HEAP_SIZE;
            let online_cpus = CORES_ONLINE.load(Ordering::SeqCst);
            log_info!("SHELL", "  - Processors Online       : {} Cores Active", online_cpus);
            log_info!("SHELL", "  - Physical RAM            : 2048 MB Total Assigned");
            log_info!("SHELL", "  - Kernel Heap Usage       : {} KB / {} KB", kernel_heap_used / 1024, kernel_heap_total / 1024);
            log_info!("SHELL", "  - Unified VFS Mounted     : Native Linux ext2 + Windows FAT32 + TarFS");
            log_info!("SHELL", "=================================================================");
        }
        "lspci" => unsafe {
            if let Some(devices) = &*addr_of_mut!(PCI_DEVICES) {
                log_info!("PCI", "PCI Bus Devices (Total: {}):", devices.len());
                for dev in devices {
                    let desc = class_name(dev.class_code, dev.subclass);
                    log_info!("PCI", 
                        "  [{:02x}:{:02x}.{}] ID {:04x}:{:04x} | {}",
                        dev.bus, dev.device, dev.function, dev.vendor_id, dev.device_id, desc
                    );
                }
            }
        },
        "ps" => unsafe {
            if let Some(sched) = &*addr_of_mut!(SCHEDULER) {
                log_info!("SCHED", "Tasks (Total: {}, Current: {}, Preempt: {}):", 
                    sched.tasks.len(), 
                    sched.current_task_idx,
                    if sched.preemptive_enabled { "ON" } else { "OFF" }
                );
                for (idx, task) in sched.tasks.iter().enumerate() {
                    let indicator = if idx == sched.current_task_idx { "*" } else { " " };
                    log_info!("SCHED", " {} [{}] '{}' | Counter: {}", indicator, task.id, task.name, task.counter);
                }
            }
        },
        "uptime" => {
            let sec = pit::get_uptime_seconds();
            let ticks = pit::get_ticks();
            log_info!("PIT", "System Uptime: {}s ({} timer ticks @ 100Hz)", sec, ticks);
        },
        "start" => {
            crate::task::enable_preemption();
            log_info!("SCHED", "Preemptive scheduler engaged.");
        },
        "yield" => {
            yield_now();
            log_info!("SCHED", "Manual yield completed.");
        },
        "sleep" => {
            log_info!("PIT", "Sleeping for 2000ms...");
            pit::sleep_ms(2000);
            log_info!("PIT", "Awake.");
        },
        "cr3" => {
            let cr3 = crate::mm::paging::read_cr3();
            log_info!("MMU", "CR3 Register: {:#018x}", cr3);
        },
        "clear" => unsafe {
            if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                writer.clear(15, 23, 42);
            }
        },
        "reboot" => unsafe {
            asm!("2: in al, 0x64; test al, 0x02; jnz 2b; mov al, 0xFE; out 0x64, al", options(nomem, nostack));
        },
        other => {
            if let Some(target_file) = other.strip_prefix("view ") {
                let clean_name = target_file.trim();
                
                unsafe {
                    if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                        writer.save_screen();
                    }
                }

                if let Ok(elf_bytes) = vfs_read_bytes("app.elf") {
                    IS_RUNNING_PROGRAM.store(true, Ordering::SeqCst);
                    if let Err(e) = load_and_run_elf(&elf_bytes, clean_name) {
                        log_error!("VIEWER", "Failed to launch viewer: {}", e);
                    }
                    IS_RUNNING_PROGRAM.store(false, Ordering::SeqCst);
                } else {
                    log_error!("VIEWER", "Viewer app.elf not found in initrd.");
                }

                unsafe {
                    if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                        writer.restore_screen();
                    }
                }
            } else if other == "view" {
                log_warn!("VIEWER", "Usage: view <filename> (e.g. 'view icon.png')");
            } else if let Some(filename) = other.strip_prefix("hana ") {
                hana_start(filename.trim());
            } else if let Some(path) = other.strip_prefix("cat ") {
                match vfs_read_bytes(path.trim()) {
                    Ok(bytes) => {
                        log_info!("VFS", "--- [{}] ({} bytes) ---", path.trim(), bytes.len());
                        if let Ok(s) = core::str::from_utf8(&bytes) {
                            for line in s.lines() {
                                log_info!("VFS", "{}", line);
                            }
                        } else {
                            log_warn!("VFS", "Binary file preview:");
                            log_info!("VFS", "{:02x?}", &bytes[..core::cmp::min(bytes.len(), 64)]);
                        }
                        log_info!("VFS", "--- [EOF] ---");
                    }
                    Err(e) => log_error!("VFS", "Cannot open file: {}", e),
                }
            } else if let Some(elf_name) = other.strip_prefix("exec ") {
                match vfs_read_bytes(elf_name.trim()) {
                    Ok(bytes) => {
                        IS_RUNNING_PROGRAM.store(true, Ordering::SeqCst);
                        if let Err(e) = load_and_run_elf(&bytes, "") {
                            log_error!("ELF", "Execution failed: {}", e);
                        }
                        IS_RUNNING_PROGRAM.store(false, Ordering::SeqCst);
                    }
                    Err(e) => log_error!("ELF", "File not found: {}", e),
                }
            } else {
                log_error!("SHELL", "Unknown command: '{}'. Type 'help'.", other);
            }
        }
    }
}

pub extern "x86-interrupt" fn divide_by_zero_handler(stack_frame: InterruptStackFrame) {
    log_fatal!("CPU", "CPU EXCEPTION: DIVIDE BY ZERO (#DE) at RIP {:#018x}", stack_frame.instruction_pointer);
}

pub extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    log_warn!("CPU", "BREAKPOINT (INT 3) at {:#018x}", stack_frame.instruction_pointer);
}

pub extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    log_fatal!("CPU", "CPU EXCEPTION: INVALID OPCODE (#UD) at RIP {:#018x}", stack_frame.instruction_pointer);
}

pub extern "x86-interrupt" fn general_protection_fault_handler(stack_frame: InterruptStackFrame, code: u64) {
    log_fatal!("CPU", "CPU EXCEPTION: #GP (Code: {:#x}) at RIP {:#018x} | RSP {:#018x}", code, stack_frame.instruction_pointer, stack_frame.stack_pointer);
    
    if (stack_frame.code_segment & 3) != 0 {
        log_error!("CPU", "Terminating faulty Ring 3 process safely and returning to Kernel.");
        unsafe {
            let saved_rsp = crate::arch::x86_64::syscall::KERNEL_SAVED_RSP;
            if saved_rsp != 0 {
                asm!(
                    "mov rsp, {sp}",
                    "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
                    "sti", "ret",
                    sp = in(reg) saved_rsp,
                    options(noreturn)
                );
            }
        }
    }
    loop { core::hint::spin_loop(); }
}

pub extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame, _code: u64) -> ! {
    log_fatal!("CPU", "DOUBLE FAULT (#DF) at RIP {:#018x}", stack_frame.instruction_pointer);
    loop { core::hint::spin_loop(); }
}

pub extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    let cr2: u64;
    unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)); }
    log_fatal!("MMU", "PAGE FAULT on Address {:#018x} (Error: {:#b}) at RIP {:#018x} | CS: {:#x}", cr2, error_code, stack_frame.instruction_pointer, stack_frame.code_segment);

    if (stack_frame.code_segment & 3) != 0 {
        log_error!("MMU", "Terminating faulting userspace task and recovering shell.");
        unsafe {
            let saved_rsp = crate::arch::x86_64::syscall::KERNEL_SAVED_RSP;
            if saved_rsp != 0 {
                asm!(
                    "mov rsp, {sp}",
                    "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
                    "sti", "ret",
                    sp = in(reg) saved_rsp,
                    options(noreturn)
                );
            }
        }
    }

    loop { core::hint::spin_loop(); }
}
