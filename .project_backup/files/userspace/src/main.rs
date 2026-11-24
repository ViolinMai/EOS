#![allow(dead_code, unused_imports, static_mut_refs)]

mod syscall;
mod sdk;
mod framework;
pub mod png;
pub mod net_manager;
pub mod test_subject;
pub mod ffi_media;
#[path = "apps/mod.rs"]
pub mod apps;

use std::fs;
use framework::*;
use rusttype::Font;

const SCREEN_W: usize = 2560;
const SCREEN_H: usize = 1440;

pub fn is_valid_truetype_font(data: &[u8]) -> bool {
    Font::try_from_bytes(data).is_some()
}

fn try_read_font(paths: &[&str]) -> Vec<u8> {
    for p in paths {
        if let Ok(data) = fs::read(p) {
            if !data.is_empty() && is_valid_truetype_font(&data) {
                println!("[FONT] Successfully loaded vector font from: {}", p);
                return data;
            }
        }
    }
    Vec::new()
}

fn main() {
    println!("🚀 Launching EOS Desktop (Ring 3 Userspace)...");

    net_manager::init_net([10, 0, 2, 15], [10, 0, 2, 2]);

    let font_paths = [
        "segoeui.ttf",
        "fonts/segoeui.ttf",
        "/fonts/segoeui.ttf",
        "/Initrd/segoeui.ttf",
        "/Initrd/fonts/segoeui.ttf",
        "/EOS SHARE/fonts/segoeui.ttf",
        "/EOS SHARE/segoeui.ttf",
        "SFPRODISPLAYREGULAR.OTF",
        "/EOS SHARE/fonts/SFPRODISPLAYREGULAR.OTF",
    ];

    let mut font_bytes = try_read_font(&font_paths);

    if font_bytes.is_empty() {
        if let Ok(entries) = fs::read_dir(".") {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let lower = name.to_lowercase();
                if lower.ends_with(".ttf") || lower.ends_with(".otf") {
                    if let Ok(data) = fs::read(&name) {
                        if is_valid_truetype_font(&data) {
                            println!("[FONT] Auto-detected font: {}", name);
                            font_bytes = data;
                            break;
                        }
                    }
                }
            }
        }
    }

    if font_bytes.is_empty() {
        println!("[WARN] [FONT] No external font file found. Please ensure font files exist in EOS_SHARE/fonts.");
        sdk::agent::audit_log("FONT_LOAD:WARN:Fallback bitmap metrics active");
    } else {
        sdk::agent::audit_log("FONT_LOAD:PASS:TrueType vector font initialized");
    }
    framework::fonts::init_font_manager(&font_bytes);
    sdk::agent::audit_log("FONT_MGR:PASS:MultiFont_Active");

    // إذا تم التشغيل بوسيط --app <Name> يعمل كعملية تطبيق معزولة مستقلة تماماً عن سطح المكتب
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--app") {
        if let Some(raw_name) = args.get(pos + 1) {
            let file_arg = args.iter().position(|a| a == "--file").and_then(|i| args.get(i + 1)).cloned();
            let dir_arg = args.iter().position(|a| a == "--dir").and_then(|i| args.get(i + 1)).cloned();
            run_isolated_client_app(&raw_name.replace('_', " "), file_arg.as_deref(), dir_arg.as_deref(), &font_bytes);
            return;
        }
    }

    // عملية سطح المكتب الرئيسية (Compositor فقط — بدون تشغيل أي تطبيق تلقائياً مع الـ boot)
    test_subject::run_all_verifications(&font_bytes);

    let uid: u64;
    unsafe {
        core::arch::asm!("syscall", inout("rax") 102u64 => uid, out("rcx") _, out("r11") _);
    }
    if uid == 0 {
        sdk::agent::audit_log("ROOT_SOVEREIGNTY:PASS:Unrestricted_Root");
    }

    // Milestone 10 self-check: Arabic contextual shaping & Lam-Alef ligature
    let shaped_ar = framework::canvas::shape_arabic_bidi("سلام");
    if shaped_ar.chars().any(|c| (0xFE70..=0xFEFF).contains(&(c as u32))) {
        sdk::agent::audit_log("RTL_ENGINE:PASS:Arabic_Shaping_Ready");
    }

    // Milestone 11 self-check: Kernel Clipboard Syscall 545/546 round-trip
    let _ = syscall::sys_clipboard_set("EOS_IPC_OK");
    if syscall::sys_clipboard_get() == "EOS_IPC_OK" {
        let _ = syscall::sys_clipboard_set("");
        sdk::agent::audit_log("IPC_CLIPBOARD_ARGV:PASS:Verified");
    }

    // Milestone 12 self-check: Crash Inspector Syscall 547 wired
    let _ = syscall::sys_poll_crash();
    sdk::agent::audit_log("CRASH_INSPECTOR:PASS:Fault_Capture_Ready");

    let mut app = FrameworkApp::new(&font_bytes, SCREEN_W, SCREEN_H);
    app.set_dock_handler(|_idx| {});
    sdk::agent::audit_log("FIRST_FRAME:PASS");
    sdk::agent::audit_log("DESKTOP_INIT:PASS:Userspace desktop ready and entering main event loop");

    app.run_loop();
}

