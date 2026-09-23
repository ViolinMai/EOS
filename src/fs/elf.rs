#![allow(unused_unsafe)]
use crate::arch::x86_64::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};
use crate::arch::x86_64::keyboard::clear_keyboard_buffer;
use crate::arch::x86_64::syscall::ELF_EXIT_REQUESTED;
use crate::log_info;
use crate::mm::paging::{invalidate_tlb, read_cr3, PageTable, PAGE_PRESENT, PAGE_USER, PAGE_WRITABLE};
use crate::mm::paging::VMM;
use core::arch::naked_asm;
use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;

pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const PT_LOAD: u32 = 1;

pub const USER_STACK_TOP: u64 = 0x0000_0000_2040_0000;
pub const USER_IPC_PAGE: u64  = 0x0000_0000_2050_0000;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    pub magic: [u8; 4],
    pub class: u8,
    pub endianness: u8,
    pub version: u8,
    pub os_abi: u8,
    pub abi_version: u8,
    pub padding: [u8; 7],
    pub elf_type: u16,
    pub machine: u16,
    pub version2: u32,
    pub entry: u64,
    pub phoff: u64,
    pub shoff: u64,
    pub flags: u32,
    pub ehsize: u16,
    pub phentsize: u16,
    pub phnum: u16,
    pub shentsize: u16,
    pub shnum: u16,
    pub shstrndx: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

#[unsafe(naked)]
extern "C" fn jump_to_ring3(entry: u64, rsp: u64, user_cs: u64, user_ds: u64) {
    naked_asm!(
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "mov [{kernel_sp}], rsp",

        "push rcx",       // SS (User Data)
        "push rsi",       // RSP (User Stack)
        "push 0x202",     // RFLAGS (IF enabled)
        "push rdx",       // CS (User Code)
        "push rdi",       // RIP (User Entry Point)

        "swapgs",         // 💡 ضرورية جداً عند التحول إلى Ring 3
        "iretq",
        kernel_sp = sym crate::arch::x86_64::syscall::KERNEL_SAVED_RSP,
    );
}

