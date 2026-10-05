use alloc::vec::Vec;
use crate::drivers::pci::pci_read_u32;
use crate::mm::frame::allocate_frame_safe;
use crate::mm::paging::VMM;

const REG_CTRL: u16 = 0x0000;
const REG_STATUS: u16 = 0x0008;
const REG_RCTL: u16 = 0x0100;
const REG_RDBAL: u16 = 0x2800;
const REG_RDBAH: u16 = 0x2804;
const REG_RDLEN: u16 = 0x2808;
const REG_RDH: u16 = 0x2810;
const REG_RDT: u16 = 0x2818;

const REG_TCTL: u16 = 0x0400;
const REG_TDBAL: u16 = 0x3800;
const REG_TDBAH: u16 = 0x3804;
const REG_TDLEN: u16 = 0x3808;
const REG_TDH: u16 = 0x3810;
const REG_TDT: u16 = 0x3818;
const REG_MTA: u16 = 0x5200;

const RCTL_EN: u32 = 1 << 1;
const RCTL_SBP: u32 = 1 << 2;
const RCTL_UPE: u32 = 1 << 3;
const RCTL_MPE: u32 = 1 << 4;
const RCTL_BAM: u32 = 1 << 15;
const RCTL_BSIZE_2048: u32 = 0 << 16;
const RCTL_SECRC: u32 = 1 << 26;

const TCTL_EN: u32 = 1 << 1;
const TCTL_PSP: u32 = 1 << 3;

const NUM_TX_DESC: usize = 32;
const NUM_RX_DESC: usize = 32;
const BUFFER_SIZE: usize = 2048;

#[repr(C, packed)]
#[derive(Copy, Clone, Default)]
pub struct E1000TxDesc {
    pub addr: u64,
    pub length: u16,
    pub cso: u8,
    pub cmd: u8,
    pub status: u8,
    pub css: u8,
    pub special: u16,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Default)]
pub struct E1000RxDesc {
    pub addr: u64,
    pub length: u16,
    pub checksum: u16,
    pub status: u8,
    pub errors: u8,
    pub special: u16,
}

pub struct E1000Driver {
    pub bar0_mem: u64,
    pub tx_descs: *mut E1000TxDesc,
    pub rx_descs: *mut E1000RxDesc,
    pub tx_buffers: [*mut u8; NUM_TX_DESC],
    pub rx_buffers: [*mut u8; NUM_RX_DESC],
    pub tx_tail: usize,
    pub rx_cur: usize,
}

unsafe impl Send for E1000Driver {}
unsafe impl Sync for E1000Driver {}

static mut E1000_STORAGE: Option<E1000Driver> = None;

#[inline]
unsafe fn pci_enable_bus_mastering(bus: u8, dev: u8, func: u8) {
    unsafe {
        let addr = (1u32 << 31) | ((bus as u32) << 16) | ((dev as u32) << 11) | ((func as u32) << 8) | 0x04;
        core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") addr);
        let mut val: u16;
        core::arch::asm!("in ax, dx", in("dx") 0xCFCu16, out("ax") val);
        val |= (1 << 2) | (1 << 1) | (1 << 0); // Bus Master + Memory Space + I/O Space
        core::arch::asm!("out dx, ax", in("dx") 0xCFCu16, in("ax") val);
    }
}

impl E1000Driver {
    #[inline(always)]
    fn write_reg(&self, offset: u16, val: u32) {
        unsafe {
            let ptr = (self.bar0_mem + offset as u64) as *mut u32;
            core::ptr::write_volatile(ptr, val);
        }
    }

    #[inline(always)]
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

    pub fn send_packet(&mut self, data: &[u8]) {
        if data.len() > BUFFER_SIZE {
            return;
        }

        let desc = unsafe { &mut *self.tx_descs.add(self.tx_tail) };

        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), self.tx_buffers[self.tx_tail], data.len());
        }

        desc.length = data.len() as u16;
        desc.cmd = (1 << 0) | (1 << 1) | (1 << 3); // EOP | IFCS | RS
        desc.status = 0;

        self.tx_tail = (self.tx_tail + 1) % NUM_TX_DESC;
        self.write_reg(REG_TDT, self.tx_tail as u32);

        let mut spins = 0;
        while (unsafe { core::ptr::read_volatile(&desc.status) } & 0x01) == 0 && spins < 50_000 {
            core::hint::spin_loop();
            spins += 1;
        }
    }

    pub fn receive_packet(&mut self) -> Option<Vec<u8>> {
        let desc = unsafe { &mut *self.rx_descs.add(self.rx_cur) };
        let status = unsafe { core::ptr::read_volatile(&desc.status) };

        if (status & 0x01) == 0 {
            return None;
        }

        let len = desc.length as usize;
        let mut packet = alloc::vec![0u8; len];
        unsafe {
            core::ptr::copy_nonoverlapping(self.rx_buffers[self.rx_cur], packet.as_mut_ptr(), len);
        }

        desc.status = 0;
        let old_cur = self.rx_cur;
        self.rx_cur = (self.rx_cur + 1) % NUM_RX_DESC;
        self.write_reg(REG_RDT, old_cur as u32);

        Some(packet)
    }
}

