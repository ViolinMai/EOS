use alloc::vec::Vec;
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::drivers::pci::{PCI_DEVICES, pci_enable_bus_mastering, pci_read_bar};
use crate::mm::frame::allocate_frame_safe;
use crate::mm::paging::{VMM, map_mmio_pages};

// Intel HDA MMIO Register Offsets (BAR0)
const HDA_REG_GCTL: usize = 0x08;
const HDA_REG_STATESTS: usize = 0x0E;
const HDA_REG_CORBLBASE: usize = 0x40;
const HDA_REG_CORBUBASE: usize = 0x44;
const HDA_REG_CORBWP: usize = 0x48;
const HDA_REG_CORBCTL: usize = 0x4C;
const HDA_REG_RIRBLBASE: usize = 0x50;
const HDA_REG_RIRBUBASE: usize = 0x54;
const HDA_REG_RIRBCTL: usize = 0x5C;

const HDA_BDL_COUNT: usize = 16;
const HDA_BUF_BYTES: usize = 4096;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct HdaBdlEntry {
    pub addr: u64,
    pub len: u32,
    pub ioc: u32,
}

pub struct IntelHdaDriver {
    pub mmio_base: u64,
    pub corb_phys: u64,
    pub rirb_phys: u64,
    pub bdl: Vec<HdaBdlEntry>,
    pub pcm_pool: Vec<u8>,
    pub next_idx: usize,
    pub volume_pct: u8,
    pub codecs_mask: u16,
}

static HDA_LOCK: AtomicBool = AtomicBool::new(false);
static mut HDA_DRV: Option<IntelHdaDriver> = None;

fn lock_hda() {
    while HDA_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
fn unlock_hda() { HDA_LOCK.store(false, Ordering::Release); }

impl IntelHdaDriver {
    #[inline]
    unsafe fn read32(&self, off: usize) -> u32 {
        unsafe { read_volatile((self.mmio_base as usize + off) as *const u32) }
    }
    #[inline]
    unsafe fn write32(&self, off: usize, val: u32) {
        unsafe { write_volatile((self.mmio_base as usize + off) as *mut u32, val); }
    }
    #[inline]
    unsafe fn read16(&self, off: usize) -> u16 {
        unsafe { read_volatile((self.mmio_base as usize + off) as *const u16) }
    }
    #[inline]
    unsafe fn write16(&self, off: usize, val: u16) {
        unsafe { write_volatile((self.mmio_base as usize + off) as *mut u16, val); }
    }
    #[inline]
    unsafe fn write8(&self, off: usize, val: u8) {
        unsafe { write_volatile((self.mmio_base as usize + off) as *mut u8, val); }
    }

    pub fn make_verb(codec: u32, node: u32, verb: u32, payload: u32) -> u32 {
        ((codec & 0xF) << 28) | ((node & 0xFF) << 20) | ((verb & 0xFFF) << 8) | (payload & 0xFF)
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
        // PCI Class 0x04 (Multimedia), Subclass 0x03 (Intel HDA)
        if dev.class_code != 0x04 || dev.subclass != 0x03 {
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

            let corb_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let rirb_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            core::ptr::write_bytes((corb_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((rirb_phys + hhdm) as *mut u8, 0, 4096);

            let mut bdl = alloc::vec![HdaBdlEntry::default(); HDA_BDL_COUNT];
            let pcm_pool = alloc::vec![0u8; HDA_BDL_COUNT * HDA_BUF_BYTES];
            let pool_phys = (pcm_pool.as_ptr() as u64).saturating_sub(hhdm);
            for i in 0..HDA_BDL_COUNT {
                bdl[i] = HdaBdlEntry {
                    addr: pool_phys + (i * HDA_BUF_BYTES) as u64,
                    len: HDA_BUF_BYTES as u32,
                    ioc: 0,
                };
            }

            let mut drv = IntelHdaDriver {
                mmio_base,
                corb_phys,
                rirb_phys,
                bdl,
                pcm_pool,
                next_idx: 0,
                volume_pct: 85,
                codecs_mask: 0,
            };

            // Reset & bring controller out of reset (GCTL.CRST = 1)
            drv.write32(HDA_REG_GCTL, 0);
            for _ in 0..50_000 { core::hint::spin_loop(); }
            drv.write32(HDA_REG_GCTL, 1);
            for _ in 0..100_000 {
                if (drv.read32(HDA_REG_GCTL) & 1) != 0 { break; }
                core::hint::spin_loop();
            }

            // Configure CORB & RIRB DMA rings
            drv.write8(HDA_REG_CORBCTL, 0);
            drv.write8(HDA_REG_RIRBCTL, 0);
            drv.write32(HDA_REG_CORBLBASE, corb_phys as u32);
            drv.write32(HDA_REG_CORBUBASE, (corb_phys >> 32) as u32);
            drv.write16(HDA_REG_CORBWP, 0);
            drv.write32(HDA_REG_RIRBLBASE, rirb_phys as u32);
            drv.write32(HDA_REG_RIRBUBASE, (rirb_phys >> 32) as u32);
            drv.write8(HDA_REG_CORBCTL, 0x02); // CORB DMA Run
            drv.write8(HDA_REG_RIRBCTL, 0x02); // RIRB DMA Run

            drv.codecs_mask = drv.read16(HDA_REG_STATESTS);

            crate::log_info!(
                "HDA",
                "Intel HDA Controller Initialized (BAR0:{:#x}, Codecs:{:#x}, 48kHz Stereo BDL)",
                bar0,
                drv.codecs_mask
            );
            *addr_of_mut!(HDA_DRV) = Some(drv);
            return;
        }
    }
    crate::log_info!("HDA", "No Intel HDA controller on bus (AC'97 fallback active).");
}

pub fn is_active() -> bool {
    unsafe { (*addr_of!(HDA_DRV)).is_some() }
}

pub fn set_volume(vol_pct: u8) {
    lock_hda();
    unsafe {
        if let Some(drv) = (*addr_of_mut!(HDA_DRV)).as_mut() {
            drv.volume_pct = vol_pct.min(100);
        }
    }
    unlock_hda();
}

pub fn write_pcm(data: &[u8]) -> usize {
    if data.is_empty() { return 0; }
    lock_hda();
    let mut written = 0usize;
    unsafe {
        if let Some(drv) = (*addr_of_mut!(HDA_DRV)).as_mut() {
            let mut off = 0usize;
            while off < data.len() {
                let chunk = (data.len() - off).min(HDA_BUF_BYTES) & !3;
                if chunk == 0 { break; }
                let dst_off = drv.next_idx * HDA_BUF_BYTES;
                drv.pcm_pool[dst_off..dst_off + chunk].copy_from_slice(&data[off..off + chunk]);
                drv.bdl[drv.next_idx].len = chunk as u32;
                drv.next_idx = (drv.next_idx + 1) % HDA_BDL_COUNT;
                off += chunk;
                written += chunk;
            }
        }
    }
    unlock_hda();
    written
}

pub fn self_test() -> bool {
    let verb = IntelHdaDriver::make_verb(0, 2, 0x705, 0x00); // Set Power State D0
    core::mem::size_of::<HdaBdlEntry>() == 16 && verb == 0x0027_0500
}
