use crate::arch::x86_64::gdt::KERNEL_CODE_SELECTOR;
use crate::arch::x86_64::idt::InterruptDescriptorTable;
use crate::arch::x86_64::keyboard::{handle_scancode, read_scancode, KeyEvent};
use crate::arch::x86_64::pic;
use crate::arch::x86_64::pit;
use crate::arch::x86_64::syscall;
use crate::drivers::ata::{read_sector_drive, scan_shared_disk};
use crate::drivers::mouse::on_mouse_interrupt;
use crate::drivers::pci::{class_name, PCI_DEVICES};
use crate::font::FONT_WIDTH;
use crate::fs::elf::{load_and_run_elf, USER_MEM_SIZE};
use crate::fs::tar::INITRD;
use crate::mm::heap::{HEAP_ALLOCATOR, HEAP_SIZE};
use crate::mm::paging::VMM;
use crate::task::{yield_now, SCHEDULER, TASK_STACK_SIZE};
use crate::writer::WRITER;
use crate::{log_debug, log_error, log_info, log_warn};
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::{asm, naked_asm};
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

pub static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();
static mut INPUT_BUFFER: Option<String> = None;
static mut CURSOR_INDEX: usize = 0;
static mut HISTORY: Option<Vec<String>> = None;
static mut HISTORY_INDEX: usize = 0;
static mut CLIPBOARD: Option<String> = None;

pub static IS_RUNNING_PROGRAM: AtomicBool = AtomicBool::new(false);
static MOUSE_BLINK_STATE: AtomicBool = AtomicBool::new(true);

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

        let idt_ptr = addr_of_mut!(IDT);

        (*idt_ptr).entries[0].set_handler(
            divide_by_zero_handler as *const () as usize,
            KERNEL_CODE_SELECTOR,
            0,
            0x8E,
        );

        (*idt_ptr).entries[3].set_handler(
            breakpoint_handler as *const () as usize,
            KERNEL_CODE_SELECTOR,
            0,
            0x8E,
        );

        (*idt_ptr).entries[6].set_handler(
            invalid_opcode_handler as *const () as usize,
            KERNEL_CODE_SELECTOR,
            0,
            0x8E,
        );

        (*idt_ptr).entries[8].set_handler(
            double_fault_handler as *const () as usize,
            KERNEL_CODE_SELECTOR,
            1,
            0x8E,
        );

        (*idt_ptr).entries[13].set_handler(
            general_protection_fault_handler as *const () as usize,
            KERNEL_CODE_SELECTOR,
            0,
            0x8E,
        );

        (*idt_ptr).entries[14].set_handler(
            page_fault_handler as *const () as usize,
            KERNEL_CODE_SELECTOR,
            0,
            0x8E,
        );

        (*idt_ptr).entries[32].set_handler(
            timer_interrupt_preempt_entry as *const () as usize,
            KERNEL_CODE_SELECTOR,
            0,
            0x8E,
        );

        (*idt_ptr).entries[33].set_handler(
            keyboard_interrupt_handler as *const () as usize,
            KERNEL_CODE_SELECTOR,
            0,
            0x8E,
        );

        (*idt_ptr).entries[44].set_handler(
            mouse_interrupt_handler as *const () as usize,
            KERNEL_CODE_SELECTOR,
            0,
            0x8E,
        );

        InterruptDescriptorTable::load_raw(idt_ptr);
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
                writer.draw_cursor(visible);
            }
        }
    }
}

#[unsafe(naked)]
extern "C" fn timer_interrupt_preempt_entry() {
    naked_asm!(
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rdi",
        "push rsi",
        "push rbp",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",

        "call {on_tick_visual}",
        "mov al, 0x20",
        "out 0x20, al",

        "mov rdi, rsp",
        "call {schedule}",
        "mov rsp, rax",

        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rbp",
        "pop rsi",
        "pop rdi",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",

        "iretq",
        on_tick_visual = sym on_timer_tick_visual,
        schedule = sym crate::task::schedule_preemptive,
    );
}

