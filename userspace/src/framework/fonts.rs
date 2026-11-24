use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;
use rusttype::Font;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontRole {
    Ui,
    Mono,
    Arabic,
}

struct FontRegistry {
    fonts: BTreeMap<String, &'static Font<'static>>,
    font_paths: BTreeMap<String, String>,
    ui_font: String,
    mono_font: String,
    arabic_font: String,
}

static FONT_REGISTRY: Mutex<Option<FontRegistry>> = Mutex::new(None);

fn classify_font_role(name: &str) -> FontRole {
    let l = name.to_ascii_lowercase();
    if l.contains("mono") || l.contains("code") || l.contains("consol") || l.contains("courier") || l.contains("jetbrains") {
        FontRole::Mono
    } else if l.contains("arab") || l.contains("naskh") || l.contains("amiri") || l.contains("cairo") || l.contains("tahoma") {
        FontRole::Arabic
    } else {
        FontRole::Ui
    }
}

fn leak_font(bytes: Vec<u8>) -> Option<&'static Font<'static>> {
    let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    let font = Font::try_from_bytes(leaked)?;
    Some(Box::leak(Box::new(font)))
}

/// Instant initialization: indexes font filenames lazily without reading all TTF/OTF files over ATA PIO.
pub fn init_font_manager(initial_bytes: &[u8]) {
    let mut reg = FontRegistry {
        fonts: BTreeMap::new(),
        font_paths: BTreeMap::new(),
        ui_font: String::from("System Sans"),
        mono_font: String::from("System Mono"),
        arabic_font: String::from("System Sans"),
    };

    if !initial_bytes.is_empty() {
        if let Some(f) = leak_font(initial_bytes.to_vec()) {
            reg.fonts.insert(String::from("System Sans"), f);
            reg.fonts.insert(String::from("System Mono"), f);
        }
    }

    let scan_dirs = ["/EOS SHARE/fonts", "/Initrd/fonts"];
    for dir in &scan_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for e in entries.flatten() {
                let fname = e.file_name().to_string_lossy().to_string();
                let lower = fname.to_ascii_lowercase();
                if !(lower.ends_with(".ttf") || lower.ends_with(".otf")) {
                    continue;
                }
                let display_name = fname
                    .rsplit_once('.')
                    .map(|(stem, _)| stem.to_string())
                    .unwrap_or_else(|| fname.clone());
                if reg.font_paths.contains_key(&display_name) {
                    continue;
                }
                let full_path = format!("{}/{}", dir.trim_end_matches('/'), fname);
                let role = classify_font_role(&display_name);
                if role == FontRole::Mono && reg.mono_font == "System Mono" {
                    reg.mono_font = display_name.clone();
                } else if role == FontRole::Arabic && reg.arabic_font == "System Sans" {
                    reg.arabic_font = display_name.clone();
                }
                reg.font_paths.insert(display_name, full_path);
            }
        }
    }

    let mut lock = FONT_REGISTRY.lock().unwrap();
    *lock = Some(reg);
}

pub fn get_font_for_role(role: FontRole) -> Option<&'static Font<'static>> {
    let mut lock = FONT_REGISTRY.lock().ok()?;
    let reg = lock.as_mut()?;
    let target = match role {
        FontRole::Ui => reg.ui_font.clone(),
        FontRole::Mono => reg.mono_font.clone(),
        FontRole::Arabic => reg.arabic_font.clone(),
    };
    if let Some(&f) = reg.fonts.get(&target) {
        return Some(f);
    }
    if let Some(path) = reg.font_paths.get(&target).cloned() {
        if let Ok(bytes) = fs::read(&path) {
            if let Some(f) = leak_font(bytes) {
                reg.fonts.insert(target.clone(), f);
                return Some(f);
            }
        }
    }
    reg.fonts.values().next().copied()
}

pub fn set_role_font(role: FontRole, name: &str) -> bool {
    if let Ok(mut lock) = FONT_REGISTRY.lock() {
        if let Some(reg) = lock.as_mut() {
            if reg.fonts.contains_key(name) || reg.font_paths.contains_key(name) {
                match role {
                    FontRole::Ui => reg.ui_font = name.to_string(),
                    FontRole::Mono => reg.mono_font = name.to_string(),
                    FontRole::Arabic => reg.arabic_font = name.to_string(),
                }
                return true;
            }
        }
    }
    false
}

pub fn active_font_name(role: FontRole) -> String {
    if let Ok(lock) = FONT_REGISTRY.lock() {
        if let Some(reg) = lock.as_ref() {
            return match role {
                FontRole::Ui => reg.ui_font.clone(),
                FontRole::Mono => reg.mono_font.clone(),
                FontRole::Arabic => reg.arabic_font.clone(),
            };
        }
    }
    String::from("System Sans")
}

pub fn list_available_fonts() -> Vec<String> {
    if let Ok(lock) = FONT_REGISTRY.lock() {
        if let Some(reg) = lock.as_ref() {
            let mut names: Vec<String> = reg.fonts.keys().cloned().collect();
            for k in reg.font_paths.keys() {
                if !names.contains(k) {
                    names.push(k.clone());
                }
            }
            return names;
        }
    }
    vec![String::from("System Sans")]
}
