use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use crate::drivers::pci::{PCI_DEVICES, pci_enable_bus_mastering, pci_read_bar};
use crate::mm::frame::allocate_frame_safe;
use crate::mm::paging::{VMM, map_mmio_pages};

const SATA_SIG_ATA: u32 = 0x0000_0101;
const HBA_PORT_IPM_ACTIVE: u32 = 1;
const HBA_PORT_DET_PRESENT: u32 = 3;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct HbaCmdHeader {
    pub flags: u16,
    pub prdtl: u16,
    pub prdbc: u32,
    pub ctba: u32,
    pub ctbau: u32,
    pub rsv1: [u32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct HbaPrdtEntry {
    pub dba: u32,
    pub dbau: u32,
    pub rsv0: u32,
    pub dbc: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct FisRegH2D {
    pub fis_type: u8,
    pub pmport_c: u8,
    pub command: u8,
    pub featurel: u8,
    pub lba0: u8,
    pub lba1: u8,
    pub lba2: u8,
    pub device: u8,
    pub lba3: u8,
    pub lba4: u8,
    pub lba5: u8,
    pub featureh: u8,
    pub countl: u8,
    pub counth: u8,
    pub icc: u8,
    pub control: u8,
    pub rsv1: [u8; 4],
}

pub struct AhciDriver {
    pub abar_virt: u64,
    pub active_ports: u32,
    pub sata_port: Option<usize>,
    pub clb_phys: u64,
    pub fb_phys: u64,
    pub ctba_phys: u64,
}

static mut AHCI_DRV: Option<AhciDriver> = None;

impl AhciDriver {
    #[inline]
    unsafe fn read32(&self, off: usize) -> u32 {
        unsafe { read_volatile((self.abar_virt as usize + off) as *const u32) }
    }

    #[inline]
    unsafe fn write32(&self, off: usize, val: u32) {
        unsafe { write_volatile((self.abar_virt as usize + off) as *mut u32, val); }
    }

    pub fn build_dma_fis(is_write: bool, lba: u64, count: u16) -> FisRegH2D {
        FisRegH2D {
            fis_type: 0x27, // FIS_TYPE_REG_H2D
            pmport_c: 0x80, // Command bit set
            command: if is_write { 0x35 } else { 0x25 }, // WRITE/READ DMA EXT
            device: 1 << 6, // LBA mode
            lba0: (lba & 0xFF) as u8,
            lba1: ((lba >> 8) & 0xFF) as u8,
            lba2: ((lba >> 16) & 0xFF) as u8,
            lba3: ((lba >> 24) & 0xFF) as u8,
            lba4: ((lba >> 32) & 0xFF) as u8,
            lba5: ((lba >> 40) & 0xFF) as u8,
            countl: (count & 0xFF) as u8,
            counth: (count >> 8) as u8,
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
        // PCI Class 0x01 (Mass Storage), Subclass 0x06 (SATA AHCI)
        if dev.class_code != 0x01 || dev.subclass != 0x06 {
            continue;
        }
        unsafe {
            pci_enable_bus_mastering(dev.bus, dev.device, dev.function);
            let abar_phys = pci_read_bar(dev.bus, dev.device, dev.function, 5);
            if abar_phys == 0 { continue; }
            let abar_virt = match map_mmio_pages(abar_phys, 4) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let clb_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let fb_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let ctba_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            core::ptr::write_bytes((clb_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((fb_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((ctba_phys + hhdm) as *mut u8, 0, 4096);

            let mut drv = AhciDriver {
                abar_virt,
                active_ports: 0,
                sata_port: None,
                clb_phys,
                fb_phys,
                ctba_phys,
            };

            // Enable AHCI mode (GHC.AE = bit 31)
            let ghc = drv.read32(0x04);
            drv.write32(0x04, ghc | (1 << 31));

            let pi = drv.read32(0x0C);
            drv.active_ports = pi;
            for port in 0..32usize {
                if (pi & (1 << port)) == 0 { continue; }
                let pbase = 0x100 + port * 0x80;
                let ssts = drv.read32(pbase + 0x28);
                let det = ssts & 0x0F;
                let ipm = (ssts >> 8) & 0x0F;
                let sig = drv.read32(pbase + 0x24);
                if det == HBA_PORT_DET_PRESENT && ipm == HBA_PORT_IPM_ACTIVE && sig == SATA_SIG_ATA {
                    // Program CLB / FB for DMA
                    drv.write32(pbase + 0x00, clb_phys as u32);
                    drv.write32(pbase + 0x04, (clb_phys >> 32) as u32);
                    drv.write32(pbase + 0x08, fb_phys as u32);
                    drv.write32(pbase + 0x0C, (fb_phys >> 32) as u32);
                    drv.sata_port = Some(port);
                    break;
                }
            }

            crate::log_info!(
                "AHCI",
                "SATA AHCI HBA Initialized (ABAR:{:#x}, PI:{:#x}, ActiveSATA:{:?})",
                abar_phys,
                pi,
                drv.sata_port
            );
            *addr_of_mut!(AHCI_DRV) = Some(drv);
            return;
        }
    }
    crate::log_info!("AHCI", "No SATA AHCI HBA found on bus (fallback to legacy ATA active).");
}

pub fn is_active() -> bool {
    unsafe { (*addr_of!(AHCI_DRV)).is_some() }
}

pub fn self_test() -> bool {
    let fis = AhciDriver::build_dma_fis(false, 0x1234, 8);
    core::mem::size_of::<HbaCmdHeader>() == 32
        && core::mem::size_of::<HbaPrdtEntry>() == 16
        && fis.fis_type == 0x27
        && fis.command == 0x25
}
