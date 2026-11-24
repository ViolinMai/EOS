#![allow(unused_unsafe)]
use crate::arch::x86_64::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};
use crate::arch::x86_64::keyboard::clear_keyboard_buffer;
use crate::arch::x86_64::syscall::{ELF_EXIT_REQUESTED, reset_core2_process};
use crate::log_info;
use crate::mm::paging::{read_cr3, free_user_pages, PageTable, VirtualMemoryManager, VMM};
use alloc::vec::Vec;
use core::arch::naked_asm;
use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;

pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const ET_DYN: u16 = 3;
pub const PT_LOAD: u32 = 1;
pub const PT_INTERP: u32 = 3;
pub const PT_PHDR: u32 = 6;
pub const MAIN_PIE_BASE: u64 = 0x0000_0000_4000_0000;
pub const INTERP_BASE: u64 = 0x0000_7F00_0000_0000;
pub const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    pub magic: [u8; 4], pub class: u8, pub endianness: u8, pub version: u8,
    pub os_abi: u8, pub abi_version: u8, pub padding: [u8; 7], pub elf_type: u16,
    pub machine: u16, pub version2: u32, pub entry: u64, pub phoff: u64,
    pub shoff: u64, pub flags: u32, pub ehsize: u16, pub phentsize: u16,
    pub phnum: u16, pub shentsize: u16, pub shnum: u16, pub shstrndx: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64ProgramHeader {
    pub p_type: u32, pub p_flags: u32, pub p_offset: u64, pub p_vaddr: u64,
    pub p_paddr: u64, pub p_filesz: u64, pub p_memsz: u64, pub p_align: u64,
}

#[unsafe(naked)]
pub extern "C" fn jump_to_ring3(entry: u64, rsp: u64, user_cs: u64, user_ds: u64, spawn_rsp_ptr: *mut u64, new_cr3: u64) {
    naked_asm!(
        "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
        "mov [r8], rsp",
        "mov cr3, r9",
        "mov ax, cx",
        "mov ds, ax", "mov es, ax", "mov fs, ax", "mov gs, ax",
        "push rcx",         // SS = user_ds
        "push rsi",         // RSP = user_rsp
        "push 0x202",       // RFLAGS (IF = 1)
        "push rdx",         // CS = user_cs
        "push rdi",         // RIP = entry
        "xor rax, rax",
        "xor rbx, rbx",
        "xor rcx, rcx",
        "xor rdx, rdx",
        "xor rsi, rsi",
        "xor rbp, rbp",
        "xor r8, r8",
        "xor r9, r9",
        "xor r10, r10",
        "xor r11, r11",
        "xor r12, r12",
        "xor r13, r13",
        "xor r14, r14",
        "xor r15, r15",
        "xor rdi, rdi",
        "iretq"
    );
}

