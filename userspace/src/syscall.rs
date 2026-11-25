#![allow(dead_code)]
use core::arch::asm;

pub fn sys_present(pixels: *const u32, w: usize, h: usize) {
    unsafe {
        asm!("syscall", in("rax") 502, in("rdi") pixels as u64, in("rsi") 0, in("rdx") 0, in("r10") w as u64, in("r8") h as u64, out("rcx") _, out("r11") _);
    }
}

pub fn sys_poll_event() -> Option<[u64; 3]> {
    let type_val: u64;
    let data1: u64;
    let data2: u64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 503u64 => type_val,
            out("rdi") data1,
            out("rsi") data2,
            out("rcx") _, out("r11") _
        );
    }
    if type_val == 0 { None } else { Some([type_val, data1, data2]) }
}

pub fn sys_present_rects(pixels: *const u32, width: usize, height: usize, rects: &[[u32; 4]]) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 504, in("rdi") pixels as u64, in("rsi") rects.as_ptr() as u64, in("rdx") rects.len() as u64, in("r10") width as u64, in("r8") height as u64, out("rcx") _, out("r11") _);
    }
}

pub fn sys_net_tx(data: &[u8]) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 512, in("rdi") data.as_ptr() as u64, in("rsi") data.len() as u64, out("rcx") _, out("r11") _);
    }
}

pub fn sys_net_rx(buf: &mut [u8]) -> usize {
    let len: usize;
    unsafe {
        core::arch::asm!("syscall", inout("rax") 513u64 => len, in("rdi") buf.as_mut_ptr() as u64, in("rsi") buf.len() as u64, out("rcx") _, out("r11") _);
    }
    len
}

pub fn sys_net_mac(buf: &mut [u8; 6]) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 514, in("rdi") buf.as_mut_ptr() as u64, out("rcx") _, out("r11") _);
    }
}

pub fn sys_gpu_move_cursor(x: u32, y: u32) -> bool {
    let ret: u64;
    unsafe {
        asm!("syscall", inout("rax") 506u64 => ret, in("rdi") x as u64, in("rsi") y as u64, out("rcx") _, out("r11") _);
    }
    ret == 1
}

pub fn sys_gpu_submit_3d(cmds: &[u32]) -> bool {
    let ret: u64;
    unsafe {
        asm!("syscall", inout("rax") 507u64 => ret, in("rdi") cmds.as_ptr() as u64, in("rsi") cmds.len() as u64, out("rcx") _, out("r11") _);
    }
    ret == 1
}

pub fn sys_gpu_info(out: &mut [u32; 6]) -> bool {
    let ret: i64;
    unsafe {
        asm!("syscall", inout("rax") 508i64 => ret, in("rdi") out.as_mut_ptr() as u64, out("rcx") _, out("r11") _);
    }
    ret == 0
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct RawProcTelemetry {
    pub pid: u32,
    pub core_id: u32,
    pub state: u32,
    pub _pad: u32,
    pub mem_bytes: u64,
    pub cpu_cycles: u64,
    pub syscalls: u64,
    pub start_tsc: u64,
    pub name: [u8; 24],
}

pub fn sys_proc_list(out: &mut [RawProcTelemetry]) -> usize {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 509i64 => ret,
            in("rdi") out.as_mut_ptr() as u64,
            in("rsi") out.len() as u64,
            out("rcx") _, out("r11") _
        );
    }
    if ret > 0 { ret as usize } else { 0 }
}

pub fn sys_media_decode_image(src: &[u8], out_pixels: *mut u32, out_cap: usize, out_meta: &mut [u32; 2]) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 510i64 => ret,
            in("rdi") src.as_ptr() as u64,
            in("rsi") src.len() as u64,
            in("rdx") out_pixels as u64,
            in("r10") out_cap as u64,
            in("r8") out_meta.as_mut_ptr() as u64,
            out("rcx") _, out("r11") _
        );
    }
    ret
}

/// Direct single-syscall kernel sleep (avoids musl's 6-syscall clock_nanosleep overhead)
pub fn sys_sleep_ms(ms: u64) {
    let ts: [u64; 2] = [ms / 1000, (ms % 1000) * 1_000_000];
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 35i64 => ret,
            in("rdi") ts.as_ptr() as u64,
            in("rsi") 0u64,
            out("rcx") _, out("r11") _
        );
    }
    let _ = ret;
}