pub fn get_driver() -> Option<&'static mut E1000Driver> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(E1000_STORAGE);
        (*ptr).as_mut()
    }
}

pub fn init() {
    let pci_ptr = core::ptr::addr_of!(crate::drivers::pci::PCI_DEVICES);
    let devices = match unsafe { (*pci_ptr).as_ref() } {
        Some(d) => d,
        None => return,
    };

    for dev in devices {
        if dev.vendor_id == 0x8086 && dev.device_id == 0x100E {
            let bar0 = unsafe { pci_read_u32(dev.bus, dev.device, dev.function, 0x10) };
            let mem_base = (bar0 & !0xF) as u64;

            unsafe { pci_enable_bus_mastering(dev.bus, dev.device, dev.function); }

            let hhdm = unsafe { (*core::ptr::addr_of!(VMM)).unwrap().hhdm_offset };

            let tx_ring_phys = allocate_frame_safe().expect("OOM E1000 TX Ring");
            let rx_ring_phys = allocate_frame_safe().expect("OOM E1000 RX Ring");

            let tx_descs = (tx_ring_phys + hhdm) as *mut E1000TxDesc;
            let rx_descs = (rx_ring_phys + hhdm) as *mut E1000RxDesc;

            unsafe {
                core::ptr::write_bytes(tx_descs as *mut u8, 0, 4096);
                core::ptr::write_bytes(rx_descs as *mut u8, 0, 4096);
            }

            let mut tx_buffers = [core::ptr::null_mut(); NUM_TX_DESC];
            let mut rx_buffers = [core::ptr::null_mut(); NUM_RX_DESC];

            for i in 0..NUM_TX_DESC {
                let frame_phys = allocate_frame_safe().expect("OOM E1000 TX Buffer");
                tx_buffers[i] = (frame_phys + hhdm) as *mut u8;
                unsafe {
                    let desc = &mut *tx_descs.add(i);
                    desc.addr = frame_phys;
                    desc.cmd = 0;
                    desc.status = 1;
                }
            }

            for i in 0..NUM_RX_DESC {
                let frame_phys = allocate_frame_safe().expect("OOM E1000 RX Buffer");
                rx_buffers[i] = (frame_phys + hhdm) as *mut u8;
                unsafe {
                    let desc = &mut *rx_descs.add(i);
                    desc.addr = frame_phys;
                    desc.status = 0;
                }
            }

            let driver = E1000Driver {
                bar0_mem: mem_base,
                tx_descs,
                rx_descs,
                tx_buffers,
                rx_buffers,
                tx_tail: 0,
                rx_cur: 0,
            };

            // Set Link Up
            let ctrl = driver.read_reg(REG_CTRL);
            driver.write_reg(REG_CTRL, ctrl | (1 << 6));

            // Clear Multicast Table Array
            for i in 0..128 {
                driver.write_reg(REG_MTA + (i * 4), 0);
            }

            // RX Ring setup
            driver.write_reg(REG_RDBAL, rx_ring_phys as u32);
            driver.write_reg(REG_RDBAH, (rx_ring_phys >> 32) as u32);
            driver.write_reg(REG_RDLEN, (NUM_RX_DESC * core::mem::size_of::<E1000RxDesc>()) as u32);
            driver.write_reg(REG_RDH, 0);
            driver.write_reg(REG_RDT, (NUM_RX_DESC - 1) as u32);
            driver.write_reg(REG_RCTL, RCTL_EN | RCTL_SBP | RCTL_UPE | RCTL_MPE | RCTL_BAM | RCTL_BSIZE_2048 | RCTL_SECRC);

            // TX Ring setup
            driver.write_reg(REG_TDBAL, tx_ring_phys as u32);
            driver.write_reg(REG_TDBAH, (tx_ring_phys >> 32) as u32);
            driver.write_reg(REG_TDLEN, (NUM_TX_DESC * core::mem::size_of::<E1000TxDesc>()) as u32);
            driver.write_reg(REG_TDH, 0);
            driver.write_reg(REG_TDT, 0);
            driver.write_reg(REG_TCTL, TCTL_EN | TCTL_PSP | (0x0F << 4) | (0x40 << 12));

            unsafe {
                let storage_ptr = core::ptr::addr_of_mut!(E1000_STORAGE);
                *storage_ptr = Some(driver);
            }

            crate::log_info!("E1000", "E1000 Hardware DMA Initialized (MMIO: {:#010x})", mem_base);
            return;
        }
    }
}
