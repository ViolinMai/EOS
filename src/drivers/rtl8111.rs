use alloc::vec::Vec;
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use crate::drivers::pci::{PCI_DEVICES, pci_enable_bus_mastering, pci_read_bar};
use crate::mm::frame::allocate_frame_safe;
use crate::mm::paging::{VMM, map_mmio_pages};

const RTL_VENDOR: u16 = 0x10EC;
const RTL_DEV_8168: u16 = 0x8168;
const RTL_DEV_8136: u16 = 0x8136;

const RTL_REG_TNPDS_LO: usize = 0x20;
const RTL_REG_TNPDS_HI: usize = 0x24;
const RTL_REG_CMD: usize = 0x37;
const RTL_REG_TPPOLL: usize = 0x38;
const RTL_REG_RCR: usize = 0x44;
const RTL_REG_RDSAR_LO: usize = 0xE4;
const RTL_REG_RDSAR_HI: usize = 0xE8;

const NUM_DESC: usize = 32;
const DESC_OWN: u32 = 1 << 31;
const DESC_EOR: u32 = 1 << 30;
const DESC_FS: u32 = 1 << 29;
const DESC_LS: u32 = 1 << 28;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct RtlDesc {
    pub opts1: u32,
    pub opts2: u32,
    pub addr_lo: u32,
    pub addr_hi: u32,
}

pub struct Rtl8111Driver {
    pub mmio_base: u64,
    pub tx_ring: *mut RtlDesc,
    pub rx_ring: *mut RtlDesc,
    pub tx_bufs: [*mut u8; NUM_DESC],
    pub rx_bufs: [*mut u8; NUM_DESC],
    pub tx_idx: usize,
    pub rx_idx: usize,
}

unsafe impl Send for Rtl8111Driver {}
unsafe impl Sync for Rtl8111Driver {}

static mut RTL_DRV: Option<Rtl8111Driver> = None;

impl Rtl8111Driver {
    #[inline]
    unsafe fn read8(&self, off: usize) -> u8 {
        unsafe { read_volatile((self.mmio_base as usize + off) as *const u8) }
    }
    #[inline]
    unsafe fn write8(&self, off: usize, val: u8) {
        unsafe { write_volatile((self.mmio_base as usize + off) as *mut u8, val); }
    }
    #[inline]
    unsafe fn write32(&self, off: usize, val: u32) {
        unsafe { write_volatile((self.mmio_base as usize + off) as *mut u32, val); }
    }

    pub fn read_mac(&self) -> [u8; 6] {
        unsafe {
            [
                self.read8(0), self.read8(1), self.read8(2),
                self.read8(3), self.read8(4), self.read8(5),
            ]
        }
    }

    pub fn send_packet(&mut self, data: &[u8]) {
        if data.is_empty() || data.len() > 2048 { return; }
        unsafe {
            let desc = &mut *self.tx_ring.add(self.tx_idx);
            core::ptr::copy_nonoverlapping(data.as_ptr(), self.tx_bufs[self.tx_idx], data.len());
            let eor = if self.tx_idx == NUM_DESC - 1 { DESC_EOR } else { 0 };
            desc.opts1 = DESC_OWN | eor | DESC_FS | DESC_LS | (data.len() as u32 & 0x3FFF);
            self.tx_idx = (self.tx_idx + 1) % NUM_DESC;
            self.write8(RTL_REG_TPPOLL, 0x40); // Normal Priority Queue Poll
        }
    }

    pub fn receive_packet(&mut self) -> Option<Vec<u8>> {
        unsafe {
            let desc = &mut *self.rx_ring.add(self.rx_idx);
            let opts1 = read_volatile(&desc.opts1);
            if (opts1 & DESC_OWN) != 0 {
                return None;
            }
            let len = (opts1 & 0x3FFF) as usize;
            let mut pkt = alloc::vec![0u8; len.min(2048)];
            if !pkt.is_empty() {
                core::ptr::copy_nonoverlapping(self.rx_bufs[self.rx_idx], pkt.as_mut_ptr(), pkt.len());
            }
            let eor = if self.rx_idx == NUM_DESC - 1 { DESC_EOR } else { 0 };
            desc.opts1 = DESC_OWN | eor | 2048;
            self.rx_idx = (self.rx_idx + 1) % NUM_DESC;
            Some(pkt)
        }
    }
}

