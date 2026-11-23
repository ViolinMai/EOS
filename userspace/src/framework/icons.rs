use std::collections::HashMap;
use crate::framework::canvas::Canvas;
use crate::framework::svg::rasterize_svg;

struct CachedSvgIcon {
    w: usize,
    h: usize,
    pixels: Vec<u32>,
}

static EOS_ORB_LOGO_PNG: &[u8] = include_bytes!("../../assets/icon.png");
static mut EOS_ORB_CACHE: Option<(usize, usize, Vec<u32>)> = None;
static mut SVG_ICON_CACHE: Option<HashMap<(String, usize, usize), CachedSvgIcon>> = None;

pub fn draw_eos_logo(canvas: &mut Canvas, dx: i32, dy: i32, dw: i32, dh: i32) -> bool {
    if dw <= 0 || dh <= 0 { return false; }
    let (iw, ih, px_ref) = unsafe {
        if EOS_ORB_CACHE.is_none() {
            if let Ok((pixels, w, h)) = crate::png::decode_png(EOS_ORB_LOGO_PNG) {
                EOS_ORB_CACHE = Some((w, h, pixels));
            }
        }
        match EOS_ORB_CACHE.as_ref() {
            Some((w, h, p)) => (*w, *h, p),
            None => return false,
        }
    };
    let clip = canvas.current_clip();
    let x1 = dx.max(clip.x).max(0);
    let y1 = dy.max(clip.y).max(0);
    let x2 = (dx + dw).min(clip.x + clip.w).min(canvas.width as i32);
    let y2 = (dy + dh).min(clip.y + clip.h).min(canvas.height as i32);
    for py in y1..y2 {
        let sy = (((py - dy) as usize) * ih) / (dh as usize);
        let src_row = sy.min(ih - 1) * iw;
        let dst_row = (py as usize) * canvas.width;
        for px in x1..x2 {
            let sx = (((px - dx) as usize) * iw) / (dw as usize);
            let fg = px_ref[src_row + sx.min(iw - 1)];
            let idx = dst_row + (px as usize);
            canvas.buffer[idx] = blend_px(canvas.buffer[idx], fg);
        }
    }
    true
}

fn lookup_embedded_svg(name: &str) -> Option<&'static str> {
    match name {
        // Core Apps & Shell
        "video" => Some(include_str!("../../assets/icons/video.svg")),
        "image" | "preview" => Some(include_str!("../../assets/icons/image.svg")),
        "document" | "doc" | "textedit" => Some(include_str!("../../assets/icons/document.svg")),
        "console" | "terminal" => Some(include_str!("../../assets/icons/console.svg")),
        "settings" | "gear" => Some(include_str!("../../assets/icons/settings.svg")),
        "chrome" | "browser" => Some(include_str!("../../assets/icons/chrome.svg")),
        "tune" | "test" => Some(include_str!("../../assets/icons/tune.svg")),
        "lighthouse" | "monitor" => Some(include_str!("../../assets/icons/lighthouse.svg")),
        "virtual" | "computer" => Some(include_str!("../../assets/icons/virtual.svg")),
        "disc" | "recycle_bin" => Some(include_str!("../../assets/icons/disc.svg")),
        "lib" | "folder" | "finder" => Some(include_str!("../../assets/icons/lib.svg")),
        "tree" => Some(include_str!("../../assets/icons/tree.svg")),

        // File Extensions
        "rust" => Some(include_str!("../../assets/icons/rust.svg")),
        "python" => Some(include_str!("../../assets/icons/python.svg")),
        "c" => Some(include_str!("../../assets/icons/c.svg")),
        "cpp" => Some(include_str!("../../assets/icons/cpp.svg")),
        "json" => Some(include_str!("../../assets/icons/json.svg")),
        "markdown" => Some(include_str!("../../assets/icons/markdown.svg")),
        "toml" => Some(include_str!("../../assets/icons/toml.svg")),
        "yaml" => Some(include_str!("../../assets/icons/yaml.svg")),
        "svg" => Some(include_str!("../../assets/icons/svg.svg")),
        "lua" => Some(include_str!("../../assets/icons/lua.svg")),
        "pdf" => Some(include_str!("../../assets/icons/pdf.svg")),
        "zip" => Some(include_str!("../../assets/icons/zip.svg")),
        "audio" => Some(include_str!("../../assets/icons/audio.svg")),
        "subtitles" => Some(include_str!("../../assets/icons/subtitles.svg")),
        "exe" => Some(include_str!("../../assets/icons/exe.svg")),
        "lock" => Some(include_str!("../../assets/icons/lock.svg")),
        "log" => Some(include_str!("../../assets/icons/log.svg")),
        "git" => Some(include_str!("../../assets/icons/git.svg")),
        _ => None,
    }
}

