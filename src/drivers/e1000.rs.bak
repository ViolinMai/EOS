use alloc::vec::Vec;
use crate::drivers::pci::pci_read_u32;

pub struct E1000Driver {
    pub bar0_mem: u64,
}

static mut E1000_STORAGE: Option<E1000Driver> = None;

impl E1000Driver {
    fn write_reg(&self, offset: u16, val: u32) {
        unsafe {
            let ptr = (self.bar0_mem + offset as u64) as *mut u32;
            core::ptr::write_volatile(ptr, val);
        }
    }

    fn read_reg(&self, offset: u16) -> u32 {
        unsafe {
            let ptr = (self.bar0_mem + offset as u64) as *const u32;
            core::ptr::read_volatile(ptr)
        }
    }

    pub fn read_mac(&self) -> [u8; 6] {
        let low = self.read_reg(0x5400);
        let high = self.read_reg(0x5404);
        [
            (low & 0xFF) as u8,
            ((low >> 8) & 0xFF) as u8,
            ((low >> 16) & 0xFF) as u8,
            ((low >> 24) & 0xFF) as u8,
            (high & 0xFF) as u8,
            ((high >> 8) & 0xFF) as u8,
        ]
    }

    pub fn send_packet(&self, _data: &[u8]) { }

    pub fn receive_packet(&self) -> Option<Vec<u8>> {
        None
    }
}

pub fn get_driver() -> Option<&'static mut E1000Driver> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(E1000_STORAGE);
        (*ptr).as_mut()
    }
}

pub fn init() {
    unsafe {
        let pci_ptr = core::ptr::addr_of!(crate::drivers::pci::PCI_DEVICES);
        if let Some(devices) = (*pci_ptr).as_ref() {
            for dev in devices {
                if dev.vendor_id == 0x8086 && dev.device_id == 0x100E {
                    let bar0 = pci_read_u32(dev.bus, dev.device, dev.function, 0x10);
                    let mem_base = (bar0 & !0xF) as u64;
                    let storage_ptr = core::ptr::addr_of_mut!(E1000_STORAGE);
                    *storage_ptr = Some(E1000Driver { bar0_mem: mem_base });
                    crate::log_info!("E1000", "Intel E1000 NIC Driver bound at MMIO {:#010x}", mem_base);
                    return;
                }
            }
        }
    }
}
