#![allow(dead_code)]

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use crate::splash_data::{SPLASH_BITMAP, SPLASH_BITMAP_W, SPLASH_BITMAP_H};

pub const FONT_WIDTH: usize = 8;
pub const FONT_HEIGHT: usize = 16;

pub struct FramebufferWriter {
    pub buffer: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub scale: usize,
}

pub static mut WRITER: Option<FramebufferWriter> = None;

impl FramebufferWriter {
    pub fn new(buffer: *mut u8, width: usize, height: usize, pitch: usize) -> Self {
        Self {
            buffer,
            width,
            height,
            pitch,
            cursor_x: 24,
            cursor_y: 24,
            scale: 2,
        }
    }

    #[inline(always)]
    pub fn put_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height { return; }
        unsafe {
            let offset = (y * self.pitch) + (x * 4);
            let pixel_ptr = self.buffer.add(offset) as *mut u32;
            *pixel_ptr = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }
    }

    pub fn draw_filled_circle(&mut self, cx: isize, cy: isize, radius: isize, r: u8, g: u8, b: u8) {
        let r_sq = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= r_sq {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && px < self.width as isize && py >= 0 && py < self.height as isize {
                        self.put_pixel(px as usize, py as usize, r, g, b);
                    }
                }
            }
        }
    }

    pub fn render_boot_splash(&mut self, step: usize) {
        unsafe {
            let slice = core::slice::from_raw_parts_mut(self.buffer as *mut u32, (self.pitch / 4) * self.height);
            slice.fill(0xFF000000);
        }

        let bmp_w = SPLASH_BITMAP_W;
        let bmp_h = SPLASH_BITMAP_H;

        let start_x = ((self.width.saturating_sub(bmp_w)) / 2) as isize;
        let start_y = (((self.height.saturating_sub(bmp_h)) / 2).saturating_sub(40)) as isize;

        let bytes_per_row = (bmp_w + 7) / 8;
        for y in 0..bmp_h {
            let py = start_y + y as isize;
            for bx in 0..bytes_per_row {
                let byte_idx = y * bytes_per_row + bx;
                if byte_idx >= SPLASH_BITMAP.len() { break; }
                let byte_val = SPLASH_BITMAP[byte_idx];
                if byte_val == 0 { continue; }
                for bit in 0..8 {
                    if (byte_val & (1 << (7 - bit))) != 0 {
                        let px = start_x + (bx * 8 + bit) as isize;
                        if px >= 0 && px < self.width as isize && py >= 0 && py < self.height as isize {
                            self.put_pixel(px as usize, py as usize, 255, 255, 255);
                        }
                    }
                }
            }
        }

        let dot_y = start_y + bmp_h as isize + 36;
        let mid_x = (self.width / 2) as isize;
        let spacing = 28;
        for i in 0..3 {
            let dx = mid_x + ((i as isize - 1) * spacing);
            if i == (step % 3) {
                self.draw_filled_circle(dx, dot_y, 6, 255, 255, 255);
            } else {
                self.draw_filled_circle(dx, dot_y, 4, 80, 80, 95);
            }
        }
    }
}

// ============================================================================
// Milestone 07: Virtual Terminal Switcher (tty1 GUI <-> tty2 Root Shell)
// IRQ-safe lock-free input queue + Framebuffer/VirtIO-GPU console renderer
// ============================================================================

static ACTIVE_TTY: AtomicU8 = AtomicU8::new(1);
static PENDING_TTY_SWITCH: AtomicU8 = AtomicU8::new(0);

const TTY_KEY_CAP: usize = 128;
static mut TTY_KEY_BUF: [u8; TTY_KEY_CAP] = [0; TTY_KEY_CAP];
static TTY_KEY_HEAD: AtomicUsize = AtomicUsize::new(0);
static TTY_KEY_TAIL: AtomicUsize = AtomicUsize::new(0);

#[inline(always)]
pub fn active_tty() -> u8 {
    ACTIVE_TTY.load(Ordering::Acquire)
}