fn get_selected_text() -> Option<String> {
    unsafe {
        if let Some(writer) = &*addr_of_mut!(WRITER) {
            if let (Some(s_x), Some(e_x)) = (writer.select_start_x, writer.select_end_x) {
                let char_w = FONT_WIDTH * writer.scale;
                let start_idx = s_x.min(e_x).saturating_sub(24 + (5 * char_w)) / char_w;
                let end_idx = (s_x.max(e_x).saturating_sub(24 + (5 * char_w)) + (char_w - 1)) / char_w;

                let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_ref().unwrap();
                if start_idx < buffer.len() {
                    let actual_end = end_idx.min(buffer.len());
                    return Some(buffer[start_idx..actual_end].into());
                }
            }
        }
    }
    None
}

pub extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        let scancode = read_scancode();

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

                            pic::send_eoi(1);
                            execute_command(cmd_str.as_str());
                            print_prompt();
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
            KeyEvent::LeftArrow => {
                if !IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
                    let c_idx = addr_of_mut!(CURSOR_INDEX);
                    if *c_idx > 0 {
                        *c_idx -= 1;
                        let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_ref().unwrap();
                        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                            writer.redraw_line_text(buffer.as_str(), *c_idx);
                        }
                    }
                }
            }
            KeyEvent::RightArrow => {
                if !IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
                    let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_ref().unwrap();
                    let c_idx = addr_of_mut!(CURSOR_INDEX);
                    if *c_idx < buffer.len() {
                        *c_idx += 1;
                        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                            writer.redraw_line_text(buffer.as_str(), *c_idx);
                        }
                    }
                }
            }
            KeyEvent::Copy => {
                if IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
                    log_warn!("SIGINT: Process interrupted by user via Ctrl+C.");
                    IS_RUNNING_PROGRAM.store(false, Ordering::SeqCst);
                } else {
                    let text_to_copy = get_selected_text().unwrap_or_else(|| {
                        (*addr_of_mut!(INPUT_BUFFER)).as_ref().unwrap().clone()
                    });

                    if let Some(clip) = &mut *addr_of_mut!(CLIPBOARD) {
                        *clip = text_to_copy;
                        log_info!("[Clipboard]: Copied '{}'", clip);
                    }
                }
            }
            KeyEvent::Paste => {
                if !IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
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
            }
            KeyEvent::Cut => {
                if !IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
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
            }
            KeyEvent::UpArrow => {
                if !IS_RUNNING_PROGRAM.load(Ordering::Relaxed) {
                    if let Some(hist) = &*addr_of_mut!(HISTORY) {
                        if !hist.is_empty() && *addr_of_mut!(HISTORY_INDEX) > 0 {
                            *addr_of_mut!(HISTORY_INDEX) -= 1;
                            let prev_cmd = &hist[*addr_of_mut!(HISTORY_INDEX)];

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
                        let buffer = (*addr_of_mut!(INPUT_BUFFER)).as_mut().unwrap();

                        if *addr_of_mut!(HISTORY_INDEX) + 1 < hist.len() {
                            *addr_of_mut!(HISTORY_INDEX) += 1;
                            let next_cmd = &hist[*addr_of_mut!(HISTORY_INDEX)];

                            buffer.clear();
                            buffer.push_str(next_cmd);
                            *addr_of_mut!(CURSOR_INDEX) = buffer.len();

                            if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                                writer.redraw_line_text(buffer.as_str(), *addr_of_mut!(CURSOR_INDEX));
                            }
                        } else {
                            *addr_of_mut!(HISTORY_INDEX) = hist.len();
                            buffer.clear();
                            *addr_of_mut!(CURSOR_INDEX) = 0;
                            if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                                writer.clear_current_line(5);
                            }
                        }
                    }
                }
            }
            KeyEvent::None => {}
        }

        pic::send_eoi(1);
    }
}

pub fn print_prompt() {
    unsafe {
        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
            writer.write_str("eos> ", 56, 189, 248);
            writer.draw_cursor(true);
        }
        crate::serial_print!("eos> ");
    }
}

fn parse_num(s: &str) -> Option<u64> {
    let clean = s.trim();
    if let Some(hex) = clean.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        clean.parse::<u64>().ok()
    }
}