pub fn load_and_run_elf(data: &[u8], arg: &str) -> Result<(), &'static str> {
    if data.len() < core::mem::size_of::<Elf64Header>() {
        return Err("File smaller than ELF header");
    }

    let header = unsafe { *(data.as_ptr() as *const Elf64Header) };

    if header.magic != ELF_MAGIC {
        return Err("Invalid ELF magic identifier");
    }

    if header.class != 2 || header.machine != 0x3E {
        return Err("Binary is not x86_64 64-bit");
    }

    let phoff = header.phoff as usize;
    let phnum = header.phnum as usize;
    let phentsize = header.phentsize as usize;

    let vmm = unsafe { (&*addr_of_mut!(VMM)).as_ref().ok_or("VMM not initialized")? };
    
    let frame_alloc = unsafe {
        let alloc_ptr = addr_of_mut!(crate::mm::frame::FRAME_ALLOCATOR);
        (*alloc_ptr).as_mut().ok_or("PMM not initialized")?
    };

    log_info!("ELF", "Allocating and mapping physical memory for Userspace...");

    let pdpt_phys = frame_alloc.allocate_frame().ok_or("OOM: Failed to allocate PDPT")?;
    let pd_phys = frame_alloc.allocate_frame().ok_or("OOM: Failed to allocate PD")?;

    let pdpt = unsafe { &mut *((pdpt_phys + vmm.hhdm_offset) as *mut PageTable) };
    let pd = unsafe { &mut *((pd_phys + vmm.hhdm_offset) as *mut PageTable) };

    unsafe {
        core::ptr::write_bytes(pdpt as *mut PageTable as *mut u8, 0, 4096);
        core::ptr::write_bytes(pd as *mut PageTable as *mut u8, 0, 4096);
    }

    let user_flags = PAGE_PRESENT | PAGE_WRITABLE | PAGE_USER;

    let pml4_phys = read_cr3() & 0x000F_FFFF_FFFF_F000;
    let pml4 = unsafe { &mut *((pml4_phys + vmm.hhdm_offset) as *mut PageTable) };
    pml4.entries[0].set(pdpt_phys, user_flags);
    pdpt.entries[0].set(pd_phys, user_flags);

    for i in 0..phnum {
        let ph_offset = phoff + (i * phentsize);
        if ph_offset + core::mem::size_of::<Elf64ProgramHeader>() > data.len() {
            return Err("Program header out of bounds");
        }

        let ph = unsafe { *(data.as_ptr().add(ph_offset) as *const Elf64ProgramHeader) };

        if ph.p_type == PT_LOAD {
            let vaddr = ph.p_vaddr;
            let mem_sz = ph.p_memsz as usize;
            let file_sz = ph.p_filesz as usize;
            let file_off = ph.p_offset as usize;

            let start_page = vaddr & !0xFFF;
            let end_page = (vaddr + mem_sz as u64 + 0xFFF) & !0xFFF;
            
            let mut curr_vaddr = start_page;
            while curr_vaddr < end_page {
                let p2_idx = ((curr_vaddr >> 21) & 0x1FF) as usize;
                let p1_idx = ((curr_vaddr >> 12) & 0x1FF) as usize;

                if !pd.entries[p2_idx].is_present() {
                    let pt_phys = frame_alloc.allocate_frame().ok_or("OOM: PT")?;
                    let pt = unsafe { &mut *((pt_phys + vmm.hhdm_offset) as *mut PageTable) };
                    unsafe { core::ptr::write_bytes(pt as *mut PageTable as *mut u8, 0, 4096); }
                    pd.entries[p2_idx].set(pt_phys, user_flags);
                }

                let pt_phys = pd.entries[p2_idx].physical_address();
                let pt = unsafe { &mut *((pt_phys + vmm.hhdm_offset) as *mut PageTable) };

                if !pt.entries[p1_idx].is_present() {
                    let frame_phys = frame_alloc.allocate_frame().ok_or("OOM: User Frame")?;
                    let frame_ptr = (frame_phys + vmm.hhdm_offset) as *mut u8;
                    unsafe { core::ptr::write_bytes(frame_ptr, 0, 4096); }
                    pt.entries[p1_idx].set(frame_phys, user_flags);
                    unsafe { invalidate_tlb(curr_vaddr); }
                }
                curr_vaddr += 4096;
            }

            unsafe {
                let mut remaining = file_sz;
                let mut data_offset = file_off;
                let mut dest_vaddr = vaddr;

                while remaining > 0 {
                    let p2_idx = ((dest_vaddr >> 21) & 0x1FF) as usize;
                    let p1_idx = ((dest_vaddr >> 12) & 0x1FF) as usize;

                    let pt_phys = pd.entries[p2_idx].physical_address();
                    let pt = &*((pt_phys + vmm.hhdm_offset) as *const PageTable);
                    let frame_phys = pt.entries[p1_idx].physical_address();

                    let page_offset = (dest_vaddr & 0xFFF) as usize;
                    let copy_size = core::cmp::min(remaining, 4096 - page_offset);

                    let dest_ptr = (frame_phys + vmm.hhdm_offset) as *mut u8;
                    core::ptr::copy_nonoverlapping(
                        data.as_ptr().add(data_offset),
                        dest_ptr.add(page_offset),
                        copy_size
                    );

                    remaining -= copy_size;
                    data_offset += copy_size;
                    dest_vaddr += copy_size as u64;
                }
            }
        }
    }

    let stack_pages = 64; 
    let stack_start = USER_STACK_TOP - (stack_pages * 4096);
    for i in 0..stack_pages {
        let curr_vaddr = stack_start + (i * 4096);
        let p2_idx = ((curr_vaddr >> 21) & 0x1FF) as usize;
        let p1_idx = ((curr_vaddr >> 12) & 0x1FF) as usize;

        if !pd.entries[p2_idx].is_present() {
            let pt_phys = frame_alloc.allocate_frame().ok_or("OOM: PT Stack")?;
            let pt = unsafe { &mut *((pt_phys + vmm.hhdm_offset) as *mut PageTable) };
            unsafe { core::ptr::write_bytes(pt as *mut PageTable as *mut u8, 0, 4096); }
            pd.entries[p2_idx].set(pt_phys, user_flags);
        }

        let pt_phys = pd.entries[p2_idx].physical_address();
        let pt = unsafe { &mut *((pt_phys + vmm.hhdm_offset) as *mut PageTable) };

        if !pt.entries[p1_idx].is_present() {
            let frame_phys = frame_alloc.allocate_frame().ok_or("OOM: Stack Frame")?;
            let frame_ptr = (frame_phys + vmm.hhdm_offset) as *mut u8;
            unsafe { core::ptr::write_bytes(frame_ptr, 0, 4096); }
            pt.entries[p1_idx].set(frame_phys, user_flags);
            unsafe { invalidate_tlb(curr_vaddr); }
        }
    }

    {
        let curr_vaddr = USER_IPC_PAGE;
        let p2_idx = ((curr_vaddr >> 21) & 0x1FF) as usize;
        let p1_idx = ((curr_vaddr >> 12) & 0x1FF) as usize;

        if !pd.entries[p2_idx].is_present() {
            let pt_phys = frame_alloc.allocate_frame().ok_or("OOM: PT IPC")?;
            let pt = unsafe { &mut *((pt_phys + vmm.hhdm_offset) as *mut PageTable) };
            unsafe { core::ptr::write_bytes(pt as *mut PageTable as *mut u8, 0, 4096); }
            pd.entries[p2_idx].set(pt_phys, user_flags);
        }

        let pt_phys = pd.entries[p2_idx].physical_address();
        let pt = unsafe { &mut *((pt_phys + vmm.hhdm_offset) as *mut PageTable) };

        let frame_phys = frame_alloc.allocate_frame().ok_or("OOM: IPC Frame")?;
        let ipc_ptr = (frame_phys + vmm.hhdm_offset) as *mut u8;
        unsafe { core::ptr::write_bytes(ipc_ptr, 0, 4096); }
        let bytes = arg.as_bytes();
        let len = core::cmp::min(bytes.len(), 255);
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), ipc_ptr, len);
        }
        pt.entries[p1_idx].set(frame_phys, user_flags);
        unsafe { invalidate_tlb(curr_vaddr); }
    }

    clear_keyboard_buffer();

    let actual_entry = header.entry;
    ELF_EXIT_REQUESTED.store(false, Ordering::SeqCst);
    log_info!("ELF", "Executing Ring 3 Entry: {:#010x}", actual_entry);

    unsafe {
        jump_to_ring3(
            actual_entry,
            USER_STACK_TOP - 0x2000,
            USER_CODE_SELECTOR as u64,
            USER_DATA_SELECTOR as u64,
        );
    }

    log_info!("ELF", "Process completed cleanly, returned to Kernel Shell.");
    Ok(())
}