fn map_elf_segments(
    data: &[u8],
    header: &Elf64Header,
    new_pml4_phys: u64,
    new_pml4: &PageTable,
    vmm: &VirtualMemoryManager,
    load_bias: u64,
) -> Result<u64, &'static str> {
    let phoff = header.phoff as usize;
    let phnum = header.phnum as usize;
    let phentsize = header.phentsize as usize;
    let mut phdr_vaddr: u64 = 0;

    for i in 0..phnum {
        let ph_offset = phoff + (i * phentsize);
        if ph_offset + core::mem::size_of::<Elf64ProgramHeader>() > data.len() {
            return Err("Program header out of bounds");
        }
        let ph = unsafe { *(data.as_ptr().add(ph_offset) as *const Elf64ProgramHeader) };

        if ph.p_type == PT_PHDR {
            phdr_vaddr = ph.p_vaddr.wrapping_add(load_bias);
        }

        if ph.p_type == PT_LOAD {
            let vaddr = ph.p_vaddr.wrapping_add(load_bias);
            let mem_sz = ph.p_memsz as usize;
            let file_sz = ph.p_filesz as usize;
            let file_off = ph.p_offset as usize;

            if file_off.saturating_add(file_sz) > data.len() {
                return Err("PT_LOAD segment exceeds bounds");
            }
            if phdr_vaddr == 0 && (header.phoff >= ph.p_offset) && (header.phoff < ph.p_offset + ph.p_filesz) {
                phdr_vaddr = vaddr + (header.phoff - ph.p_offset);
            }

            let start_page = vaddr & !0xFFF;
            let end_page = (vaddr + mem_sz as u64 + 0xFFF) & !0xFFF;
            crate::mm::paging::map_user_pages(new_pml4_phys, start_page, ((end_page - start_page) / 4096) as usize)?;

            unsafe {
                let mut remaining = file_sz;
                let mut data_offset = file_off;
                let mut dest_vaddr = vaddr;
                while remaining > 0 {
                    let p4_idx = ((dest_vaddr >> 39) & 0x1FF) as usize;
                    let p3_idx = ((dest_vaddr >> 30) & 0x1FF) as usize;
                    let p2_idx = ((dest_vaddr >> 21) & 0x1FF) as usize;
                    let p1_idx = ((dest_vaddr >> 12) & 0x1FF) as usize;
                    let pt4 = &*((new_pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
                    let pt3 = &*((pt4.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
                    let pt2 = &*((pt3.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
                    let frame_phys = pt2.entries[p1_idx].physical_address();

                    let page_offset = (dest_vaddr & 0xFFF) as usize;
                    let copy_size = core::cmp::min(remaining, 4096 - page_offset);
                    core::ptr::copy_nonoverlapping(
                        data.as_ptr().add(data_offset),
                        ((frame_phys + vmm.hhdm_offset) as *mut u8).add(page_offset),
                        copy_size,
                    );
                    remaining -= copy_size;
                    data_offset += copy_size;
                    dest_vaddr += copy_size as u64;
                }

                if mem_sz > file_sz {
                    let mut bss_remaining = mem_sz - file_sz;
                    let mut bss_vaddr = vaddr + file_sz as u64;
                    while bss_remaining > 0 {
                        let p4_idx = ((bss_vaddr >> 39) & 0x1FF) as usize;
                        let p3_idx = ((bss_vaddr >> 30) & 0x1FF) as usize;
                        let p2_idx = ((bss_vaddr >> 21) & 0x1FF) as usize;
                        let p1_idx = ((bss_vaddr >> 12) & 0x1FF) as usize;
                        let pt4 = &*((new_pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
                        let pt3 = &*((pt4.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
                        let pt2 = &*((pt3.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
                        let frame_phys = pt2.entries[p1_idx].physical_address();

                        let page_offset = (bss_vaddr & 0xFFF) as usize;
                        let zero_size = core::cmp::min(bss_remaining, 4096 - page_offset);
                        core::ptr::write_bytes(((frame_phys + vmm.hhdm_offset) as *mut u8).add(page_offset), 0, zero_size);
                        bss_remaining -= zero_size;
                        bss_vaddr += zero_size as u64;
                    }
                }
            }
        }
    }

    Ok(phdr_vaddr)
}

fn find_pt_interp(data: &[u8], header: &Elf64Header) -> Option<alloc::string::String> {
    let phoff = header.phoff as usize;
    let phnum = header.phnum as usize;
    let phentsize = header.phentsize as usize;
    for i in 0..phnum {
        let ph_offset = phoff + (i * phentsize);
        if ph_offset + core::mem::size_of::<Elf64ProgramHeader>() > data.len() { break; }
        let ph = unsafe { *(data.as_ptr().add(ph_offset) as *const Elf64ProgramHeader) };
        if ph.p_type == PT_INTERP {
            let off = ph.p_offset as usize;
            let sz = ph.p_filesz as usize;
            if off.saturating_add(sz) <= data.len() && sz > 0 {
                let raw = &data[off..off + sz];
                let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                if let Ok(s) = core::str::from_utf8(&raw[..end]) {
                    return Some(alloc::string::String::from(s.trim()));
                }
            }
        }
    }
    None
}

fn load_interpreter_bytes(interp_path: &str) -> Result<Vec<u8>, &'static str> {
    if let Ok(b) = crate::fs::vfs_read_bytes(interp_path) {
        return Ok(b);
    }
    let base = interp_path.rsplit('/').next().unwrap_or(interp_path);
    let cands = [
        alloc::format!("EOS SHARE/lib/{}", base),
        alloc::format!("r/lib/{}", base),
        alloc::string::String::from(base),
    ];
    for c in &cands {
        if let Ok(b) = crate::fs::vfs_read_bytes(c) {
            return Ok(b);
        }
    }
    Err("PT_INTERP dynamic linker (ld-musl-x86_64.so.1) not found in VFS")
}

pub fn load_and_run_elf(data: &[u8], args_str: &str) -> Result<(), &'static str> {
    log_info!("ELF", "Validating binary headers (Size: {} bytes)...", data.len());
    if data.len() < core::mem::size_of::<Elf64Header>() { return Err("Binary too small for ELF Header"); }
    let header = unsafe { *(data.as_ptr() as *const Elf64Header) };
    if header.magic != ELF_MAGIC { return Err("Invalid ELF magic identifier"); }
    if header.class != 2 || header.machine != 0x3E { return Err("Binary is not x86_64 64-bit"); }

    reset_core2_process();

    let main_bias: u64 = if header.elf_type == ET_DYN { MAIN_PIE_BASE } else { 0 };
    let main_entry = header.entry.wrapping_add(main_bias);
    let phnum = header.phnum as usize;
    let phentsize = header.phentsize as usize;
    log_info!("ELF", "ELF Validated: Entry point {:#018x}, Program Headers: {}", main_entry, phnum);

    let vmm = unsafe { (&*addr_of_mut!(VMM)).as_ref().ok_or("VMM not initialized")? };
    let kernel_cr3 = read_cr3() & 0x000F_FFFF_FFFF_F000;
    let new_pml4_phys = crate::mm::frame::allocate_frame_safe().ok_or("OOM: PML4")?;
    let new_pml4 = unsafe { &mut *((new_pml4_phys + vmm.hhdm_offset) as *mut PageTable) };
    let current_pml4 = unsafe { &*((kernel_cr3 + vmm.hhdm_offset) as *const PageTable) };

    unsafe {
        core::ptr::write_bytes(new_pml4 as *mut PageTable as *mut u8, 0, 4096);
        for i in 256..512 { new_pml4.entries[i] = current_pml4.entries[i]; }
    }

    log_info!("ELF", "Mapping program segments into new address space with BSS zeroing...");
    let main_phdr_vaddr = map_elf_segments(data, &header, new_pml4_phys, new_pml4, vmm, main_bias)?;

    // Milestone 38: Dynamic ELF Loader (PT_INTERP -> ld-musl-x86_64.so.1)
    let mut at_base: u64 = 0;
    let mut jump_entry: u64 = main_entry;
    if let Some(interp_path) = find_pt_interp(data, &header) {
        log_info!("ELF", "Dynamic ELF detected (PT_INTERP: '{}'), loading dynamic linker...", interp_path);
        let interp_bytes = load_interpreter_bytes(&interp_path)?;
        if interp_bytes.len() < core::mem::size_of::<Elf64Header>() {
            return Err("Interpreter ELF too small");
        }
        let interp_hdr = unsafe { *(interp_bytes.as_ptr() as *const Elf64Header) };
        if interp_hdr.magic != ELF_MAGIC {
            return Err("Invalid interpreter ELF magic");
        }
        let interp_bias = if interp_hdr.elf_type == ET_DYN { INTERP_BASE } else { 0 };
        let _ = map_elf_segments(&interp_bytes, &interp_hdr, new_pml4_phys, new_pml4, vmm, interp_bias)?;
        at_base = interp_bias;
        jump_entry = interp_hdr.entry.wrapping_add(interp_bias);
        log_info!("ELF", "Dynamic linker mapped at {:#018x}, jumping to {:#018x}", at_base, jump_entry);
        crate::audit::emit_raw_json("DYN_ELF", "PT_INTERP_LOAD", "PASS", "DYN_ELF:PASS:Dlopen_Ready");
    }

    let stack_pages = 512;
    let stack_start = USER_STACK_TOP - (stack_pages * 4096);
    crate::mm::paging::map_user_pages(new_pml4_phys, stack_start, stack_pages as usize)?;

    let tls_vaddr = 0x00007fffffff0000u64;
    crate::mm::paging::map_user_pages(new_pml4_phys, tls_vaddr, 2)?;

    unsafe {
        let p4_idx = ((tls_vaddr >> 39) & 0x1FF) as usize;
        let p3_idx = ((tls_vaddr >> 30) & 0x1FF) as usize;
        let p2_idx = ((tls_vaddr >> 21) & 0x1FF) as usize;
        let p1_idx = ((tls_vaddr >> 12) & 0x1FF) as usize;
        let pt4 = &*((new_pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let pt3 = &*((pt4.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let pt2 = &*((pt3.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let tls_base = (pt2.entries[p1_idx].physical_address() + vmm.hhdm_offset) as *mut u64;
        core::ptr::write_bytes(tls_base as *mut u8, 0, 4096);
        *tls_base = tls_vaddr;
    }

    let mut args: Vec<alloc::string::String> = Vec::new();
    if args_str.is_empty() {
        args.push(alloc::string::String::from("eos_desktop"));
    } else {
        let mut cur = alloc::string::String::new();
        let mut in_quotes = false;
        let mut quote_ch = '"';
        for ch in args_str.chars() {
            if in_quotes {
                if ch == quote_ch {
                    in_quotes = false;
                } else {
                    cur.push(ch);
                }
            } else if ch == '"' || ch == '\'' {
                in_quotes = true;
                quote_ch = ch;
            } else if ch.is_whitespace() {
                if !cur.is_empty() {
                    args.push(core::mem::take(&mut cur));
                }
            } else {
                cur.push(ch);
            }
        }
        if !cur.is_empty() {
            args.push(cur);
        }
    }

    let default_envs = [
        "PATH=/EOS SHARE/bin:/r/bin:/bin:/usr/bin",
        "LD_LIBRARY_PATH=/EOS SHARE/lib:/r/lib:/lib:/usr/lib",
        "PYTHONHOME=/EOS SHARE",
        "PYTHONPATH=/EOS SHARE/python_stdlib:/EOS SHARE/lib/python3.12/lib-dynload:/EOS SHARE/python_libs:/r/lib/python3.12/site-packages",
        "PYTHONUNBUFFERED=1",
        "TERM=xterm-256color",
    ];

    let user_rsp_page = USER_STACK_TOP - 0x2000;
    let user_rsp;

    unsafe {
        let p4_idx = ((user_rsp_page >> 39) & 0x1FF) as usize;
        let p3_idx = ((user_rsp_page >> 30) & 0x1FF) as usize;
        let p2_idx = ((user_rsp_page >> 21) & 0x1FF) as usize;
        let p1_idx = ((user_rsp_page >> 12) & 0x1FF) as usize;
        let pt4 = &*((new_pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let pt3 = &*((pt4.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let pt2 = &*((pt3.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let stack_base = (pt2.entries[p1_idx].physical_address() + vmm.hhdm_offset) as *mut u8;

        core::ptr::write_bytes(stack_base, 0, 4096);

        // Seed 16 bytes of AT_RANDOM entropy at user_rsp_page + 0x100
        let mut rng = crate::arch::x86_64::pit::read_tsc() ^ 0xA5A5_5A5A_1234_5678;
        for i in 0..16usize {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            *stack_base.add(0x100 + i) = ((rng >> 32) & 0xFF) as u8;
        }

        let mut string_cursor = 4096 - 16;
        let mut arg_pointers: Vec<u64> = Vec::new();
        for arg in &args {
            let bytes = arg.as_bytes();
            string_cursor -= bytes.len() + 1;
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), stack_base.add(string_cursor), bytes.len());
            *stack_base.add(string_cursor + bytes.len()) = 0;
            arg_pointers.push(user_rsp_page + string_cursor as u64);
        }

        let mut env_pointers: Vec<u64> = Vec::new();
        for env in &default_envs {
            let bytes = env.as_bytes();
            string_cursor -= bytes.len() + 1;
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), stack_base.add(string_cursor), bytes.len());
            *stack_base.add(string_cursor + bytes.len()) = 0;
            env_pointers.push(user_rsp_page + string_cursor as u64);
        }

        let mut entries: Vec<u64> = Vec::new();
        entries.push(arg_pointers.len() as u64);
        for ptr in arg_pointers {
            entries.push(ptr);
        }
        entries.push(0);

        for ptr in env_pointers {
            entries.push(ptr);
        }
        entries.push(0);

        // Auxiliary Vector (System V AMD64 ABI) — Full Dynamic Linker & Root Support
        entries.push(3);  entries.push(main_phdr_vaddr);     // AT_PHDR
        entries.push(4);  entries.push(phentsize as u64);    // AT_PHENT
        entries.push(5);  entries.push(phnum as u64);        // AT_PHNUM
        entries.push(6);  entries.push(4096);                // AT_PAGESZ
        entries.push(7);  entries.push(at_base);             // AT_BASE
        entries.push(9);  entries.push(main_entry);          // AT_ENTRY
        entries.push(11); entries.push(0);                   // AT_UID
        entries.push(12); entries.push(0);                   // AT_EUID
        entries.push(13); entries.push(0);                   // AT_GID
        entries.push(14); entries.push(0);                   // AT_EGID
        entries.push(17); entries.push(100);                 // AT_CLKTCK
        entries.push(23); entries.push(0);                   // AT_SECURE
        entries.push(25); entries.push(user_rsp_page + 0x100); // AT_RANDOM
        entries.push(0);  entries.push(0);                   // AT_NULL

        let total_words = entries.len();
        let target_offset = 0x800usize;
        let stack_dst = stack_base.add(target_offset) as *mut u64;
        core::ptr::copy_nonoverlapping(entries.as_ptr(), stack_dst, total_words);

        user_rsp = user_rsp_page + target_offset as u64;
    }

    clear_keyboard_buffer();
    ELF_EXIT_REQUESTED.store(false, Ordering::SeqCst);
    let core_id = crate::profiler::get_core_id();
    let state_ptr = unsafe { addr_of_mut!(crate::arch::x86_64::syscall::CORE_SYSCALL_STATES[core_id]) };
    let spawn_rsp_ptr = unsafe { &mut (*state_ptr).kernel_spawn_rsp as *mut u64 };

    log_info!("ELF", "Jumping to Ring 3 userspace entry {:#018x} with RSP {:#018x} (Aligned 16: {}) on Core {}...",
        jump_entry, user_rsp, (user_rsp % 16) == 0, core_id);

    unsafe {
        crate::arch::x86_64::fpu::enable_sse();
        let mxcsr: u32 = 0x1F80;
        core::arch::asm!("ldmxcsr [{}]", in(reg) &mxcsr, options(nostack));

        crate::arch::x86_64::syscall::wrmsr(0xC0000100, tls_vaddr);
        crate::arch::x86_64::syscall::wrmsr(0xC0000101, 0);

        jump_to_ring3(jump_entry, user_rsp, USER_CODE_SELECTOR as u64, USER_DATA_SELECTOR as u64, spawn_rsp_ptr, new_pml4_phys);
        core::arch::asm!("mov cr3, {}", in(reg) kernel_cr3);
        free_user_pages(new_pml4_phys);
    }
    log_info!("ELF", "Ring 3 process returned cleanly to kernel.");
    Ok(())
}