/// مصدر الأيقونات الموحد (Single Source of Truth) لكل التطبيقات في النظام
pub fn icon_for_app(app_name: &str) -> &'static str {
    match app_name {
        "Finder" | "Windows Explorer" | "Computer" => "lib",
        "TextEdit" | "Notepad" => "document",
        "Terminal" | "Command Prompt" => "console",
        "Activity Monitor" | "Windows Task Manager" => "lighthouse",
        "Browser" | "Internet Explorer" => "chrome",
        "Settings" | "Control Panel" => "settings",
        "Video" | "Video Player" => "video",
        "Test" | "Framework Test" => "tune",
        "Recycle Bin" => "disc",
        _ => "lib",
    }
}

pub fn draw_app_icon(canvas: &mut Canvas, app_name: &str, dx: i32, dy: i32, dw: i32, dh: i32) -> bool {
    draw_svg_icon(canvas, icon_for_app(app_name), dx, dy, dw, dh)
}

pub fn icon_for_filename(name: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "lib";
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".mp4") || lower.ends_with(".mkv") || lower.ends_with(".avi") || lower.ends_with(".webm") || lower.ends_with(".mov") {
        "video"
    } else if lower.ends_with(".mp3") || lower.ends_with(".wav") || lower.ends_with(".flac") || lower.ends_with(".ogg") || lower.ends_with(".aac") {
        "audio"
    } else if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".bmp") || lower.ends_with(".webp") {
        "image"
    } else if lower.ends_with(".svg") {
        "svg"
    } else if lower.ends_with(".srt") || lower.ends_with(".ass") {
        "subtitles"
    } else if lower.ends_with(".rs") {
        "rust"
    } else if lower.ends_with(".py") {
        "python"
    } else if lower.ends_with(".c") {
        "c"
    } else if lower.ends_with(".cpp") || lower.ends_with(".cc") || lower.ends_with(".hpp") {
        "cpp"
    } else if lower.ends_with(".lua") {
        "lua"
    } else if lower.ends_with(".json") {
        "json"
    } else if lower.ends_with(".md") {
        "markdown"
    } else if lower.ends_with(".toml") {
        "toml"
    } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        "yaml"
    } else if lower.ends_with(".pdf") {
        "pdf"
    } else if lower.ends_with(".zip") || lower.ends_with(".tar") || lower.ends_with(".gz") {
        "zip"
    } else if lower.ends_with(".elf") || lower.ends_with(".exe") || lower.ends_with(".bin") {
        "exe"
    } else if lower.ends_with(".lock") {
        "lock"
    } else if lower.ends_with(".log") {
        "log"
    } else if lower.starts_with(".git") {
        "git"
    } else {
        "document"
    }
}

#[inline(always)]
fn blend_px(bg: u32, fg: u32) -> u32 {
    let a = (fg >> 24) & 0xFF;
    if a == 0 { return bg; }
    if a == 255 { return fg; }
    let inv = 255 - a;
    let rb = (((fg & 0x00FF00FF) * a + (bg & 0x00FF00FF) * inv + 0x00800080) >> 8) & 0x00FF00FF;
    let g = (((fg & 0x0000FF00) * a + (bg & 0x0000FF00) * inv + 0x00008000) >> 8) & 0x0000FF00;
    0xFF000000 | rb | g
}

pub fn draw_svg_icon(canvas: &mut Canvas, name: &str, dx: i32, dy: i32, dw: i32, dh: i32) -> bool {
    if dw <= 0 || dh <= 0 { return false; }
    let w = dw as usize;
    let h = dh as usize;

    let cache = unsafe {
        if SVG_ICON_CACHE.is_none() {
            SVG_ICON_CACHE = Some(HashMap::new());
        }
        SVG_ICON_CACHE.as_mut().unwrap()
    };

    let key = (name.to_string(), w, h);
    if !cache.contains_key(&key) {
        let Some(svg_xml) = lookup_embedded_svg(name) else { return false };
        let pixels = rasterize_svg(svg_xml, w, h);
        cache.insert(key.clone(), CachedSvgIcon { w, h, pixels });
    }

    let icon = cache.get(&key).unwrap();
    let clip = canvas.current_clip();
    let x1 = dx.max(clip.x).max(0);
    let y1 = dy.max(clip.y).max(0);
    let x2 = (dx + dw).min(clip.x + clip.w).min(canvas.width as i32);
    let y2 = (dy + dh).min(clip.y + clip.h).min(canvas.height as i32);

    for py in y1..y2 {
        let sy = (py - dy) as usize;
        if sy >= icon.h { continue; }
        let src_row = sy * icon.w;
        let dst_row = (py as usize) * canvas.width;
        for px in x1..x2 {
            let sx = (px - dx) as usize;
            if sx >= icon.w { continue; }
            let fg = icon.pixels[src_row + sx];
            let idx = dst_row + (px as usize);
            canvas.buffer[idx] = blend_px(canvas.buffer[idx], fg);
        }
    }
    true
}