fn execute_command(cmd: &str) {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return;
    }

    match trimmed {
        "help" => {
            log_info!("Commands: top, ps, lsdisk, catdisk <file>, exec <elf>, ls, cat <file>, clear, history, uptime, sleep, cr3, reboot, panic");
        }
        "top" | "tm" | "tasks" => {
            log_info!("================== EOS TASK & RESOURCE MANAGER ==================");
            
            let kernel_heap_used = HEAP_ALLOCATOR.used();
            let kernel_heap_total = HEAP_SIZE;
            let heap_pct = (kernel_heap_used * 100) / kernel_heap_total;

            log_info!("[MEMORY / RAM]");
            log_info!("  - Physical RAM            : 2048 MB Total Assigned");
            log_info!("  - Kernel Heap Usage       : {} KB / {} KB ({} MB) [{}% Used]", 
                kernel_heap_used / 1024, 
                kernel_heap_total / 1024, 
                kernel_heap_total / (1024 * 1024),
                heap_pct
            );
            log_info!("  - Userspace VMM Buffer    : 512 MB Max Capacity (Ring 3)");

            log_info!("[STORAGE & DISK FOOTPRINT]");
            unsafe {
                if let Some(archive) = &*addr_of_mut!(INITRD) {
                    let mut ramdisk_bytes = 0usize;
                    for f in &archive.files { ramdisk_bytes += f.size; }
                    log_info!("  - Ramdisk (TarFS)         : {} files | Used: {} KB", 
                        archive.files.len(), 
                        ramdisk_bytes / 1024
                    );
                }
            }
            match scan_shared_disk() {
                Ok(files) => {
                    let mut share_bytes = 0u64;
                    for f in &files { share_bytes += f.size as u64; }
                    log_info!("  - Windows Shared Disk     : {} files | Total: {} KB", 
                        files.len(), 
                        share_bytes / 1024
                    );
                }
                Err(_) => {
                    log_info!("  - Windows Shared Disk     : Standby / Port Inactive");
                }
            }

            log_info!("[ACTIVE TASKS & PER-TASK RAM]");
            unsafe {
                if let Some(sched) = &*addr_of_mut!(SCHEDULER) {
                    log_info!("  Scheduler State: {} | Total Tasks: {}", 
                        if sched.preemptive_enabled { "Preemptive (10ms)" } else { "Cooperative" },
                        sched.tasks.len()
                    );
                    log_info!("  {:<3} {:<5} {:<24} {:<10} {:<10}", "ACT", "TID", "NAME", "RAM (STACK)", "TICKS");
                    for (idx, task) in sched.tasks.iter().enumerate() {
                        let active = if idx == sched.current_task_idx { ">>" } else { "  " };
                        let task_ram_kb = TASK_STACK_SIZE / 1024;
                        log_info!("  {:<3} {:<5} {:<24} {} KB     {:<10}", 
                            active, 
                            task.id, 
                            task.name, 
                            task_ram_kb,
                            task.counter
                        );
                    }
                }
            }
            log_info!("=================================================================");
        }
        "clear" => unsafe {
            if let Some(writer) = &mut *addr_of_mut!(WRITER) {
                writer.clear(15, 23, 42);
            }
        },
        "clipboard" => unsafe {
            if let Some(clip) = &*addr_of_mut!(CLIPBOARD) {
                log_info!("Clipboard content: '{}'", clip);
            }
        },
        "history" => unsafe {
            if let Some(hist) = &*addr_of_mut!(HISTORY) {
                log_info!("Command History ({} commands):", hist.len());
                for (i, c) in hist.iter().enumerate() {
                    log_info!("  {:2}: {}", i + 1, c);
                }
            }
        },
        "ls" => unsafe {
            if let Some(archive) = &*addr_of_mut!(INITRD) {
                log_info!("Ramdisk Contents (Total: {} files):", archive.files.len());
                for f in &archive.files {
                    log_info!("  - {} ({} bytes)", f.name, f.size);
                }
            } else {
                log_error!("No Ramdisk mounted.");
            }
        },
        "lsdisk" => {
            log_info!("Scanning Live Shared Disk (from Windows C:\\EOS_SHARE)...");
            match scan_shared_disk() {
                Ok(files) => {
                    log_info!("Files Found on Shared Disk (Total: {}):", files.len());
                    for f in files {
                        log_info!("  [FILE] {:<16} | Size: {:>8} bytes | Cluster: {}", f.name, f.size, f.first_cluster);
                    }
                }
                Err(err) => {
                    log_error!("Failed to scan shared disk: {}", err);
                }
            }
        }
        "ps" => unsafe {
            if let Some(sched) = &*addr_of_mut!(SCHEDULER) {
                log_info!("Tasks (Total: {}, Current: {}, Preempt: {}):", 
                    sched.tasks.len(), 
                    sched.current_task_idx,
                    if sched.preemptive_enabled { "ON" } else { "OFF" }
                );
                for (idx, task) in sched.tasks.iter().enumerate() {
                    let indicator = if idx == sched.current_task_idx { "*" } else { " " };
                    log_info!(" {} [{}] '{}' | Counter: {}", indicator, task.id, task.name, task.counter);
                }
            }
        },
        "start" => {
            crate::task::enable_preemption();
            log_info!("Preemptive scheduler engaged.");
        }
        "yield" => {
            yield_now();
            log_info!("Manual yield completed.");
        }
        "lspci" => unsafe {
            if let Some(devices) = &*addr_of_mut!(PCI_DEVICES) {
                log_info!("PCI Bus Devices (Total: {}):", devices.len());
                for dev in devices {
                    let desc = class_name(dev.class_code, dev.subclass);
                    log_info!(
                        "  [{:02x}:{:02x}.{}] ID {:04x}:{:04x} | {}",
                        dev.bus, dev.device, dev.function, dev.vendor_id, dev.device_id, desc
                    );
                }
            }
        },
        "uptime" => {
            let sec = pit::get_uptime_seconds();
            let ticks = pit::get_ticks();
            log_info!("System Uptime: {}s ({} timer ticks @ 100Hz)", sec, ticks);
        }
        "sleep" => {
            log_info!("Sleeping for 2000ms...");
            pit::sleep_ms(2000);
            log_info!("Awake.");
        }
        "cr3" => {
            let cr3 = crate::mm::paging::read_cr3();
            log_info!("CR3 Register: {:#018x}", cr3);
        }
        "reboot" => {
            log_warn!("Rebooting CPU...");
            unsafe {
                asm!(
                    "2:",
                    "in al, 0x64",
                    "test al, 0x02",
                    "jnz 2b",
                    "mov al, 0xFE",
                    "out 0x64, al",
                    options(nomem, nostack)
                );
            }
        }
        "panic" => {
            panic!("Manual Kernel Panic triggered by user.");
        }
        other => {
            if let Some(elf_name) = other.strip_prefix("exec ") {
                unsafe {
                    if let Some(archive) = &*addr_of_mut!(INITRD) {
                        let target = elf_name.trim();
                        let found = archive.files.iter().find(|f| f.name == target);
                        match found {
                            Some(file) => {
                                let slice = core::slice::from_raw_parts(file.data_ptr, file.size);
                                IS_RUNNING_PROGRAM.store(true, Ordering::SeqCst);
                                match load_and_run_elf(slice) {
                                    Ok(()) => {
                                        log_info!("ELF binary '{}' completed cleanly.", target);
                                    }
                                    Err(err) => {
                                        log_error!("ELF Load Error: {}", err);
                                    }
                                }
                                IS_RUNNING_PROGRAM.store(false, Ordering::SeqCst);
                            }
                            None => {
                                log_error!("ELF file not found: '{}'", target);
                            }
                        }
                    } else {
                        log_error!("No Ramdisk mounted.");
                    }
                }
            } else if let Some(file_name) = other.strip_prefix("cat ") {
                unsafe {
                    if let Some(archive) = &*addr_of_mut!(INITRD) {
                        match archive.read_file(file_name.trim()) {
                            Some(content) => {
                                log_info!("--- [{}] ---", file_name.trim());
                                for line in content.lines() {
                                    log_info!("{}", line);
                                }
                                log_info!("--- [EOF] ---");
                            }
                            None => {
                                log_error!("File not found in Ramdisk: '{}'", file_name.trim());
                            }
                        }
                    } else {
                        log_error!("No Ramdisk mounted.");
                    }
                }
            } else if let Some(target_file) = other.strip_prefix("catdisk ") {
                let target = target_file.trim();
                match scan_shared_disk() {
                    Ok(files) => {
                        if let Some(file) = files.iter().find(|f| f.name.eq_ignore_ascii_case(target)) {
                            log_info!("Reading '{}' ({} bytes) from shared disk...", file.name, file.size);
                            let mut buf = [0u8; 512];
                            let mut sector_data = [0u8; 512];
                            let _ = read_sector_drive(1, 0, &mut sector_data);
                            let reserved = u16::from_le_bytes([sector_data[14], sector_data[15]]) as u32;
                            let num_fats = sector_data[16] as u32;
                            let mut fat_size = u16::from_le_bytes([sector_data[22], sector_data[23]]) as u32;
                            if fat_size == 0 {
                                fat_size = u32::from_le_bytes([sector_data[36], sector_data[37], sector_data[38], sector_data[39]]);
                            }
                            let root_entries = u16::from_le_bytes([sector_data[17], sector_data[18]]) as u32;
                            let root_sectors = ((root_entries * 32) + 511) / 512;
                            let spc = sector_data[13] as u32;
                            let data_start = reserved + (num_fats * fat_size) + root_sectors;
                            let cluster_lba = data_start + ((file.first_cluster.saturating_sub(2)) * spc);

                            if read_sector_drive(1, cluster_lba, &mut buf).is_ok() {
                                let display_len = (file.size as usize).min(512);
                                if let Ok(s) = core::str::from_utf8(&buf[..display_len]) {
                                    log_info!("--- [{}] ---", file.name);
                                    for line in s.lines() {
                                        log_info!("{}", line);
                                    }
                                    log_info!("--- [EOF] ---");
                                } else {
                                    log_warn!("Binary preview:");
                                    log_info!("{:02x?}", &buf[..display_len.min(64)]);
                                }
                            }
                        } else {
                            log_error!("File '{}' not found on shared disk.", target);
                        }
                    }
                    Err(e) => log_error!("Shared disk error: {}", e),
                }
            } else if let Some(text) = other.strip_prefix("echo ") {
                log_info!("{}", text);
            } else {
                log_error!("Unknown command: '{}'. Type 'help'.", other);
            }
        }
    }
}