/// Safe to invoke directly from Keyboard IRQ1 (zero locks, zero allocations).
pub fn request_tty_switch(tty: u8) {
    if tty == 1 || tty == 2 {
        ACTIVE_TTY.store(tty, Ordering::Release);
        PENDING_TTY_SWITCH.store(tty, Ordering::Release);
    }
}

/// Safe to invoke directly from Keyboard IRQ1.
pub fn tty_push_byte(b: u8) {
    let head = TTY_KEY_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % TTY_KEY_CAP;
    if next != TTY_KEY_TAIL.load(Ordering::Acquire) {
        unsafe { TTY_KEY_BUF[head] = b; }
        TTY_KEY_HEAD.store(next, Ordering::Release);
    }
}

fn tty_pop_byte() -> Option<u8> {
    let tail = TTY_KEY_TAIL.load(Ordering::Relaxed);
    if tail == TTY_KEY_HEAD.load(Ordering::Acquire) {
        return None;
    }
    let b = unsafe { TTY_KEY_BUF[tail] };
    TTY_KEY_TAIL.store((tail + 1) % TTY_KEY_CAP, Ordering::Release);
    Some(b)
}

struct TtyConsole {
    lines: Vec<(String, u32)>,
    input: String,
    cwd: String,
    cmd_history: Vec<String>,
    fb_cache: Vec<u32>,
    fb_w: usize,
    fb_h: usize,
    dirty: bool,
}

static mut TTY_CONSOLE: Option<TtyConsole> = None;

fn tty_load_shared_history(con: &mut TtyConsole) {
    con.cmd_history.clear();
    if let Ok(bytes) = crate::fs::vfs_read_bytes("home/march/.bash_history") {
        if let Ok(txt) = core::str::from_utf8(&bytes) {
            for l in txt.lines() {
                let t = l.trim();
                if !t.is_empty() {
                    con.cmd_history.push(String::from(t));
                }
            }
        }
    }
}

fn tty_append_shared_history(con: &mut TtyConsole, cmd: &str) {
    let t = cmd.trim();
    if t.is_empty() { return; }
    tty_load_shared_history(con);
    con.cmd_history.retain(|x| x != t);
    con.cmd_history.push(String::from(t));
    let mut payload = String::new();
    for h in &con.cmd_history {
        payload.push_str(h);
        payload.push('\n');
    }
    let _ = crate::fs::vfs_save_text_file("home/march/.bash_history", payload.as_bytes());
}

fn get_tty_console() -> &'static mut TtyConsole {
    unsafe {
        if (*addr_of_mut!(TTY_CONSOLE)).is_none() {
            let mut lines = Vec::new();
            lines.push((String::from("EOS Kernel Virtual Console (tty2) — Unified POSIX Root Shell"), 0xFF38BDF8));
            lines.push((String::from("Ctrl+Alt+F1: Desktop (tty1) | Shared History with Terminal & SSH | Type 'help'"), 0xFF94A3B8));
            lines.push((String::new(), 0xFFFFFFFF));
            let mut con = TtyConsole {
                lines,
                input: String::new(),
                cwd: String::from("/"),
                cmd_history: Vec::new(),
                fb_cache: Vec::new(),
                fb_w: 0,
                fb_h: 0,
                dirty: true,
            };
            tty_load_shared_history(&mut con);
            *addr_of_mut!(TTY_CONSOLE) = Some(con);
        }
        (*addr_of_mut!(TTY_CONSOLE)).as_mut().unwrap()
    }
}