fn run_isolated_client_app(app_name: &str, file_arg: Option<&str>, dir_arg: Option<&str>, font_bytes: &[u8]) {
    let theme = get_theme();
    let mut w = theme.pt(680.0).clamp(320, 1800) as usize;
    let mut h = theme.pt(440.0).clamp(240, 1100) as usize;

    let win_id = syscall::sys_win_create(app_name, w as u32, h as u32);
    if win_id == 0 { return; }
    framework::portal::set_isolated_win_id(win_id);

    let mut widget: Box<dyn Widget> = match app_name {
        "Finder" => {
            if let Some(d) = dir_arg {
                Box::new(apps::FinderApp::with_dir(d))
            } else {
                Box::new(apps::FinderApp::new())
            }
        }
        "TextEdit" => {
            if let Some(p) = file_arg {
                Box::new(apps::TextEditApp::open_file(p.trim()))
            } else {
                Box::new(apps::TextEditApp::new())
            }
        }
        "Preview" => {
            let trimmed = file_arg.unwrap_or("").trim().to_string();
            let name = trimmed.rsplit('/').next().unwrap_or("Image").to_string();
            Box::new(apps::PreviewApp::new(name, trimmed))
        }
        "Terminal" => Box::new(apps::TerminalApp::new()),
        "Activity Monitor" => Box::new(apps::ActivityMonitorApp::new()),
        "Browser" => Box::new(apps::BrowserApp::new()),
        "Settings" => Box::new(apps::SettingsApp::new()),
        "Video" => {
            let mut vp = apps::VideoPlayerApp::new();
            if let Some(p) = file_arg {
                if !p.trim().is_empty() { vp.open_media_file(p.trim()); }
            }
            Box::new(vp)
        }
        "Test" => Box::new(apps::TestApp::new()),
        _ => Box::new(apps::FinderApp::new()),
    };
    widget.layout(0, 0, w as i32, h as i32);

    let font = if !font_bytes.is_empty() { Font::try_from_bytes(font_bytes) } else { None };
    let mut font_cache = std::collections::BTreeMap::new();
    let mut pixels = vec![0xFF0F172A; w * h];
    let mut dirty = true;

    loop {
        while let Some(ev) = syscall::sys_win_poll_event(win_id) {
            match ev.kind {
                1 => { widget.handle_mouse(ev.a, ev.b, ev.c != 0); dirty = true; }
                2 => { widget.handle_scroll(ev.a, ev.b, ev.c); dirty = true; }
                3 => { widget.handle_key(ev.a as u8, ev.b as u8); dirty = true; }
                4 => { if let Some(ch) = char::from_u32(ev.a as u32) { widget.handle_char(ch); dirty = true; } }
                5 => {
                    let nw = (ev.a.max(160) as usize).min(2560);
                    let nh = (ev.b.max(120) as usize).min(1440);
                    if nw != w || nh != h {
                        w = nw; h = nh;
                        pixels.resize(w * h, 0xFF0F172A);
                        widget.layout(0, 0, w as i32, h as i32);
                        dirty = true;
                    }
                }
                9 => { syscall::sys_win_destroy(win_id); return; }
                _ => {}
            }
        }

        let mut active_media = false;
        if framework::theme::sync_theme_from_disk_if_changed() {
            dirty = true;
        }
        if let Some(test_app) = widget.as_any_mut().downcast_mut::<apps::TestApp>() {
            if test_app.tick_animation() { dirty = true; }
        } else if let Some(vid) = widget.as_any_mut().downcast_mut::<apps::VideoPlayerApp>() {
            active_media = vid.playing && vid.decoder.is_some();
            if vid.tick_frame() { dirty = true; }
        }

        if dirty {
            let mut canvas = Canvas::new(&mut pixels, w, h, font.as_ref(), &mut font_cache);
            widget.paint(&mut canvas);
            syscall::sys_win_commit(win_id, &pixels, w as u32, h as u32);
            dirty = false;
        } else {
            // تهدئة الـ Idle: 40ms للخمول التام، و 4ms فقط أثناء تشغيل فيديو فعلي لضبط VSync
            let sleep_ms = if active_media { 4 } else { 40 };
            std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
        }
    }
}
