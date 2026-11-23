use alloc::vec::Vec;
use core::arch::asm;
use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::drivers::pci::{pci_enable_bus_mastering, pci_read_bar};
use crate::mm::paging::VMM;

const AC97_VENDOR: u16 = 0x8086;
const AC97_DEVICE: u16 = 0x2415;
const BDL_COUNT: usize = 32;
const BUF_BYTES: usize = 4096;

#[inline(always)]
unsafe fn outb(port: u16, val: u8) { unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack)); } }
#[inline(always)]
unsafe fn inb(port: u16) -> u8 { let v: u8; unsafe { asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack)); } v }
#[inline(always)]
unsafe fn outw(port: u16, val: u16) { unsafe { asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack)); } }
#[inline(always)]
unsafe fn outl(port: u16, val: u32) { unsafe { asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack)); } }

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct BdlEntry {
    addr: u32,
    samples: u16,
    flags: u16,
}

pub struct Ac97Driver {
    nam: u16,
    nabm: u16,
    bdl: Vec<BdlEntry>,
    pcm_pool: Vec<u8>,
    next_idx: usize,
}

static AUDIO_LOCK: AtomicBool = AtomicBool::new(false);
static mut AC97: Option<Ac97Driver> = None;

fn lock_audio() {
    while AUDIO_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}
fn unlock_audio() { AUDIO_LOCK.store(false, Ordering::Release); }

pub fn is_active() -> bool {
    unsafe { (*addr_of!(AC97)).is_some() }
}

pub fn init() {
    let devices = match unsafe { (*addr_of!(crate::drivers::pci::PCI_DEVICES)).as_ref() } {
        Some(d) => d,
        None => return,
    };
    let hhdm = match unsafe { (*addr_of!(VMM)).as_ref() } {
        Some(v) => v.hhdm_offset,
        None => return,
    };

    for dev in devices {
        if dev.vendor_id != AC97_VENDOR || dev.device_id != AC97_DEVICE {
            continue;
        }
        unsafe {
            pci_enable_bus_mastering(dev.bus, dev.device, dev.function);
            let nam = (pci_read_bar(dev.bus, dev.device, dev.function, 0) & 0xFFFF) as u16;
            let nabm = (pci_read_bar(dev.bus, dev.device, dev.function, 1) & 0xFFFF) as u16;
            if nam == 0 || nabm == 0 { return; }

            // Cold Reset AC'97 Link
            outl(nabm + 0x2C, 0x02);
            for _ in 0..100_000 { core::hint::spin_loop(); }

            // Reset Mixer & Unmute Master + PCM Out (0 dB)
            outw(nam + 0x00, 0x0000);
            outw(nam + 0x02, 0x0404); // Master Volume (~85%)
            outw(nam + 0x18, 0x0000); // PCM Out Volume (100%)

            // Reset PCM Out Bus Master Channel
            outb(nabm + 0x1B, 0x02);
            let mut guard = 0;
            while (inb(nabm + 0x1B) & 0x02) != 0 && guard < 100_000 {
                core::hint::spin_loop();
                guard += 1;
            }

            let mut bdl = alloc::vec![BdlEntry::default(); BDL_COUNT];
            let pcm_pool = alloc::vec![0u8; BDL_COUNT * BUF_BYTES];
            let pool_phys = (pcm_pool.as_ptr() as u64).saturating_sub(hhdm);
            let bdl_phys = (bdl.as_ptr() as u64).saturating_sub(hhdm);

            for i in 0..BDL_COUNT {
                bdl[i] = BdlEntry {
                    addr: (pool_phys + (i * BUF_BYTES) as u64) as u32,
                    samples: (BUF_BYTES / 2) as u16,
                    flags: 0,
                };
            }

            outl(nabm + 0x10, bdl_phys as u32);
            outb(nabm + 0x15, 0);

            *addr_of_mut!(AC97) = Some(Ac97Driver {
                nam,
                nabm,
                bdl,
                pcm_pool,
                next_idx: 0,
            });
            crate::log_info!("AUDIO", "Intel AC'97 Hardware Audio Initialized (NAM:{:#x} NABM:{:#x} 48kHz Stereo)", nam, nabm);
            return;
        }
    }
}

pub fn set_volume(vol_pct: u8) {
    lock_audio();
    unsafe {
        if let Some(drv) = (*addr_of_mut!(AC97)).as_mut() {
            if vol_pct == 0 {
                outw(drv.nam + 0x02, 0x8000);
            } else {
                let v = vol_pct.min(100) as u16;
                let att = ((100 - v) * 31) / 100;
                outw(drv.nam + 0x02, (att << 8) | att);
            }
        }
    }
    unlock_audio();
}

pub fn write_pcm(data: &[u8]) -> usize {
    if data.is_empty() { return 0; }
    lock_audio();
    let mut written = 0usize;
    unsafe {
        if let Some(drv) = (*addr_of_mut!(AC97)).as_mut() {
            let mut offset = 0usize;
            while offset < data.len() {
                let chunk = (data.len() - offset).min(BUF_BYTES) & !3;
                if chunk == 0 { break; }

                let idx = drv.next_idx;
                let running = (inb(drv.nabm + 0x1B) & 0x01) != 0;
                if running {
                    let civ = inb(drv.nabm + 0x14) as usize;
                    let next_after = (idx + 1) % BDL_COUNT;
                    if next_after == civ {
                        break; // DMA buffer full — return written bytes without blocking
                    }
                }

                let dst_off = idx * BUF_BYTES;
                drv.pcm_pool[dst_off..dst_off + chunk].copy_from_slice(&data[offset..offset + chunk]);
                drv.bdl[idx].samples = (chunk / 2) as u16;

                outb(drv.nabm + 0x15, idx as u8);
                if (inb(drv.nabm + 0x1B) & 0x01) == 0 {
                    outb(drv.nabm + 0x1B, 0x01); // Start DMA playback
                }

                drv.next_idx = (idx + 1) % BDL_COUNT;
                offset += chunk;
                written += chunk;
            }
        }
    }
    unlock_audio();
    written
}