/// Polled from Core 0 main loop (outside IRQ) to handle tty2 switching & root commands safely.
pub fn tty_poll() {
    let req = PENDING_TTY_SWITCH.swap(0, Ordering::AcqRel);
    if req == 2 {
        crate::log_info!("TTY", "Switched to tty2 (Full-Screen Root Console)");
        let con = get_tty_console();
        render_tty_console(con);
        con.dirty = false;
        return;
    } else if req == 1 {
        crate::log_info!("TTY", "Switched back to tty1 (Graphical Desktop Compositor)");
        return;
    }

    if active_tty() != 2 {
        while tty_pop_byte().is_some() {}
        return;
    }

    let con = get_tty_console();
    while let Some(b) = tty_pop_byte() {
        match b {
            b'\n' | b'\r' => {
                let cmd = con.input.clone();
                let prompt_line = format!("root@eos:{}# {}", con.cwd, cmd);
                con.lines.push((prompt_line, 0xFF38BDF8));
                con.input.clear();
                execute_tty_command(con, cmd.trim());
                con.dirty = true;
            }
            0x08 => {
                con.input.pop();
                con.dirty = true;
            }
            0x20..=0x7E => {
                con.input.push(b as char);
                con.dirty = true;
            }
            _ => {}
        }
    }

    if con.dirty {
        render_tty_console(con);
        con.dirty = false;
    }
}

fn tty_resolve_path(cwd: &str, arg: &str) -> String {
    let a = arg.trim();
    if a.is_empty() || a == "." {
        String::from(cwd)
    } else if a.starts_with('/') {
        String::from(a)
    } else if cwd == "/" {
        format!("/{}", a)
    } else {
        format!("{}/{}", cwd.trim_end_matches('/'), a)
    }
}

