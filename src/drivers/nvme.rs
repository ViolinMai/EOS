use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use crate::drivers::pci::{PCI_DEVICES, pci_enable_bus_mastering, pci_read_bar};
use crate::mm::frame::allocate_frame_safe;
use crate::mm::paging::{VMM, map_mmio_pages};

// NVMe MMIO Register Offsets (BAR0)
const NVME_REG_CAP: usize = 0x00;
const NVME_REG_VS: usize = 0x08;
const NVME_REG_CC: usize = 0x14;
const NVME_REG_CSTS: usize = 0x1C;
const NVME_REG_AQA: usize = 0x24;
const NVME_REG_ASQ: usize = 0x28;
const NVME_REG_ACQ: usize = 0x30;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct NvmeCommand {
    pub cdw0: u32,
    pub nsid: u32,
    pub rsvd: [u32; 2],
    pub mptr: u64,
    pub prp1: u64,
    pub prp2: u64,
    pub cdw10: u32,
    pub cdw11: u32,
    pub cdw12: u32,
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct NvmeCompletion {
    pub dw0: u32,
    pub dw1: u32,
    pub sq_head: u16,
    pub sq_id: u16,
    pub cid: u16,
    pub status: u16,
}

pub struct NvmeController {
    pub mmio_base: u64,
    pub version: u32,
    pub doorbell_stride: usize,
    pub asq_phys: u64,
    pub acq_phys: u64,
    pub asq_virt: *mut NvmeCommand,
    pub acq_virt: *mut NvmeCompletion,
    pub asq_tail: u16,
    pub acq_head: u16,
    pub phase: u16,
    pub ready: bool,
}

static mut NVME_CTRL: Option<NvmeController> = None;

impl NvmeController {
    #[inline]
    unsafe fn read32(&self, off: usize) -> u32 {
        unsafe { read_volatile((self.mmio_base as usize + off) as *const u32) }
    }

    #[inline]
    unsafe fn write32(&self, off: usize, val: u32) {
        unsafe { write_volatile((self.mmio_base as usize + off) as *mut u32, val); }
    }

    #[inline]
    unsafe fn read64(&self, off: usize) -> u64 {
        unsafe { read_volatile((self.mmio_base as usize + off) as *const u64) }
    }

    #[inline]
    unsafe fn write64(&self, off: usize, val: u64) {
        unsafe { write_volatile((self.mmio_base as usize + off) as *mut u64, val); }
    }

    pub fn build_rw_cmd(&self, is_write: bool, nsid: u32, lba: u64, sectors: u16, prp1: u64, cid: u16) -> NvmeCommand {
        let opcode: u32 = if is_write { 0x01 } else { 0x02 };
        NvmeCommand {
            cdw0: opcode | ((cid as u32) << 16),
            nsid,
            prp1,
            cdw10: (lba & 0xFFFF_FFFF) as u32,
            cdw11: (lba >> 32) as u32,
            cdw12: sectors.saturating_sub(1) as u32,
            ..Default::default()
        }
    }
}

pub fn init() {
    let devices = match unsafe { (*addr_of!(PCI_DEVICES)).as_ref() } {
        Some(d) => d,
        None => return,
    };
    let hhdm = match unsafe { (*addr_of!(VMM)).as_ref() } {
        Some(v) => v.hhdm_offset,
        None => return,
    };

    for dev in devices {
        // PCI Class 0x01 (Mass Storage), Subclass 0x08 (NVMe)
        if dev.class_code != 0x01 || dev.subclass != 0x08 {
            continue;
        }
        unsafe {
            pci_enable_bus_mastering(dev.bus, dev.device, dev.function);
            let bar0 = pci_read_bar(dev.bus, dev.device, dev.function, 0);
            if bar0 == 0 { continue; }
            let mmio_base = match map_mmio_pages(bar0, 4) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let asq_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let acq_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            core::ptr::write_bytes((asq_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((acq_phys + hhdm) as *mut u8, 0, 4096);

            let mut ctrl = NvmeController {
                mmio_base,
                version: 0,
                doorbell_stride: 4,
                asq_phys,
                acq_phys,
                asq_virt: (asq_phys + hhdm) as *mut NvmeCommand,
                acq_virt: (acq_phys + hhdm) as *mut NvmeCompletion,
                asq_tail: 0,
                acq_head: 0,
                phase: 1,
                ready: false,
            };

            let cap = ctrl.read64(NVME_REG_CAP);
            ctrl.version = ctrl.read32(NVME_REG_VS);
            ctrl.doorbell_stride = 4usize << (((cap >> 32) & 0xF) as usize);

            // 1. Disable controller before configuring Admin Queues
            let cc = ctrl.read32(NVME_REG_CC);
            if (cc & 1) != 0 {
                ctrl.write32(NVME_REG_CC, cc & !1);
                for _ in 0..500_000 {
                    if (ctrl.read32(NVME_REG_CSTS) & 1) == 0 { break; }
                    core::hint::spin_loop();
                }
            }

            // 2. Configure 64-entry Admin Submission & Completion Queues
            ctrl.write32(NVME_REG_AQA, (63 << 16) | 63);
            ctrl.write64(NVME_REG_ASQ, asq_phys);
            ctrl.write64(NVME_REG_ACQ, acq_phys);

            // 3. Enable controller (IOSQES=6 (64B), IOCQES=4 (16B), EN=1)
            ctrl.write32(NVME_REG_CC, (4 << 20) | (6 << 16) | 1);
            for _ in 0..500_000 {
                if (ctrl.read32(NVME_REG_CSTS) & 1) != 0 {
                    ctrl.ready = true;
                    break;
                }
                core::hint::spin_loop();
            }

            crate::log_info!(
                "NVME",
                "PCIe NVMe SSD Controller Initialized (v{}.{}, BAR0:{:#x}, Ready:{})",
                ctrl.version >> 16,
                (ctrl.version >> 8) & 0xFF,
                bar0,
                ctrl.ready
            );
            *addr_of_mut!(NVME_CTRL) = Some(ctrl);
            return;
        }
    }
    crate::log_info!("NVME", "No PCIe NVMe SSD present on bus (driver armed for bare-metal).");
}

pub fn is_active() -> bool {
    unsafe { (*addr_of!(NVME_CTRL)).as_ref().map(|c| c.ready).unwrap_or(false) }
}

pub fn self_test() -> bool {
    core::mem::size_of::<NvmeCommand>() == 64 && core::mem::size_of::<NvmeCompletion>() == 16
}