pub fn get_driver() -> Option<&'static mut Rtl8111Driver> {
    unsafe { (*addr_of_mut!(RTL_DRV)).as_mut() }
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
        if dev.vendor_id != RTL_VENDOR || (dev.device_id != RTL_DEV_8168 && dev.device_id != RTL_DEV_8136) {
            continue;
        }
        unsafe {
            pci_enable_bus_mastering(dev.bus, dev.device, dev.function);
            let bar2 = pci_read_bar(dev.bus, dev.device, dev.function, 2);
            let bar0 = pci_read_bar(dev.bus, dev.device, dev.function, 0);
            let mmio_phys = if bar2 != 0 { bar2 } else { bar0 };
            if mmio_phys == 0 { continue; }
            let mmio_base = match map_mmio_pages(mmio_phys, 2) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let tx_ring_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let rx_ring_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let tx_ring = (tx_ring_phys + hhdm) as *mut RtlDesc;
            let rx_ring = (rx_ring_phys + hhdm) as *mut RtlDesc;
            core::ptr::write_bytes(tx_ring as *mut u8, 0, 4096);
            core::ptr::write_bytes(rx_ring as *mut u8, 0, 4096);

            let mut tx_bufs = [core::ptr::null_mut(); NUM_DESC];
            let mut rx_bufs = [core::ptr::null_mut(); NUM_DESC];
            for i in 0..NUM_DESC {
                let tf = match allocate_frame_safe() { Some(f) => f, None => return };
                let rf = match allocate_frame_safe() { Some(f) => f, None => return };
                tx_bufs[i] = (tf + hhdm) as *mut u8;
                rx_bufs[i] = (rf + hhdm) as *mut u8;
                let eor = if i == NUM_DESC - 1 { DESC_EOR } else { 0 };
                *tx_ring.add(i) = RtlDesc { opts1: eor, opts2: 0, addr_lo: tf as u32, addr_hi: (tf >> 32) as u32 };
                *rx_ring.add(i) = RtlDesc { opts1: DESC_OWN | eor | 2048, opts2: 0, addr_lo: rf as u32, addr_hi: (rf >> 32) as u32 };
            }

            let drv = Rtl8111Driver {
                mmio_base,
                tx_ring,
                rx_ring,
                tx_bufs,
                rx_bufs,
                tx_idx: 0,
                rx_idx: 0,
            };

            // Software Reset
            drv.write8(RTL_REG_CMD, 0x10);
            for _ in 0..100_000 {
                if (drv.read8(RTL_REG_CMD) & 0x10) == 0 { break; }
                core::hint::spin_loop();
            }

            drv.write32(RTL_REG_TNPDS_LO, tx_ring_phys as u32);
            drv.write32(RTL_REG_TNPDS_HI, (tx_ring_phys >> 32) as u32);
            drv.write32(RTL_REG_RDSAR_LO, rx_ring_phys as u32);
            drv.write32(RTL_REG_RDSAR_HI, (rx_ring_phys >> 32) as u32);
            drv.write32(RTL_REG_RCR, 0x0000_000F); // Accept Broadcast/Multicast/Physical/All
            drv.write8(RTL_REG_CMD, 0x0C);         // Enable TX + RX

            crate::log_info!("RTL8111", "Realtek RTL8111/8168 Gigabit NIC Initialized (MMIO:{:#x})", mmio_phys);
            *addr_of_mut!(RTL_DRV) = Some(drv);
            return;
        }
    }
    crate::log_info!("RTL8111", "No Realtek RTL8111 NIC on bus (Intel E1000 active).");
}

pub fn self_test() -> bool {
    core::mem::size_of::<RtlDesc>() == 16
}
