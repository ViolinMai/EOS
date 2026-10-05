mod syscall;
mod sdk;
mod framework;
pub mod png;
pub mod net_manager;
#[path = "apps/mod.rs"]
pub mod apps;

use std::fs;
use framework::*;
use rusttype::Font;

const SCREEN_W: usize = 1920;
const SCREEN_H: usize = 1080;

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
    }

    let mut app = FrameworkApp::new(&font_bytes, SCREEN_W, SCREEN_H);

    app.spawn_app("Finder");
    app.spawn_app("Settings");

    app.set_dock_handler(|_idx| {});
    app.run_loop();
}
