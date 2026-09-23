use crate::arch::x86_64::gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};
use crate::arch::x86_64::keyboard::clear_keyboard_buffer;
use crate::arch::x86_64::syscall::{ELF_EXIT_REQUESTED, KERNEL_SAVED_RSP};
use crate::log_info;
use crate::mm::paging::{invalidate_tlb, read_cr3, PageTable, PAGE_PRESENT, PAGE_USER, PAGE_WRITABLE};
use crate::mm::paging::VMM;
use core::arch::naked_asm;
use core::ptr::addr_of_mut;
use core::sync::atomic::Ordering;
use core::mem::MaybeUninit;

pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const PT_LOAD: u32 = 1;

pub const USER_STACK_TOP: u64 = 0x0000_0000_2040_0000;

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

#[repr(align(4096))]
pub struct PageAlignedTable(pub [u64; 512]);

static mut USER_PDPT: PageAlignedTable = PageAlignedTable([0; 512]);
static mut USER_PD: PageAlignedTable = PageAlignedTable([0; 512]);

pub const PT_COUNT: usize = 256;
static mut USER_PTS: MaybeUninit<[PageAlignedTable; PT_COUNT]> = MaybeUninit::uninit();

pub const USER_MEM_SIZE: usize = 512 * 1024 * 1024;
#[repr(align(4096))]
pub struct UserPhysicalStorage(pub [u8; USER_MEM_SIZE]);

static mut USER_MEMORY_BLOCK: MaybeUninit<UserPhysicalStorage> = MaybeUninit::uninit();

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

        "push rcx",         // SS
        "push rsi",         // RSP
        "push 0x202",       // RFLAGS (IF=1)
        "push rdx",         // CS
        "push rdi",         // RIP
        "iretq",

        kernel_sp = sym KERNEL_SAVED_RSP,
    );
}

pub fn load_and_run_elf(data: &[u8]) -> Result<(), &'static str> {
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

    let storage_ptr = addr_of_mut!(USER_MEMORY_BLOCK) as *mut u8;
    unsafe {
        core::ptr::write_bytes(storage_ptr, 0, USER_MEM_SIZE);
    }

    for i in 0..phnum {
        let ph_offset = phoff + (i * phentsize);
        if ph_offset + core::mem::size_of::<Elf64ProgramHeader>() > data.len() {
            return Err("Program header out of bounds");
        }

        let ph = unsafe { *(data.as_ptr().add(ph_offset) as *const Elf64ProgramHeader) };

        if ph.p_type == PT_LOAD {
            let file_off = ph.p_offset as usize;
            let file_sz = ph.p_filesz as usize;
            let mem_sz = ph.p_memsz as usize;
            let vaddr_offset = (ph.p_vaddr.saturating_sub(0x00400000)) as usize;

            if file_off + file_sz > data.len() {
                return Err("Segment exceeds file boundary");
            }
            if vaddr_offset + mem_sz > USER_MEM_SIZE {
                return Err("ELF memory footprint exceeds allocated buffer (512MB)");
            }

            unsafe {
                let dest = storage_ptr.add(vaddr_offset);
                core::ptr::copy_nonoverlapping(data.as_ptr().add(file_off), dest, file_sz);
                if mem_sz > file_sz {
                    core::ptr::write_bytes(dest.add(file_sz), 0, mem_sz - file_sz);
                }
            }
        }
    }

    unsafe {
        let vmm = (&*addr_of_mut!(VMM)).as_ref().ok_or("VMM not initialized")?;

        let pdpt_virt = addr_of_mut!(USER_PDPT) as u64;
        let pd_virt   = addr_of_mut!(USER_PD) as u64;
        let code_data_virt = storage_ptr as u64;

        let (pdpt_phys, _) = vmm.translate(pdpt_virt).ok_or("Failed translate PDPT")?;
        let (pd_phys, _)   = vmm.translate(pd_virt).ok_or("Failed translate PD")?;
        let (code_phys, _) = vmm.translate(code_data_virt).ok_or("Failed translate CODE DATA")?;

        core::ptr::write_bytes(addr_of_mut!(USER_PDPT) as *mut u8, 0, 4096);
        core::ptr::write_bytes(addr_of_mut!(USER_PD) as *mut u8, 0, 4096);

        let user_flags = PAGE_PRESENT | PAGE_WRITABLE | PAGE_USER;

        let pml4_phys = read_cr3() & 0x000F_FFFF_FFFF_F000;
        let pml4 = &mut *((pml4_phys + vmm.hhdm_offset) as *mut PageTable);
        pml4.entries[0].set(pdpt_phys, user_flags);

        USER_PDPT.0[0] = pd_phys | user_flags;

        let pts_ptr = addr_of_mut!(USER_PTS) as *mut PageAlignedTable;

        for pt_idx in 0..PT_COUNT {
            let pt_ptr = pts_ptr.add(pt_idx);
            core::ptr::write_bytes(pt_ptr as *mut u8, 0, 4096);
            let (pt_phys, _) = vmm.translate(pt_ptr as u64).ok_or("Failed translate PT")?;
            USER_PD.0[2 + pt_idx] = pt_phys | user_flags;

            for page in 0..512 {
                let frame_offset = ((pt_idx * 512) + page) as u64 * 4096;
                let frame = code_phys + frame_offset;
                (*pt_ptr).0[page] = frame | user_flags;
                invalidate_tlb(0x00400000 + frame_offset);
            }
        }

        clear_keyboard_buffer();
    }

    let actual_entry = header.entry;
    ELF_EXIT_REQUESTED.store(false, Ordering::SeqCst);
    log_info!("ELF: Launching entry point at {:#010x} (RAM: 512MB, Stack: {:#010x})...", actual_entry, USER_STACK_TOP);

    jump_to_ring3(
        actual_entry,
        USER_STACK_TOP - 0x1000,
        USER_CODE_SELECTOR as u64,
        USER_DATA_SELECTOR as u64,
    );

    Ok(())
}
