use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use crate::drivers::pci::{PCI_DEVICES, pci_enable_bus_mastering, pci_read_bar};
use crate::mm::paging::map_mmio_pages;

pub struct EhciController {
    pub cap_base: u64,
    pub op_base: u64,
    pub num_ports: usize,
    pub connected_ports: u32,
}

static mut EHCI_CTRLS: [Option<EhciController>; 2] = [None, None];
static mut EHCI_COUNT: usize = 0;

pub fn init() {
    let devices = match unsafe { (*addr_of!(PCI_DEVICES)).as_ref() } {
        Some(d) => d,
        None => return,
    };

    for dev in devices {
        // PCI Class 0x0C (Serial Bus), Subclass 0x03 (USB), ProgIF 0x20 (EHCI USB 2.0)
        if dev.class_code != 0x0C || dev.subclass != 0x03 || dev.prog_if != 0x20 {
            continue;
        }
        unsafe {
            let idx = *addr_of!(EHCI_COUNT);
            if idx >= 2 { break; }

            pci_enable_bus_mastering(dev.bus, dev.device, dev.function);
            let bar0 = pci_read_bar(dev.bus, dev.device, dev.function, 0);
            if bar0 == 0 { continue; }
            let cap_base = match map_mmio_pages(bar0, 2) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let caplength = read_volatile(cap_base as *const u8) as u64;
            let hcsparams = read_volatile((cap_base + 0x04) as *const u32);
            let num_ports = (hcsparams & 0x0F) as usize;
            let op_base = cap_base + caplength;

            // Halt controller safely before reset
            let usbcmd_ptr = op_base as *mut u32;
            let mut cmd = read_volatile(usbcmd_ptr);
            cmd &= !1;
            write_volatile(usbcmd_ptr, cmd);
            for _ in 0..50_000 { core::hint::spin_loop(); }

            // Host Controller Reset (HCRESET)
            write_volatile(usbcmd_ptr, 1 << 1);
            for _ in 0..100_000 {
                if (read_volatile(usbcmd_ptr) & (1 << 1)) == 0 { break; }
                core::hint::spin_loop();
            }

            // Route all ports to EHCI (CONFIGFLAG = 1)
            write_volatile((op_base + 0x40) as *mut u32, 1);
            for _ in 0..50_000 { core::hint::spin_loop(); }

            let mut connected = 0u32;
            for p in 0..num_ports.min(15) {
                let portsc = read_volatile((op_base + 0x44 + (p as u64) * 4) as *const u32);
                if (portsc & 1) != 0 {
                    connected |= 1 << p;
                }
            }

            crate::log_info!(
                "USB-EHCI",
                "Controller #{} Ready (BAR0:{:#x}, Ports:{}, ConnectedMask:{:#x})",
                idx, bar0, num_ports, connected
            );
            (*addr_of_mut!(EHCI_CTRLS))[idx] = Some(EhciController {
                cap_base,
                op_base,
                num_ports,
                connected_ports: connected,
            });
            *addr_of_mut!(EHCI_COUNT) = idx + 1;
        }
    }
}

pub fn is_active() -> bool {
    unsafe { *addr_of!(EHCI_COUNT) > 0 }
}
