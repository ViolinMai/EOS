use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use crate::drivers::pci::{PCI_DEVICES, pci_enable_bus_mastering, pci_read_bar};
use crate::mm::paging::map_mmio_pages;

const ATHEROS_VENDOR: u16 = 0x168C;
const AR9285_DEVICE: u16 = 0x002B;
const AR_SREV: usize = 0x4020;
const AR_STA_ID0: usize = 0x8000;
const AR_STA_ID1: usize = 0x8004;

pub struct Ath9kWifi {
    pub mmio_base: u64,
    pub srev: u32,
    pub mac: [u8; 6],
}

static mut ATH9K_DEV: Option<Ath9kWifi> = None;

pub fn init() {
    let devices = match unsafe { (*addr_of!(PCI_DEVICES)).as_ref() } {
        Some(d) => d,
        None => return,
    };

    for dev in devices {
        if dev.vendor_id != ATHEROS_VENDOR || dev.device_id != AR9285_DEVICE {
            continue;
        }
        unsafe {
            pci_enable_bus_mastering(dev.bus, dev.device, dev.function);
            let bar0 = pci_read_bar(dev.bus, dev.device, dev.function, 0);
            if bar0 == 0 { continue; }
            let mmio_base = match map_mmio_pages(bar0, 16) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Wake up MAC/BB via RTC force wake register (0x7040)
            write_volatile((mmio_base as usize + 0x7040) as *mut u32, 0x1);
            for _ in 0..50_000 { core::hint::spin_loop(); }

            let srev = read_volatile((mmio_base as usize + AR_SREV) as *const u32);
            let id0 = read_volatile((mmio_base as usize + AR_STA_ID0) as *const u32);
            let id1 = read_volatile((mmio_base as usize + AR_STA_ID1) as *const u32);
            let mac = [
                (id0 & 0xFF) as u8,
                ((id0 >> 8) & 0xFF) as u8,
                ((id0 >> 16) & 0xFF) as u8,
                ((id0 >> 24) & 0xFF) as u8,
                (id1 & 0xFF) as u8,
                ((id1 >> 8) & 0xFF) as u8,
            ];

            crate::log_info!(
                "WIFI-ATH9K",
                "Atheros AR9285 Detected (BAR0:{:#x}, SREV:{:#x}, MAC:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x})",
                bar0, srev, mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            );
            *addr_of_mut!(ATH9K_DEV) = Some(Ath9kWifi { mmio_base, srev, mac });
            return;
        }
    }
}

pub fn is_active() -> bool {
    unsafe { (*addr_of!(ATH9K_DEV)).is_some() }
}
