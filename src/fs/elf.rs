#![allow(unused_unsafe)]
use crate::arch::x86_64::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};
use crate::arch::x86_64::keyboard::clear_keyboard_buffer;
use crate::arch::x86_64::syscall::{ELF_EXIT_REQUESTED, reset_core2_process};
use crate::{log_info, log_debug};
use crate::mm::paging::{invalidate_tlb, read_cr3, free_user_pages, PageTable, PAGE_PRESENT, PAGE_USER, PAGE_WRITABLE};
use crate::mm::paging::VMM;
use alloc::vec::Vec;
use core::arch::naked_asm;
use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;

pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const PT_LOAD: u32 = 1;
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
extern "C" fn jump_to_ring3(entry: u64, rsp: u64, user_cs: u64, user_ds: u64, spawn_rsp_ptr: *mut u64, new_cr3: u64) {
    naked_asm!(
        "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
        "mov [r8], rsp", "mov cr3, r9", 
        "mov ax, cx", "mov ds, ax", "mov es, ax", "mov fs, ax", "mov gs, ax",
        "push rcx", "push rsi", "push 0x202", "push rdx", "push rdi", "iretq",
    );
}

pub fn load_and_run_elf(data: &[u8], args_str: &str) -> Result<(), &'static str> {
    if data.len() < core::mem::size_of::<Elf64Header>() { return Err("Binary too small for ELF Header"); }
    let header = unsafe { *(data.as_ptr() as *const Elf64Header) };
    if header.magic != ELF_MAGIC { return Err("Invalid ELF magic identifier"); }
    if header.class != 2 || header.machine != 0x3E { return Err("Binary is not x86_64 64-bit"); }

    reset_core2_process();

    let entry_point = header.entry;
    let phoff = header.phoff as usize;
    let phnum = header.phnum as usize;
    let phentsize = header.phentsize as usize;
    log_info!("ELF", "Loading ELF Binary with Strict Isolation (New CR3).");

    let vmm = unsafe { (&*addr_of_mut!(VMM)).as_ref().ok_or("VMM not initialized")? };
    let kernel_cr3 = read_cr3() & 0x000F_FFFF_FFFF_F000;
    let new_pml4_phys = crate::mm::frame::allocate_frame_safe().ok_or("OOM: PML4")?;
    let new_pml4 = unsafe { &mut *((new_pml4_phys + vmm.hhdm_offset) as *mut PageTable) };
    let current_pml4 = unsafe { &*((kernel_cr3 + vmm.hhdm_offset) as *const PageTable) };

    unsafe {
        core::ptr::write_bytes(new_pml4 as *mut PageTable as *mut u8, 0, 4096);
        for i in 256..512 { new_pml4.entries[i] = current_pml4.entries[i]; }
    }

    for i in 0..phnum {
        let ph_offset = phoff + (i * phentsize);
        if ph_offset + core::mem::size_of::<Elf64ProgramHeader>() > data.len() { return Err("Program header out of bounds"); }
        let ph = unsafe { *(data.as_ptr().add(ph_offset) as *const Elf64ProgramHeader) };

        if ph.p_type == PT_LOAD {
            let vaddr = ph.p_vaddr; let mem_sz = ph.p_memsz as usize;
            let file_sz = ph.p_filesz as usize; let file_off = ph.p_offset as usize;

            if file_off.saturating_add(file_sz) > data.len() { return Err("PT_LOAD segment exceeds bounds"); }
            let start_page = vaddr & !0xFFF; let end_page = (vaddr + mem_sz as u64 + 0xFFF) & !0xFFF;
            crate::mm::paging::map_user_pages(new_pml4_phys, start_page, ((end_page - start_page) / 4096) as usize)?;

            unsafe {
                let mut remaining = file_sz; let mut data_offset = file_off; let mut dest_vaddr = vaddr;
                while remaining > 0 {
                    let p4_idx = ((dest_vaddr >> 39) & 0x1FF) as usize; let p3_idx = ((dest_vaddr >> 30) & 0x1FF) as usize;
                    let p2_idx = ((dest_vaddr >> 21) & 0x1FF) as usize; let p1_idx = ((dest_vaddr >> 12) & 0x1FF) as usize;
                    let pt4 = &*((new_pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
                    let pt3 = &*((pt4.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
                    let pt2 = &*((pt3.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
                    let frame_phys = pt2.entries[p1_idx].physical_address();

                    let page_offset = (dest_vaddr & 0xFFF) as usize;
                    let copy_size = core::cmp::min(remaining, 4096 - page_offset);
                    core::ptr::copy_nonoverlapping(data.as_ptr().add(data_offset), ((frame_phys + vmm.hhdm_offset) as *mut u8).add(page_offset), copy_size);
                    remaining -= copy_size; data_offset += copy_size; dest_vaddr += copy_size as u64;
                }
            }
        }
    }

    let stack_pages = 512;
    let stack_start = USER_STACK_TOP - (stack_pages * 4096);
    crate::mm::paging::map_user_pages(new_pml4_phys, stack_start, stack_pages as usize)?;

    let mut args: Vec<&str> = Vec::new();
    if args_str.is_empty() { args.push("eos_app"); } else { for part in args_str.split_whitespace() { args.push(part); } }
    let user_rsp_page = USER_STACK_TOP - 0x2000;
    let user_rsp;

    unsafe {
        let p4_idx = ((user_rsp_page >> 39) & 0x1FF) as usize; let p3_idx = ((user_rsp_page >> 30) & 0x1FF) as usize;
        let p2_idx = ((user_rsp_page >> 21) & 0x1FF) as usize; let p1_idx = ((user_rsp_page >> 12) & 0x1FF) as usize;
        let pt4 = &*((new_pml4.entries[p4_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let pt3 = &*((pt4.entries[p3_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let pt2 = &*((pt3.entries[p2_idx].physical_address() + vmm.hhdm_offset) as *const PageTable);
        let stack_base = (pt2.entries[p1_idx].physical_address() + vmm.hhdm_offset) as *mut u8;
        
        let mut string_cursor = 4096 - 512;
        let mut arg_pointers: Vec<u64> = Vec::new();
        for arg in &args {
            let bytes = arg.as_bytes(); string_cursor -= bytes.len() + 1;
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), stack_base.add(string_cursor), bytes.len());
            *stack_base.add(string_cursor + bytes.len()) = 0;
            arg_pointers.push(user_rsp_page + string_cursor as u64);
        }

        let stack_ptr = stack_base.add(0x800) as *mut u64; let mut idx = 0isize;
        *stack_ptr.offset(idx) = arg_pointers.len() as u64; idx += 1;
        for ptr in arg_pointers { *stack_ptr.offset(idx) = ptr; idx += 1; }
        *stack_ptr.offset(idx) = 0; idx += 1; // argv null
        *stack_ptr.offset(idx) = 0; idx += 1; // envp null
        *stack_ptr.offset(idx) = 6; idx += 1; *stack_ptr.offset(idx) = 4096; idx += 1; // AT_PAGESZ
        *stack_ptr.offset(idx) = 25; idx += 1; *stack_ptr.offset(idx) = user_rsp_page + 0x100; idx += 1; // AT_RANDOM
        *stack_ptr.offset(idx) = 9; idx += 1; *stack_ptr.offset(idx) = entry_point; idx += 1; // AT_ENTRY
        *stack_ptr.offset(idx) = 0; idx += 1; *stack_ptr.offset(idx) = 0;
        user_rsp = (user_rsp_page + 0x800) & !0x0F;
    }

    clear_keyboard_buffer();
    ELF_EXIT_REQUESTED.store(false, Ordering::SeqCst);
    let core_id = crate::profiler::get_core_id();
    let state_ptr = unsafe { addr_of_mut!(crate::arch::x86_64::syscall::CORE_SYSCALL_STATES[core_id]) };
    let spawn_rsp_ptr = unsafe { &mut (*state_ptr).kernel_spawn_rsp as *mut u64 };

    unsafe {
        crate::arch::x86_64::syscall::wrmsr(0xC0000100, 0); crate::arch::x86_64::syscall::wrmsr(0xC0000101, 0);
        jump_to_ring3(entry_point, user_rsp, USER_CODE_SELECTOR as u64, USER_DATA_SELECTOR as u64, spawn_rsp_ptr, new_pml4_phys);
        core::arch::asm!("mov cr3, {}", in(reg) kernel_cr3);
        free_user_pages(new_pml4_phys);
    }
    Ok(())
}