pub fn sys_spawn_elf(path: &str, args: &str) -> i64 {
    let mut pb = [0u8; 128]; let mut ab = [0u8; 128];
    let p = path.as_bytes(); let a = args.as_bytes();
    pb[..p.len().min(127)].copy_from_slice(&p[..p.len().min(127)]);
    ab[..a.len().min(127)].copy_from_slice(&a[..a.len().min(127)]);
    let ret: i64;
    unsafe { asm!("syscall", inout("rax") 500i64 => ret, in("rdi") pb.as_ptr() as u64, in("rsi") ab.as_ptr() as u64, out("rcx") _, out("r11") _); }
    ret
}

pub fn sys_wait_elf_output(core_id: usize) -> String {
    let mut buf = vec![0u8; 32768];
    let ret: i64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 501i64 => ret,
            in("rdi") core_id as u64,
            in("rsi") buf.as_mut_ptr() as u64,
            in("rdx") buf.len() as u64,
            out("rcx") _, out("r11") _
        );
    }
    let n = (ret.max(0) as usize).min(buf.len());
    String::from_utf8_lossy(&buf[..n]).to_string()
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct RawExtWindowDesc {
    pub id: u32,
    pub core_id: u32,
    pub width: u32,
    pub height: u32,
    pub generation: u32,
    pub file_req: u32,
    pub title: [u8; 40],
}

impl Default for RawExtWindowDesc {
    fn default() -> Self {
        Self {
            id: 0,
            core_id: 0,
            width: 0,
            height: 0,
            generation: 0,
            file_req: 0,
            title: [0; 40],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct RawExtWindowEvent { pub kind: u32, pub a: i32, pub b: i32, pub c: i32 }

pub fn sys_win_create(title: &str, w: u32, h: u32) -> u32 {
    let tb = title.as_bytes(); let ret: u64;
    unsafe { asm!("syscall", inout("rax") 530u64 => ret, in("rdi") tb.as_ptr() as u64, in("rsi") tb.len() as u64, in("rdx") w as u64, in("r10") h as u64, out("rcx") _, out("r11") _); }
    ret as u32
}
pub fn sys_win_commit(id: u32, px: &[u32], w: u32, h: u32) {
    unsafe { asm!("syscall", in("rax") 531u64, in("rdi") id as u64, in("rsi") px.as_ptr() as u64, in("rdx") w as u64, in("r10") h as u64, out("rcx") _, out("r11") _); }
}
pub fn sys_win_poll_event(id: u32) -> Option<RawExtWindowEvent> {
    let mut ev = RawExtWindowEvent::default(); let ret: u64;
    unsafe { asm!("syscall", inout("rax") 532u64 => ret, in("rdi") id as u64, in("rsi") &mut ev as *mut _ as u64, out("rcx") _, out("r11") _); }
    if ret == 1 { Some(ev) } else { None }
}
pub fn sys_win_wait_event(id: u32, timeout_ms: u64) -> Option<RawExtWindowEvent> {
    let mut ev = RawExtWindowEvent::default(); let ret: u64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 538u64 => ret,
            in("rdi") id as u64,
            in("rsi") &mut ev as *mut _ as u64,
            in("rdx") timeout_ms,
            out("rcx") _, out("r11") _
        );
    }
    if ret == 1 { Some(ev) } else { None }
}
pub fn sys_win_destroy(id: u32) {
    unsafe { asm!("syscall", in("rax") 533u64, in("rdi") id as u64, out("rcx") _, out("r11") _); }
}
pub fn sys_comp_list_windows(out: &mut [RawExtWindowDesc]) -> usize {
    let ret: u64;
    unsafe { asm!("syscall", inout("rax") 534u64 => ret, in("rdi") out.as_mut_ptr() as u64, in("rsi") out.len() as u64, out("rcx") _, out("r11") _); }
    ret as usize
}
pub fn sys_comp_read_surface(id: u32, dst: &mut [u32]) -> usize {
    let ret: u64;
    unsafe { asm!("syscall", inout("rax") 535u64 => ret, in("rdi") id as u64, in("rsi") dst.as_mut_ptr() as u64, in("rdx") dst.len() as u64, out("rcx") _, out("r11") _); }
    ret as usize
}
pub fn sys_comp_send_event(id: u32, kind: u32, a: i32, b: i32, c: i32) {
    unsafe { asm!("syscall", in("rax") 536u64, in("rdi") id as u64, in("rsi") kind as u64, in("rdx") a as u64, in("r10") b as u64, in("r8") c as u64, out("rcx") _, out("r11") _); }
}
pub fn sys_win_file_portal(cmd: u32, id: u32, buf: &mut [u8]) -> usize {
    let ret: u64;
    unsafe { asm!("syscall", inout("rax") 537u64 => ret, in("rdi") cmd as u64, in("rsi") id as u64, in("rdx") buf.as_mut_ptr() as u64, in("r10") buf.len() as u64, out("rcx") _, out("r11") _); }
    ret as usize
}

pub fn sys_audio_write(pcm: &[u8]) -> usize {
    let ret: u64;
    unsafe { asm!("syscall", inout("rax") 540u64 => ret, in("rdi") pcm.as_ptr() as u64, in("rsi") pcm.len() as u64, out("rcx") _, out("r11") _); }
    ret as usize
}

pub fn sys_audio_set_volume(vol_pct: u8) {
    unsafe { asm!("syscall", in("rax") 541u64, in("rdi") vol_pct as u64, out("rcx") _, out("r11") _); }
}

pub fn sys_audio_is_active() -> bool {
    let ret: u64;
    unsafe { asm!("syscall", inout("rax") 542u64 => ret, out("rcx") _, out("r11") _); }
    ret == 1
}

pub fn sys_tty_state() -> u8 {
    let ret: u64;
    unsafe { asm!("syscall", inout("rax") 543u64 => ret, out("rcx") _, out("r11") _); }
    ret as u8
}

pub fn sys_clipboard_set(text: &str) -> usize {
    let bytes = text.as_bytes();
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 545u64 => ret,
            in("rdi") bytes.as_ptr() as u64,
            in("rsi") bytes.len() as u64,
            out("rcx") _, out("r11") _
        );
    }
    ret as usize
}