fn execute_tty_command(con: &mut TtyConsole, cmd: &str) {
    let trimmed = cmd.trim();
    if trimmed.is_empty() { return; }
    tty_append_shared_history(con, trimmed);

    // Support POSIX pipelines (`cmd1 | cmd2`) directly inside tty2
    let stages: Vec<&str> = trimmed.split('|').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    let mut pipe_buf: Option<Vec<String>> = None;

    for (s_idx, stage) in stages.iter().enumerate() {
        let tokens: Vec<&str> = stage.split_whitespace().collect();
        if tokens.is_empty() { continue; }
        let prog = tokens[0];
        let args = &tokens[1..];
        let mut out: Vec<(String, u32)> = Vec::new();

        match prog {
            "help" => {
                out.push((String::from("POSIX: ls cd pwd cat head tail wc grep rm echo history uname whoami"), 0xFFE2E8F0));
                out.push((String::from("SYS  : ps kill <pid> free mem audit reboot shutdown gui clear | supports '|'"), 0xFF34D399));
            }
            "clear" => { con.lines.clear(); return; }
            "gui" | "exit" => { request_tty_switch(1); return; }
            "whoami" => out.push((String::from("root"), 0xFF34D399)),
            "uname" => out.push((String::from("EOS 0.1.0 x86_64 SMP (POSIX tty2)"), 0xFFE2E8F0)),
            "pwd" => out.push((con.cwd.clone(), 0xFFE2E8F0)),
            "echo" => out.push((args.join(" "), 0xFFE2E8F0)),
            "history" => {
                tty_load_shared_history(con);
                for (i, h) in con.cmd_history.iter().enumerate() {
                    out.push((format!("{:3}  {}", i + 1, h), 0xFFE2E8F0));
                }
            }
            "cd" => {
                let target = args.join(" ");
                if target.is_empty() || target == "/" || target == "~" {
                    con.cwd = String::from("/");
                } else if target == ".." {
                    if let Some(pos) = con.cwd.trim_end_matches('/').rfind('/') {
                        let p = &con.cwd[..pos];
                        con.cwd = if p.is_empty() { String::from("/") } else { String::from(p) };
                    } else { con.cwd = String::from("/"); }
                } else {
                    let next = tty_resolve_path(&con.cwd, &target);
                    if crate::fs::vfs_list_dir(&next).is_ok() {
                        con.cwd = next;
                    } else {
                        out.push((format!("cd: no such directory: {}", target), 0xFFEF4444));
                    }
                }
            }
            "mem" | "free" => {
                let used_kb = crate::mm::heap::HEAP_ALLOCATOR.used() / 1024;
                let total_kb = crate::mm::heap::HEAP_SIZE / 1024;
                out.push((format!("KernelHeap: {} KB total | {} KB used | {} KB free", total_kb, used_kb, total_kb.saturating_sub(used_kb)), 0xFFE2E8F0));
            }
            "ps" => {
                let mut snap = [crate::task::ProcTelemetry::empty(); crate::task::MAX_PROCS];
                let count = crate::task::snapshot_procs(&mut snap);
                out.push((String::from("PID   CORE   MEM(KB)   SYSCALLS   NAME"), 0xFFFCD34D));
                for p in &snap[..count] {
                    let nlen = p.name.iter().position(|&b| b == 0).unwrap_or(24);
                    let name = core::str::from_utf8(&p.name[..nlen]).unwrap_or("?");
                    out.push((format!("{:<5} {:<6} {:<9} {:<10} {}", p.pid, p.core_id, p.mem_bytes / 1024, p.syscalls, name), 0xFFE2E8F0));
                }
            }
            "kill" => {
                if let Some(pid_s) = args.first() {
                    if let Ok(pid) = pid_s.parse::<u32>() {
                        if crate::task::terminate_proc_by_pid(pid) {
                            out.push((format!("Terminated PID {}", pid), 0xFF34D399));
                        } else {
                            out.push((format!("kill: failed or protected PID {}", pid), 0xFFEF4444));
                        }
                    } else { out.push((String::from("kill: invalid PID"), 0xFFEF4444)); }
                } else { out.push((String::from("usage: kill <pid>"), 0xFFEF4444)); }
            }
            "ls" => {
                let target = if args.is_empty() { con.cwd.clone() } else { tty_resolve_path(&con.cwd, &args.join(" ")) };
                match crate::fs::vfs_list_dir(&target) {
                    Ok(items) => {
                        for it in items {
                            match it {
                                crate::fs::FsItem::Directory(n, _) => out.push((format!("{}/", n), 0xFF38BDF8)),
                                crate::fs::FsItem::File(n, sz, _) => out.push((format!("{} ({} B)", n, sz), 0xFFE2E8F0)),
                            }
                        }
                    }
                    Err(e) => out.push((format!("ls: {}: {}", target, e), 0xFFEF4444)),
                }
            }
            "cat" => {
                if let Some(ref stdin) = pipe_buf {
                    for l in stdin { out.push((l.clone(), 0xFFE2E8F0)); }
                } else if args.is_empty() {
                    out.push((String::from("usage: cat <path>"), 0xFFEF4444));
                } else {
                    let path = tty_resolve_path(&con.cwd, &args.join(" "));
                    match crate::fs::vfs_read_bytes(&path) {
                        Ok(bytes) => {
                            let s = core::str::from_utf8(&bytes).unwrap_or("<binary>");
                            for l in s.lines() { out.push((String::from(l), 0xFFE2E8F0)); }
                        }
                        Err(e) => out.push((format!("cat: {}: {}", path, e), 0xFFEF4444)),
                    }
                }
            }
            "head" => {
                let n = 10usize;
                let src = if let Some(ref stdin) = pipe_buf {
                    stdin.clone()
                } else if !args.is_empty() {
                    let path = tty_resolve_path(&con.cwd, &args.join(" "));
                    crate::fs::vfs_read_bytes(&path).ok()
                        .and_then(|b| core::str::from_utf8(&b).ok().map(|s| s.lines().map(String::from).collect()))
                        .unwrap_or_default()
                } else { Vec::new() };
                for l in src.into_iter().take(n) { out.push((l, 0xFFE2E8F0)); }
            }
            "tail" => {
                let n = 10usize;
                let src = if let Some(ref stdin) = pipe_buf {
                    stdin.clone()
                } else if !args.is_empty() {
                    let path = tty_resolve_path(&con.cwd, &args.join(" "));
                    crate::fs::vfs_read_bytes(&path).ok()
                        .and_then(|b| core::str::from_utf8(&b).ok().map(|s| s.lines().map(String::from).collect()))
                        .unwrap_or_default()
                } else { Vec::new() };
                let skip = src.len().saturating_sub(n);
                for l in src.into_iter().skip(skip) { out.push((l, 0xFFE2E8F0)); }
            }
            "grep" => {
                let pat = args.first().copied().unwrap_or("");
                let src = if let Some(ref stdin) = pipe_buf {
                    stdin.clone()
                } else if args.len() >= 2 {
                    let path = tty_resolve_path(&con.cwd, &args[1..].join(" "));
                    crate::fs::vfs_read_bytes(&path).ok()
                        .and_then(|b| core::str::from_utf8(&b).ok().map(|s| s.lines().map(String::from).collect()))
                        .unwrap_or_default()
                } else { Vec::new() };
                for l in src {
                    if !pat.is_empty() && l.contains(pat) { out.push((l, 0xFF34D399)); }
                }
            }
            "wc" => {
                let src = if let Some(ref stdin) = pipe_buf {
                    stdin.clone()
                } else if !args.is_empty() {
                    let path = tty_resolve_path(&con.cwd, &args.join(" "));
                    crate::fs::vfs_read_bytes(&path).ok()
                        .and_then(|b| core::str::from_utf8(&b).ok().map(|s| s.lines().map(String::from).collect()))
                        .unwrap_or_default()
                } else { Vec::new() };
                let lines_cnt = src.len();
                let words_cnt: usize = src.iter().map(|l| l.split_whitespace().count()).sum();
                let bytes_cnt: usize = src.iter().map(|l| l.len() + 1).sum();
                out.push((format!("{} {} {}", lines_cnt, words_cnt, bytes_cnt), 0xFFE2E8F0));
            }
            "rm" => {
                if !args.is_empty() {
                    let path = tty_resolve_path(&con.cwd, &args.join(" "));
                    match crate::fs::vfs_delete_file(&path) {
                        Ok(_) => out.push((format!("Removed '{}'", path), 0xFF34D399)),
                        Err(e) => out.push((format!("rm: {}: {}", path, e), 0xFFEF4444)),
                    }
                } else { out.push((String::from("rm: missing operand"), 0xFFEF4444)); }
            }
            "audit" => {
                let p = crate::audit::run_kernel_self_audit();
                out.push((format!("Kernel self-audit completed: {}/6 core checks passed", p), 0xFF34D399));
            }
            "reboot" => crate::arch::x86_64::power::reboot(),
            "shutdown" => crate::arch::x86_64::power::shutdown(),
            _ => out.push((format!("tty2: command not found: '{}'. Type 'help'.", prog), 0xFFEF4444)),
        }

        if s_idx + 1 < stages.len() {
            pipe_buf = Some(out.into_iter().map(|(s, _)| s).collect());
        } else {
            for item in out.into_iter().take(40) {
                con.lines.push(item);
            }
        }
    }

    while con.lines.len() > 60 {
        con.lines.remove(0);
    }
}

