use alloc::vec::Vec;
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering, fence};
use crate::drivers::pci::{pci_enable_bus_mastering, pci_read_bar, pci_read_u16, pci_read_u32, pci_read_u8};
use crate::mm::frame::allocate_frame_safe;
use crate::mm::paging::{VMM, map_mmio_pages};

const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_GPU_DEVICE_ID: u16 = 0x1050;

const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;

const VIRTIO_STATUS_ACK: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;

const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;
const QUEUE_SIZE: usize = 64;

const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;
const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;

const VIRTIO_GPU_CMD_CTX_CREATE: u32 = 0x0200;
const VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE: u32 = 0x0202;
const VIRTIO_GPU_CMD_SUBMIT_3D: u32 = 0x0207;

const VIRTIO_GPU_CMD_UPDATE_CURSOR: u32 = 0x0300;
const VIRTIO_GPU_CMD_MOVE_CURSOR: u32 = 0x0301;

const VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM: u32 = 1;
const VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM: u32 = 2;
const RESOURCE_IDS: [u32; 2] = [1, 2];
const CURSOR_RESOURCE_ID: u32 = 3;
const VIRGL_CTX_ID: u32 = 1;

#[repr(C)]
struct VirtioPciCommonCfg {
    device_feature_select: u32,
    device_feature: u32,
    driver_feature_select: u32,
    driver_feature: u32,
    msix_config: u16,
    num_queues: u16,
    device_status: u8,
    config_generation: u8,
    queue_select: u16,
    queue_size: u16,
    queue_msix_vector: u16,
    queue_enable: u16,
    queue_notify_off: u16,
    queue_desc: u64,
    queue_driver: u64,
    queue_device: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; QUEUE_SIZE],
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; QUEUE_SIZE],
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct VirtioGpuCtrlHdr {
    hdr_type: u32,
    flags: u32,
    fence_id: u64,
    ctx_id: u32,
    ring_idx: u8,
    padding: [u8; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct VirtioGpuRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VirtioGpuResourceCreate2d {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    format: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VirtioGpuAttachBacking {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    nr_entries: u32,
    addr: u64,
    length: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VirtioGpuSetScanout {
    hdr: VirtioGpuCtrlHdr,
    r: VirtioGpuRect,
    scanout_id: u32,
    resource_id: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VirtioGpuTransferToHost2d {
    hdr: VirtioGpuCtrlHdr,
    r: VirtioGpuRect,
    offset: u64,
    resource_id: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VirtioGpuResourceFlush {
    hdr: VirtioGpuCtrlHdr,
    r: VirtioGpuRect,
    resource_id: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VirtioGpuCtxCreate {
    hdr: VirtioGpuCtrlHdr,
    nlen: u32,
    context_init: u32,
    debug_name: [u8; 64],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VirtioGpuCtxResource {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VirtioGpuCmdSubmit {
    hdr: VirtioGpuCtrlHdr,
    size: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct VirtioGpuCursorPos {
    scanout_id: u32,
    x: u32,
    y: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VirtioGpuUpdateCursor {
    hdr: VirtioGpuCtrlHdr,
    pos: VirtioGpuCursorPos,
    resource_id: u32,
    hot_x: u32,
    hot_y: u32,
    padding: u32,
}

pub struct VirtioGpuDriver {
    notify_addr: *mut u16,
    desc: *mut VirtqDesc,
    avail: *mut VirtqAvail,
    used: *mut VirtqUsed,
    q_size: usize,
    avail_idx: u16,
    last_used_idx: u16,
    cmd_phys: u64,
    cmd_virt: *mut u8,
    // Cursor Queue (Queue 1)
    cursor_notify_addr: *mut u16,
    cursor_desc: *mut VirtqDesc,
    cursor_avail: *mut VirtqAvail,
    cursor_used: *mut VirtqUsed,
    cursor_q_size: usize,
    cursor_avail_idx: u16,
    cursor_last_used_idx: u16,
    cursor_cmd_phys: u64,
    cursor_cmd_virt: *mut u8,
    pub hw_cursor_active: bool,
    pub cursor_pixels: Vec<u32>,
    pub width: usize,
    pub height: usize,
    pub virgl_supported: bool,
    pub virgl_3d_active: bool,
    pub buffers: [Vec<u32>; 2],
    pub front_idx: usize,
    pub prev_dirty: Vec<[u32; 4]>,
}

unsafe impl Send for VirtioGpuDriver {}
unsafe impl Sync for VirtioGpuDriver {}

static GPU_LOCK: AtomicBool = AtomicBool::new(false);
static mut VIRTIO_GPU: Option<VirtioGpuDriver> = None;

fn lock_gpu() {
    while GPU_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}

fn unlock_gpu() {
    GPU_LOCK.store(false, Ordering::Release);
}

impl VirtioGpuDriver {
    fn send_cmd<T: Copy>(&mut self, req: &T) -> u32 {
        let req_len = core::mem::size_of::<T>();
        let resp_offset = 2048usize;
        let resp_len = 512usize;

        unsafe {
            core::ptr::copy_nonoverlapping(req as *const T as *const u8, self.cmd_virt, req_len);
            core::ptr::write_bytes(self.cmd_virt.add(resp_offset), 0, resp_len);

            let d0 = &mut *self.desc.add(0);
            d0.addr = self.cmd_phys;
            d0.len = req_len as u32;
            d0.flags = VIRTQ_DESC_F_NEXT;
            d0.next = 1;

            let d1 = &mut *self.desc.add(1);
            d1.addr = self.cmd_phys + resp_offset as u64;
            d1.len = resp_len as u32;
            d1.flags = VIRTQ_DESC_F_WRITE;
            d1.next = 0;

            let slot = (self.avail_idx as usize) % self.q_size;
            write_volatile(&mut (*self.avail).ring[slot], 0u16);
            fence(Ordering::SeqCst);
            self.avail_idx = self.avail_idx.wrapping_add(1);
            write_volatile(&mut (*self.avail).idx, self.avail_idx);
            fence(Ordering::SeqCst);

            write_volatile(self.notify_addr, 0u16);

            let mut spins = 0u32;
            while read_volatile(&(*self.used).idx) == self.last_used_idx && spins < 5_000_000 {
                core::hint::spin_loop();
                spins += 1;
            }
            self.last_used_idx = read_volatile(&(*self.used).idx);

            let resp = read_volatile(self.cmd_virt.add(resp_offset) as *const VirtioGpuCtrlHdr);
            resp.hdr_type
        }
    }

    pub fn commit_back_buffer(&mut self, back_idx: usize, rx: usize, ry: usize, rw: usize, rh: usize) {
        if rw == 0 || rh == 0 { return; }
        let res_id = RESOURCE_IDS[back_idx];
        let rect = VirtioGpuRect {
            x: rx as u32,
            y: ry as u32,
            width: rw as u32,
            height: rh as u32,
        };
        let offset = ((ry * self.width + rx) * 4) as u64;

        let xfer = VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D, ..Default::default() },
            r: rect,
            offset,
            resource_id: res_id,
            padding: 0,
        };
        let _ = self.send_cmd(&xfer);

        // Atomic Scanout Flip (Wayland Every-Frame-Is-Perfect)
        let scanout = VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_SET_SCANOUT, ..Default::default() },
            r: VirtioGpuRect { x: 0, y: 0, width: self.width as u32, height: self.height as u32 },
            scanout_id: 0,
            resource_id: res_id,
        };
        let _ = self.send_cmd(&scanout);

        let flush = VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_RESOURCE_FLUSH, ..Default::default() },
            r: rect,
            resource_id: res_id,
            padding: 0,
        };
        let _ = self.send_cmd(&flush);

        self.front_idx = back_idx;
    }

    fn send_cursor_cmd(&mut self, req: &VirtioGpuUpdateCursor) {
        if !self.hw_cursor_active { return; }
        let req_len = core::mem::size_of::<VirtioGpuUpdateCursor>();
        unsafe {
            core::ptr::copy_nonoverlapping(req as *const VirtioGpuUpdateCursor as *const u8, self.cursor_cmd_virt, req_len);
            let d0 = &mut *self.cursor_desc.add(0);
            d0.addr = self.cursor_cmd_phys;
            d0.len = req_len as u32;
            d0.flags = 0;
            d0.next = 0;

            let slot = (self.cursor_avail_idx as usize) % self.cursor_q_size;
            write_volatile(&mut (*self.cursor_avail).ring[slot], 0u16);
            fence(Ordering::SeqCst);
            self.cursor_avail_idx = self.cursor_avail_idx.wrapping_add(1);
            write_volatile(&mut (*self.cursor_avail).idx, self.cursor_avail_idx);
            fence(Ordering::SeqCst);

            write_volatile(self.cursor_notify_addr, 1u16);

            let mut spins = 0u32;
            while read_volatile(&(*self.cursor_used).idx) == self.cursor_last_used_idx && spins < 500_000 {
                core::hint::spin_loop();
                spins += 1;
            }
            self.cursor_last_used_idx = read_volatile(&(*self.cursor_used).idx);
        }
    }

    pub fn submit_3d_cmds(&mut self, words: &[u32]) -> bool {
        if !self.virgl_3d_active || words.is_empty() { return false; }
        let payload_bytes = words.len() * 4;
        let hdr_len = core::mem::size_of::<VirtioGpuCmdSubmit>();
        if hdr_len + payload_bytes > 2048 { return false; }

        let submit = VirtioGpuCmdSubmit {
            hdr: VirtioGpuCtrlHdr {
                hdr_type: VIRTIO_GPU_CMD_SUBMIT_3D,
                ctx_id: VIRGL_CTX_ID,
                ..Default::default()
            },
            size: payload_bytes as u32,
            padding: 0,
        };

        let resp_offset = 2048usize;
        unsafe {
            core::ptr::copy_nonoverlapping(&submit as *const VirtioGpuCmdSubmit as *const u8, self.cmd_virt, hdr_len);
            core::ptr::copy_nonoverlapping(words.as_ptr() as *const u8, self.cmd_virt.add(hdr_len), payload_bytes);
            core::ptr::write_bytes(self.cmd_virt.add(resp_offset), 0, 64);

            let d0 = &mut *self.desc.add(0);
            d0.addr = self.cmd_phys;
            d0.len = (hdr_len + payload_bytes) as u32;
            d0.flags = VIRTQ_DESC_F_NEXT;
            d0.next = 1;

            let d1 = &mut *self.desc.add(1);
            d1.addr = self.cmd_phys + resp_offset as u64;
            d1.len = 64;
            d1.flags = VIRTQ_DESC_F_WRITE;
            d1.next = 0;

            let slot = (self.avail_idx as usize) % self.q_size;
            write_volatile(&mut (*self.avail).ring[slot], 0u16);
            fence(Ordering::SeqCst);
            self.avail_idx = self.avail_idx.wrapping_add(1);
            write_volatile(&mut (*self.avail).idx, self.avail_idx);
            fence(Ordering::SeqCst);

            write_volatile(self.notify_addr, 0u16);

            let mut spins = 0u32;
            while read_volatile(&(*self.used).idx) == self.last_used_idx && spins < 5_000_000 {
                core::hint::spin_loop();
                spins += 1;
            }
            self.last_used_idx = read_volatile(&(*self.used).idx);
            let resp = read_volatile(self.cmd_virt.add(resp_offset) as *const VirtioGpuCtrlHdr);
            resp.hdr_type == 0x1100
        }
    }
}

pub fn move_cursor(x: u32, y: u32) -> bool {
    lock_gpu();
    let active = unsafe {
        if let Some(gpu) = (*addr_of_mut!(VIRTIO_GPU)).as_mut() {
            if gpu.hw_cursor_active {
                let cmd = VirtioGpuUpdateCursor {
                    hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_MOVE_CURSOR, ..Default::default() },
                    pos: VirtioGpuCursorPos { scanout_id: 0, x, y, padding: 0 },
                    resource_id: CURSOR_RESOURCE_ID,
                    hot_x: 0,
                    hot_y: 0,
                    padding: 0,
                };
                gpu.send_cursor_cmd(&cmd);
                true
            } else {
                false
            }
        } else {
            false
        }
    };
    unlock_gpu();
    active
}

pub fn submit_3d(words: &[u32]) -> bool {
    lock_gpu();
    let ok = unsafe {
        if let Some(gpu) = (*addr_of_mut!(VIRTIO_GPU)).as_mut() {
            gpu.submit_3d_cmds(words)
        } else {
            false
        }
    };
    unlock_gpu();
    ok
}

pub fn is_active() -> bool {
    unsafe { (*addr_of!(VIRTIO_GPU)).is_some() }
}

/// Returns [width, height, is_active, virgl_3d_active, hw_cursor_active, backend_id]
/// backend_id: 0 = Software/Limine, 1 = VirtIO-GPU 2D, 2 = VirtIO-GPU VirGL 3D
pub fn get_gpu_info() -> [u32; 6] {
    unsafe {
        if let Some(gpu) = (*addr_of!(VIRTIO_GPU)).as_ref() {
            let backend = if gpu.virgl_3d_active { 2 } else { 1 };
            [
                gpu.width as u32,
                gpu.height as u32,
                1,
                if gpu.virgl_3d_active { 1 } else { 0 },
                if gpu.hw_cursor_active { 1 } else { 0 },
                backend,
            ]
        } else if let Some(w) = (*addr_of!(crate::writer::WRITER)).as_ref() {
            [w.width as u32, w.height as u32, 0, 0, 0, 0]
        } else {
            [2560, 1440, 0, 0, 0, 0]
        }
    }
}

pub fn present_full(src: *const u32, src_w: usize, src_h: usize) {
    lock_gpu();
    unsafe {
        if let Some(gpu) = (*addr_of_mut!(VIRTIO_GPU)).as_mut() {
            let back_idx = 1 - gpu.front_idx;
            let copy_w = src_w.min(gpu.width);
            let copy_h = src_h.min(gpu.height);
            let dst = gpu.buffers[back_idx].as_mut_ptr();
            for y in 0..copy_h {
                core::ptr::copy_nonoverlapping(src.add(y * src_w), dst.add(y * gpu.width), copy_w);
            }
            gpu.prev_dirty.clear();
            gpu.prev_dirty.push([0, 0, copy_w as u32, copy_h as u32]);
            gpu.commit_back_buffer(back_idx, 0, 0, copy_w, copy_h);
        }
    }
    unlock_gpu();
}

pub fn present_rects(src: *const u32, src_w: usize, src_h: usize, rects: &[[u32; 4]]) {
    lock_gpu();
    unsafe {
        if let Some(gpu) = (*addr_of_mut!(VIRTIO_GPU)).as_mut() {
            let back_idx = 1 - gpu.front_idx;
            let max_w = src_w.min(gpu.width);
            let max_h = src_h.min(gpu.height);
            let dst = gpu.buffers[back_idx].as_mut_ptr();

            let mut min_x = usize::MAX;
            let mut min_y = usize::MAX;
            let mut max_ex = 0usize;
            let mut max_ey = 0usize;

            // Buffer Age = 2: مزامنة مناطق الإطار السابق + الحالي في الـ Back Buffer قبل القلب الذري
            for r in gpu.prev_dirty.iter().chain(rects.iter()) {
                let rx = r[0] as usize;
                let ry = r[1] as usize;
                let rw = r[2] as usize;
                let rh = r[3] as usize;
                let ex = rx.saturating_add(rw).min(max_w);
                let ey = ry.saturating_add(rh).min(max_h);
                if rx >= ex || ry >= ey { continue; }

                let line_len = ex - rx;
                for y in ry..ey {
                    core::ptr::copy_nonoverlapping(src.add(y * src_w + rx), dst.add(y * gpu.width + rx), line_len);
                }
                if rx < min_x { min_x = rx; }
                if ry < min_y { min_y = ry; }
                if ex > max_ex { max_ex = ex; }
                if ey > max_ey { max_ey = ey; }
            }

            if min_x < max_ex && min_y < max_ey {
                gpu.commit_back_buffer(back_idx, min_x, min_y, max_ex - min_x, max_ey - min_y);
            }
            gpu.prev_dirty.clear();
            gpu.prev_dirty.extend_from_slice(rects);
        }
    }
    unlock_gpu();
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
        if dev.vendor_id != VIRTIO_VENDOR_ID || dev.device_id != VIRTIO_GPU_DEVICE_ID {
            continue;
        }

        unsafe {
            pci_enable_bus_mastering(dev.bus, dev.device, dev.function);
            let status = pci_read_u16(dev.bus, dev.device, dev.function, 0x06);
            if (status & (1 << 4)) == 0 {
                crate::log_error!("VIRTIO-GPU", "PCI Capabilities list missing.");
                return;
            }

            let mut common_phys = 0u64;
            let mut notify_phys = 0u64;
            let mut notify_mult = 0u32;

            let mut cap_ptr = pci_read_u8(dev.bus, dev.device, dev.function, 0x34) & !0x3;
            let mut guard = 0;
            while cap_ptr != 0 && guard < 64 {
                guard += 1;
                let cap_id = pci_read_u8(dev.bus, dev.device, dev.function, cap_ptr);
                let next_ptr = pci_read_u8(dev.bus, dev.device, dev.function, cap_ptr + 1) & !0x3;
                if cap_id == 0x09 {
                    let cfg_type = pci_read_u8(dev.bus, dev.device, dev.function, cap_ptr + 3);
                    let bar_idx = pci_read_u8(dev.bus, dev.device, dev.function, cap_ptr + 4);
                    let offset = pci_read_u32(dev.bus, dev.device, dev.function, cap_ptr + 8) as u64;
                    let bar_base = pci_read_bar(dev.bus, dev.device, dev.function, bar_idx);
                    if bar_base != 0 {
                        if cfg_type == VIRTIO_PCI_CAP_COMMON_CFG {
                            common_phys = bar_base + offset;
                        } else if cfg_type == VIRTIO_PCI_CAP_NOTIFY_CFG {
                            notify_phys = bar_base + offset;
                            notify_mult = pci_read_u32(dev.bus, dev.device, dev.function, cap_ptr + 16);
                        }
                    }
                }
                cap_ptr = next_ptr;
            }

            if common_phys == 0 || notify_phys == 0 {
                crate::log_error!("VIRTIO-GPU", "Failed to locate VirtIO Common/Notify MMIO BARs.");
                return;
            }

            let common_virt = match map_mmio_pages(common_phys, 2) {
                Ok(v) => v as *mut VirtioPciCommonCfg,
                Err(e) => { crate::log_error!("VIRTIO-GPU", "MMIO map common failed: {}", e); return; }
            };
            let notify_virt_base = match map_mmio_pages(notify_phys, 2) {
                Ok(v) => v,
                Err(e) => { crate::log_error!("VIRTIO-GPU", "MMIO map notify failed: {}", e); return; }
            };

            // Modern VirtIO 1.0 Init Sequence
            write_volatile(&mut (*common_virt).device_status, 0);
            write_volatile(&mut (*common_virt).device_status, VIRTIO_STATUS_ACK);
            write_volatile(&mut (*common_virt).device_status, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER);

            write_volatile(&mut (*common_virt).device_feature_select, 0);
            let feat_lo = read_volatile(&(*common_virt).device_feature);
            let virgl_supported = (feat_lo & 1) != 0;

            write_volatile(&mut (*common_virt).driver_feature_select, 0);
            write_volatile(&mut (*common_virt).driver_feature, if virgl_supported { 1 } else { 0 });
            write_volatile(&mut (*common_virt).driver_feature_select, 1);
            write_volatile(&mut (*common_virt).driver_feature, 1); // VIRTIO_F_VERSION_1

            write_volatile(
                &mut (*common_virt).device_status,
                VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
            );
            if (read_volatile(&(*common_virt).device_status) & VIRTIO_STATUS_FEATURES_OK) == 0 {
                crate::log_error!("VIRTIO-GPU", "Device rejected VirtIO 1.0 features.");
                return;
            }

            // Setup controlq (Queue 0)
            write_volatile(&mut (*common_virt).queue_select, 0);
            let max_q = read_volatile(&(*common_virt).queue_size) as usize;
            let q_size = max_q.min(QUEUE_SIZE);
            write_volatile(&mut (*common_virt).queue_size, q_size as u16);

            let desc_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let avail_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let used_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let cmd_phys = match allocate_frame_safe() { Some(f) => f, None => return };

            core::ptr::write_bytes((desc_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((avail_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((used_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((cmd_phys + hhdm) as *mut u8, 0, 4096);

            write_volatile(&mut (*common_virt).queue_desc, desc_phys);
            write_volatile(&mut (*common_virt).queue_driver, avail_phys);
            write_volatile(&mut (*common_virt).queue_device, used_phys);
            let notify_off = read_volatile(&(*common_virt).queue_notify_off) as u64;
            write_volatile(&mut (*common_virt).queue_enable, 1);

            // Setup cursorq (Queue 1) for Hardware Cursor
            write_volatile(&mut (*common_virt).queue_select, 1);
            let c_max_q = read_volatile(&(*common_virt).queue_size) as usize;
            let cursor_q_size = c_max_q.min(QUEUE_SIZE).max(16);
            write_volatile(&mut (*common_virt).queue_size, cursor_q_size as u16);

            let c_desc_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let c_avail_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let c_used_phys = match allocate_frame_safe() { Some(f) => f, None => return };
            let c_cmd_phys = match allocate_frame_safe() { Some(f) => f, None => return };

            core::ptr::write_bytes((c_desc_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((c_avail_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((c_used_phys + hhdm) as *mut u8, 0, 4096);
            core::ptr::write_bytes((c_cmd_phys + hhdm) as *mut u8, 0, 4096);

            write_volatile(&mut (*common_virt).queue_desc, c_desc_phys);
            write_volatile(&mut (*common_virt).queue_driver, c_avail_phys);
            write_volatile(&mut (*common_virt).queue_device, c_used_phys);
            let c_notify_off = read_volatile(&(*common_virt).queue_notify_off) as u64;
            write_volatile(&mut (*common_virt).queue_enable, 1);

            write_volatile(
                &mut (*common_virt).device_status,
                VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK | VIRTIO_STATUS_DRIVER_OK,
            );

            let notify_addr = (notify_virt_base + (notify_off * notify_mult as u64)) as *mut u16;
            let cursor_notify_addr = (notify_virt_base + (c_notify_off * notify_mult as u64)) as *mut u16;
            let width = 2560usize;
            let height = 1440usize;
            let mut buf0 = alloc::vec![0xFF000000u32; width * height];
            let buf1 = alloc::vec![0xFF000000u32; width * height];

            // Render EOS Boot Splash directly onto VirtIO-GPU scanout buffer
            let bmp_w = crate::splash_data::SPLASH_BITMAP_W;
            let bmp_h = crate::splash_data::SPLASH_BITMAP_H;
            let start_x = width.saturating_sub(bmp_w) / 2;
            let start_y = (height.saturating_sub(bmp_h) / 2).saturating_sub(40);
            let bytes_per_row = (bmp_w + 7) / 8;
            for by in 0..bmp_h {
                let py = start_y + by;
                if py >= height { break; }
                let row_off = py * width;
                for bx in 0..bytes_per_row {
                    let bidx = by * bytes_per_row + bx;
                    if bidx >= crate::splash_data::SPLASH_BITMAP.len() { break; }
                    let val = crate::splash_data::SPLASH_BITMAP[bidx];
                    if val == 0 { continue; }
                    for bit in 0..8 {
                        if (val & (1 << (7 - bit))) != 0 {
                            let px = start_x + bx * 8 + bit;
                            if px < width {
                                buf0[row_off + px] = 0xFFFFFFFF;
                            }
                        }
                    }
                }
            }
            let dot_y = (start_y + bmp_h + 44) as isize;
            let mid_x = (width / 2) as isize;
            for i in 0..3isize {
                let cx = mid_x + (i - 1) * 32;
                let r: isize = if i == 1 { 7 } else { 5 };
                let col: u32 = if i == 1 { 0xFF38BDF8 } else { 0xFF475569 };
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx * dx + dy * dy <= r * r {
                            let px = (cx + dx) as usize;
                            let py = (dot_y + dy) as usize;
                            if px < width && py < height {
                                buf0[py * width + px] = col;
                            }
                        }
                    }
                }
            }
            let phys0 = (buf0.as_ptr() as u64) - hhdm;
            let phys1 = (buf1.as_ptr() as u64) - hhdm;

            // 64x64 2x Scaled Hardware Cursor Bitmap (BGRA8888)
            let mut cursor_pixels = alloc::vec![0x00000000u32; 64 * 64];
            let shape: [&[u8; 13]; 19] = [
                b"B............", b"BB...........", b"BWB..........", b"BWWBD........",
                b"BWWWBD.......", b"BWWWWBD......", b"BWWWWWBD.....", b"BWWWWWWBD....",
                b"BWWWWWWWBD...", b"BWWWWWWWWBD..", b"BWWWWBBBBBBD.", b"BWWWB........",
                b"BWWB.........", b"BWB.BB.......", b"BB..BWB......", b"B....BWB.....",
                b".....BWB.....", b"......BB.....", b".............",
            ];
            for (ry, row) in shape.iter().enumerate() {
                for (rx, &px) in row.iter().enumerate() {
                    let col = match px {
                        b'W' => 0xFF0F172A,
                        b'B' => 0xFFFFFFFF,
                        b'D' => 0x66000000,
                        _ => 0x00000000,
                    };
                    if col != 0 && ry < 64 && rx < 64 {
                        cursor_pixels[ry * 64 + rx] = col;
                    }
                }
            }
            let cursor_phys = (cursor_pixels.as_ptr() as u64) - hhdm;
            let want_hw_cursor = crate::config::CONFIG.is_hw_cursor();

            let mut driver = VirtioGpuDriver {
                notify_addr,
                desc: (desc_phys + hhdm) as *mut VirtqDesc,
                avail: (avail_phys + hhdm) as *mut VirtqAvail,
                used: (used_phys + hhdm) as *mut VirtqUsed,
                q_size,
                avail_idx: 0,
                last_used_idx: 0,
                cmd_phys,
                cmd_virt: (cmd_phys + hhdm) as *mut u8,
                cursor_notify_addr,
                cursor_desc: (c_desc_phys + hhdm) as *mut VirtqDesc,
                cursor_avail: (c_avail_phys + hhdm) as *mut VirtqAvail,
                cursor_used: (c_used_phys + hhdm) as *mut VirtqUsed,
                cursor_q_size,
                cursor_avail_idx: 0,
                cursor_last_used_idx: 0,
                cursor_cmd_phys: c_cmd_phys,
                cursor_cmd_virt: (c_cmd_phys + hhdm) as *mut u8,
                hw_cursor_active: want_hw_cursor,
                cursor_pixels,
                width,
                height,
                virgl_supported,
                virgl_3d_active: false,
                buffers: [buf0, buf1],
                front_idx: 0,
                prev_dirty: Vec::with_capacity(32),
            };

            let info_cmd = VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_GET_DISPLAY_INFO, ..Default::default() };
            let _ = driver.send_cmd(&info_cmd);

            for (idx, &res_id) in RESOURCE_IDS.iter().enumerate() {
                let create_cmd = VirtioGpuResourceCreate2d {
                    hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D, ..Default::default() },
                    resource_id: res_id,
                    format: VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM,
                    width: width as u32,
                    height: height as u32,
                };
                let _ = driver.send_cmd(&create_cmd);

                let attach_cmd = VirtioGpuAttachBacking {
                    hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING, ..Default::default() },
                    resource_id: res_id,
                    nr_entries: 1,
                    addr: if idx == 0 { phys0 } else { phys1 },
                    length: (width * height * 4) as u32,
                    padding: 0,
                };
                let _ = driver.send_cmd(&attach_cmd);
            }

            // Initialize Hardware Cursor Plane if enabled in KernelConfig
            if driver.hw_cursor_active {
                let c_create = VirtioGpuResourceCreate2d {
                    hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D, ..Default::default() },
                    resource_id: CURSOR_RESOURCE_ID,
                    format: VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM,
                    width: 64,
                    height: 64,
                };
                let _ = driver.send_cmd(&c_create);

                let c_attach = VirtioGpuAttachBacking {
                    hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING, ..Default::default() },
                    resource_id: CURSOR_RESOURCE_ID,
                    nr_entries: 1,
                    addr: cursor_phys,
                    length: (64 * 64 * 4) as u32,
                    padding: 0,
                };
                let _ = driver.send_cmd(&c_attach);

                let c_xfer = VirtioGpuTransferToHost2d {
                    hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D, ..Default::default() },
                    r: VirtioGpuRect { x: 0, y: 0, width: 64, height: 64 },
                    offset: 0,
                    resource_id: CURSOR_RESOURCE_ID,
                    padding: 0,
                };
                let _ = driver.send_cmd(&c_xfer);

                let c_update = VirtioGpuUpdateCursor {
                    hdr: VirtioGpuCtrlHdr { hdr_type: VIRTIO_GPU_CMD_UPDATE_CURSOR, ..Default::default() },
                    pos: VirtioGpuCursorPos { scanout_id: 0, x: (width / 2) as u32, y: (height / 2) as u32, padding: 0 },
                    resource_id: CURSOR_RESOURCE_ID,
                    hot_x: 0,
                    hot_y: 0,
                    padding: 0,
                };
                driver.send_cursor_cmd(&c_update);
            }

            // Initialize VirGL 3D Context if enabled in KernelConfig & supported by Host
            if virgl_supported && crate::config::CONFIG.is_hw_accel_3d() {
                let mut debug_name = [0u8; 64];
                let name_bytes = b"EOS_VirGL_3D";
                debug_name[..name_bytes.len()].copy_from_slice(name_bytes);
                let ctx_cmd = VirtioGpuCtxCreate {
                    hdr: VirtioGpuCtrlHdr {
                        hdr_type: VIRTIO_GPU_CMD_CTX_CREATE,
                        ctx_id: VIRGL_CTX_ID,
                        ..Default::default()
                    },
                    nlen: name_bytes.len() as u32,
                    context_init: 0,
                    debug_name,
                };
                if driver.send_cmd(&ctx_cmd) == 0x1100 {
                    for &res_id in &RESOURCE_IDS {
                        let att = VirtioGpuCtxResource {
                            hdr: VirtioGpuCtrlHdr {
                                hdr_type: VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE,
                                ctx_id: VIRGL_CTX_ID,
                                ..Default::default()
                            },
                            resource_id: res_id,
                            padding: 0,
                        };
                        let _ = driver.send_cmd(&att);
                    }
                    driver.virgl_3d_active = true;
                }
            }

            driver.commit_back_buffer(0, 0, 0, width, height);

            crate::log_info!(
                "VIRTIO-GPU",
                "Initialized {}x{} Atomic Scanout | VirGL 3D Ctx: {} | HW Cursor: {}",
                width,
                height,
                if driver.virgl_3d_active { "ACTIVE" } else { "OFF" },
                if driver.hw_cursor_active { "ACTIVE" } else { "OFF" }
            );
            *addr_of_mut!(VIRTIO_GPU) = Some(driver);
            return;
        }
    }
}