pub fn sys_clipboard_get() -> String {
    let mut buf = vec![0u8; 8192];
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 546u64 => ret,
            in("rdi") buf.as_mut_ptr() as u64,
            in("rsi") buf.len() as u64,
            out("rcx") _, out("r11") _
        );
    }
    let n = (ret as usize).min(buf.len());
    String::from_utf8_lossy(&buf[..n]).to_string()
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct RawCrashReport {
    pub valid: u32,
    pub fault_kind: u32,
    pub core_id: u32,
    pub error_code: u32,
    pub rip: u64,
    pub cr2: u64,
    pub app_name: [u8; 32],
    pub spawn_args: [u8; 96],
}

impl Default for RawCrashReport {
    fn default() -> Self {
        Self {
            valid: 0,
            fault_kind: 0,
            core_id: 0,
            error_code: 0,
            rip: 0,
            cr2: 0,
            app_name: [0; 32],
            spawn_args: [0; 96],
        }
    }
}

pub fn sys_poll_crash() -> Option<RawCrashReport> {
    let mut rep = RawCrashReport::default();
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 547u64 => ret,
            in("rdi") &mut rep as *mut _ as u64,
            out("rcx") _, out("r11") _
        );
    }
    if ret == 1 && rep.valid != 0 { Some(rep) } else { None }
}

pub fn sys_kill_proc(pid: u32) -> bool {
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 548u64 => ret,
            in("rdi") pid as u64,
            out("rcx") _, out("r11") _
        );
    }
    ret == 1
}

pub fn sys_power(cmd: u64) {
    unsafe {
        asm!(
            "syscall",
            in("rax") 550u64,
            in("rdi") cmd,
            out("rcx") _, out("r11") _
        );
    }
}

pub fn sys_wait_event(timeout_ms: u64) -> Option<[u64; 3]> {
    let type_val: u64;
    let data1: u64;
    let data2: u64;
    unsafe {
        asm!(
            "syscall",
            inout("rax") 505u64 => type_val,
            inout("rdi") timeout_ms => data1,
            out("rsi") data2,
            out("rcx") _, out("r11") _
        );
    }
    if type_val == 0 { None } else { Some([type_val, data1, data2]) }
}