fn render_tty_console(con: &mut TtyConsole) {
    let gpu_info = crate::drivers::virtio_gpu::get_gpu_info();
    let w = (gpu_info[0] as usize).clamp(640, 2560);
    let h = (gpu_info[1] as usize).clamp(480, 1440);
    if con.fb_w != w || con.fb_h != h || con.fb_cache.len() != w * h {
        con.fb_w = w;
        con.fb_h = h;
        con.fb_cache = alloc::vec![0xFF0A0F18u32; w * h];
    } else {
        con.fb_cache.fill(0xFF0A0F18u32);
    }

    // Header bar
    for y in 0..44.min(h) {
        let row = y * w;
        con.fb_cache[row..row + w].fill(0xFF132238u32);
    }
    draw_tty_str(&mut con.fb_cache, w, h, 20, 14, "EOS TTY2  |  ROOT SHELL (uid=0)  |  Ctrl+Alt+F1 / Ctrl+Shift+F1 / 'gui': Desktop", 0xFF38BDF8, 2);

    let scale = 2usize;
    let line_h = 8 * scale + 8;
    let max_rows = (h.saturating_sub(96)) / line_h;
    let start_idx = con.lines.len().saturating_sub(max_rows.saturating_sub(1));

    let mut cy = 60usize;
    for (text, color) in &con.lines[start_idx..] {
        draw_tty_str(&mut con.fb_cache, w, h, 20, cy, text, *color, scale);
        cy += line_h;
    }

    let prompt = format!("root@eos:{}# {}_", con.cwd, con.input);
    draw_tty_str(&mut con.fb_cache, w, h, 20, cy, &prompt, 0xFFF8FAFC, scale);

    if crate::drivers::virtio_gpu::is_active() {
        crate::drivers::virtio_gpu::present_full(con.fb_cache.as_ptr(), w, h);
    } else {
        unsafe {
            if let Some(writer) = (*addr_of_mut!(WRITER)).as_mut() {
                let copy_w = w.min(writer.width);
                let copy_h = h.min(writer.height);
                for y in 0..copy_h {
                    core::ptr::copy_nonoverlapping(
                        con.fb_cache.as_ptr().add(y * w),
                        (writer.buffer as *mut u32).add(y * (writer.pitch / 4)),
                        copy_w,
                    );
                }
            }
        }
    }
}