pub extern "x86-interrupt" fn divide_by_zero_handler(stack_frame: InterruptStackFrame) {
    log_error!("CPU EXCEPTION: DIVIDE BY ZERO (Vector 0)");
    log_error!("  Faulting RIP: {:#018x}", stack_frame.instruction_pointer);
}

pub extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    log_warn!("CPU EXCEPTION: BREAKPOINT (INT 3) HIT");
    log_debug!("  Trap Instruction at: {:#018x}", stack_frame.instruction_pointer);
}

pub extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    log_error!("CPU EXCEPTION: INVALID OPCODE (#UD)");
    log_error!("  RIP: {:#018x} | RSP: {:#018x} | CS: {:#x}", stack_frame.instruction_pointer, stack_frame.stack_pointer, stack_frame.code_segment);
}

pub extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    log_error!("CPU EXCEPTION: GENERAL PROTECTION FAULT (#GP)");
    log_error!("  Error Code: {:#x}", error_code);
    log_error!("  RIP: {:#018x} | RSP: {:#018x} | CS: {:#x}", stack_frame.instruction_pointer, stack_frame.stack_pointer, stack_frame.code_segment);
}

pub extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    log_error!("CRITICAL CPU FAILURE: DOUBLE FAULT (#DF)");
    log_error!("  RIP: {:#018x} | RSP: {:#018x}", stack_frame.instruction_pointer, stack_frame.stack_pointer);
    loop {
        core::hint::spin_loop();
    }
}

pub extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    let cr2: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
    }

    log_error!("================ CPU PAGE FAULT DIAGNOSTICS ================");
    log_error!("Target Address (CR2): {:#018x}", cr2);
    log_error!("Instruction RIP:      {:#018x}", stack_frame.instruction_pointer);
    log_error!("Stack Pointer (RSP):  {:#018x}", stack_frame.stack_pointer);
    log_error!("Code Segment (CS):    {:#04x}", stack_frame.code_segment);
    log_error!("Raw Error Code:       {:#010b}", error_code);
    log_error!("============================================================");

    if (error_code & (1 << 2)) != 0 {
        unsafe {
            let saved_rsp = crate::arch::x86_64::syscall::KERNEL_SAVED_RSP;
            if saved_rsp != 0 {
                asm!(
                    "mov rsp, {sp}",
                    "pop r15",
                    "pop r14",
                    "pop r13",
                    "pop r12",
                    "pop rbx",
                    "pop rbp",
                    "sti",
                    "ret",
                    sp = in(reg) saved_rsp,
                    options(noreturn)
                );
            }
        }
    }

    loop {
        core::hint::spin_loop();
    }
}
