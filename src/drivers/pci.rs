use core::arch::asm;
use alloc::vec::Vec;
use crate::log_info;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

#[derive(Debug, Clone)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
}

pub static mut PCI_DEVICES: Option<Vec<PciDevice>> = None;

#[inline]
unsafe fn outl(port: u16, val: u32) {
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    unsafe {
        asm!("in eax, dx", in("dx") port, out("eax") val, options(nomem, nostack, preserves_flags));
    }
    val
}

pub unsafe fn pci_read_u32(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let address = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        inl(PCI_CONFIG_DATA)
    }
}

pub unsafe fn pci_read_u16(bus: u8, device: u8, func: u8, offset: u8) -> u16 {
    let val = unsafe { pci_read_u32(bus, device, func, offset) };
    if (offset & 2) != 0 {
        (val >> 16) as u16
    } else {
        (val & 0xFFFF) as u16
    }
}

pub fn class_name(class: u8, subclass: u8) -> &'static str {
    match (class, subclass) {
        (0x01, 0x01) => "IDE Controller",
        (0x01, 0x06) => "SATA Controller (AHCI)",
        (0x01, 0x08) => "NVMe Controller",
        (0x01, _)    => "Mass Storage Device",
        (0x02, 0x00) => "Ethernet Adapter",
        (0x02, _)    => "Network Controller",
        (0x03, 0x00) => "VGA Compatible Controller",
        (0x03, _)    => "Display Controller",
        (0x04, _)    => "Multimedia Device",
        (0x06, 0x00) => "Host Bridge",
        (0x06, 0x01) => "ISA Bridge",
        (0x06, 0x04) => "PCI-to-PCI Bridge",
        (0x06, _)    => "Bridge Device",
        (0x0C, 0x03) => "USB Controller",
        _            => "Generic Device",
    }
}

pub fn init() {
    let mut list = Vec::new();

    unsafe {
        for bus in 0..=8 {
            for device in 0..32 {
                let vendor = pci_read_u16(bus, device, 0, 0x00);
                if vendor == 0xFFFF {
                    continue;
                }

                let dev_id = pci_read_u16(bus, device, 0, 0x02);
                let class_sub = pci_read_u32(bus, device, 0, 0x08);
                let class_code = (class_sub >> 24) as u8;
                let subclass = (class_sub >> 16) as u8;
                let prog_if = (class_sub >> 8) as u8;

                list.push(PciDevice {
                    bus,
                    device,
                    function: 0,
                    vendor_id: vendor,
                    device_id: dev_id,
                    class_code,
                    subclass,
                    prog_if,
                });
            }
        }

        log_info!("PCI: Enumeration complete. Found {} devices.", list.len());
        *core::ptr::addr_of_mut!(PCI_DEVICES) = Some(list);
    }
}