fn draw_tty_str(buf: &mut [u32], w: usize, h: usize, x: usize, y: usize, s: &str, col: u32, scale: usize) {
    let mut cx = x;
    for ch in s.chars() {
        if cx + 8 * scale >= w { break; }
        let g = tty_glyph_8x8(ch);
        for (row, &bits) in g.iter().enumerate() {
            if bits == 0 { continue; }
            for cbit in 0..8 {
                if (bits & (0x80 >> cbit)) != 0 {
                    for sy in 0..scale {
                        let py = y + row * scale + sy;
                        if py >= h { continue; }
                        let off = py * w;
                        for sx in 0..scale {
                            let px = cx + cbit * scale + sx;
                            if px < w { buf[off + px] = col; }
                        }
                    }
                }
            }
        }
        cx += 8 * scale;
    }
}

fn tty_glyph_8x8(c: char) -> [u8; 8] {
    match c {
        ' ' => [0; 8],
        '!' => [0x18,0x18,0x18,0x18,0x18,0x00,0x18,0x00],
        '#' => [0x24,0x7E,0x24,0x24,0x7E,0x24,0x00,0x00],
        '$' => [0x18,0x3E,0x60,0x3C,0x06,0x7C,0x18,0x00],
        '(' => [0x0C,0x18,0x30,0x30,0x30,0x18,0x0C,0x00],
        ')' => [0x30,0x18,0x0C,0x0C,0x0C,0x18,0x30,0x00],
        '+' => [0x00,0x18,0x18,0x7E,0x18,0x18,0x00,0x00],
        ',' => [0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x30],
        '-' => [0x00,0x00,0x00,0x7E,0x00,0x00,0x00,0x00],
        '.' => [0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x00],
        '/' => [0x02,0x04,0x08,0x10,0x20,0x40,0x00,0x00],
        '0' => [0x3C,0x66,0x6E,0x76,0x66,0x66,0x3C,0x00],
        '1' => [0x18,0x38,0x18,0x18,0x18,0x18,0x7E,0x00],
        '2' => [0x3C,0x66,0x06,0x1C,0x30,0x60,0x7E,0x00],
        '3' => [0x3C,0x66,0x06,0x1C,0x06,0x66,0x3C,0x00],
        '4' => [0x0C,0x1C,0x3C,0x6C,0x7E,0x0C,0x0C,0x00],
        '5' => [0x7E,0x60,0x7C,0x06,0x06,0x66,0x3C,0x00],
        '6' => [0x3C,0x60,0x7C,0x66,0x66,0x66,0x3C,0x00],
        '7' => [0x7E,0x06,0x0C,0x18,0x30,0x30,0x30,0x00],
        '8' => [0x3C,0x66,0x66,0x3C,0x66,0x66,0x3C,0x00],
        '9' => [0x3C,0x66,0x66,0x3E,0x06,0x0C,0x38,0x00],
        ':' => [0x00,0x18,0x18,0x00,0x18,0x18,0x00,0x00],
        '=' => [0x00,0x00,0x7E,0x00,0x7E,0x00,0x00,0x00],
        '<' => [0x0C,0x18,0x30,0x60,0x30,0x18,0x0C,0x00],
        '>' => [0x30,0x18,0x0C,0x06,0x0C,0x18,0x30,0x00],
        '?' => [0x3C,0x66,0x06,0x1C,0x18,0x00,0x18,0x00],
        '@' => [0x3C,0x66,0x6E,0x6A,0x6E,0x60,0x3C,0x00],
        '[' => [0x3C,0x30,0x30,0x30,0x30,0x30,0x3C,0x00],
        ']' => [0x3C,0x0C,0x0C,0x0C,0x0C,0x0C,0x3C,0x00],
        '_' => [0x00,0x00,0x00,0x00,0x00,0x00,0x7E,0x00],
        '|' => [0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x00],
        'A' | 'a' => [0x18,0x3C,0x66,0x66,0x7E,0x66,0x66,0x00],
        'B' | 'b' => [0x7C,0x66,0x66,0x7C,0x66,0x66,0x7C,0x00],
        'C' | 'c' => [0x3C,0x66,0x60,0x60,0x60,0x66,0x3C,0x00],
        'D' | 'd' => [0x78,0x6C,0x66,0x66,0x66,0x6C,0x78,0x00],
        'E' | 'e' => [0x7E,0x60,0x60,0x78,0x60,0x60,0x7E,0x00],
        'F' | 'f' => [0x7E,0x60,0x60,0x78,0x60,0x60,0x60,0x00],
        'G' | 'g' => [0x3C,0x66,0x60,0x6E,0x66,0x66,0x3E,0x00],
        'H' | 'h' => [0x66,0x66,0x66,0x7E,0x66,0x66,0x66,0x00],
        'I' | 'i' => [0x3C,0x18,0x18,0x18,0x18,0x18,0x3C,0x00],
        'J' | 'j' => [0x1E,0x0C,0x0C,0x0C,0x6C,0x6C,0x38,0x00],
        'K' | 'k' => [0x66,0x6C,0x78,0x70,0x78,0x6C,0x66,0x00],
        'L' | 'l' => [0x60,0x60,0x60,0x60,0x60,0x60,0x7E,0x00],
        'M' | 'm' => [0x63,0x77,0x7F,0x6B,0x63,0x63,0x63,0x00],
        'N' | 'n' => [0x66,0x76,0x7E,0x7E,0x6E,0x66,0x66,0x00],
        'O' | 'o' => [0x3C,0x66,0x66,0x66,0x66,0x66,0x3C,0x00],
        'P' | 'p' => [0x7C,0x66,0x66,0x7C,0x60,0x60,0x60,0x00],
        'Q' | 'q' => [0x3C,0x66,0x66,0x66,0x6A,0x6C,0x36,0x00],
        'R' | 'r' => [0x7C,0x66,0x66,0x7C,0x6C,0x66,0x66,0x00],
        'S' | 's' => [0x3C,0x66,0x60,0x3C,0x06,0x66,0x3C,0x00],
        'T' | 't' => [0x7E,0x18,0x18,0x18,0x18,0x18,0x18,0x00],
        'U' | 'u' => [0x66,0x66,0x66,0x66,0x66,0x66,0x3C,0x00],
        'V' | 'v' => [0x66,0x66,0x66,0x66,0x66,0x3C,0x18,0x00],
        'W' | 'w' => [0x63,0x63,0x63,0x6B,0x7F,0x77,0x63,0x00],
        'X' | 'x' => [0x66,0x66,0x3C,0x18,0x3C,0x66,0x66,0x00],
        'Y' | 'y' => [0x66,0x66,0x66,0x3C,0x18,0x18,0x18,0x00],
        'Z' | 'z' => [0x7E,0x06,0x0C,0x18,0x30,0x60,0x7E,0x00],
        _ => [0x00,0x18,0x24,0x24,0x18,0x00,0x00,0x00],
    }
}
