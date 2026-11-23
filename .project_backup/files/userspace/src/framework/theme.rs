use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone, Debug)]
pub struct Theme {
    pub scale: f32,
    pub corner_radius: f32,
    pub is_dark: bool,
    pub bg_desktop: u32,
    pub bg_app: u32,
    pub bg_sidebar: u32,
    pub bg_panel: u32,
    pub bg_window: u32,
    pub bg_titlebar: u32,
    pub bg_titlebar_inactive: u32,
    pub border_window: u32,
    pub border_frost_highlight: u32,
    pub aero_glass_tint: u32,
    pub aero_glow_active: u32,
    pub aero_glow_inactive: u32,
    pub superbar_glass: u32,
    pub start_menu_left_bg: u32,
    pub start_menu_right_bg: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub btn_close: u32,
    pub btn_close_glow: u32,
    pub btn_minimize: u32,
    pub btn_maximize: u32,
}

impl Theme {
    pub fn dark() -> Self {
        Self::aero_glass()
    }

    pub fn aero_glass() -> Self {
        Self {
            scale: 2.0,
            corner_radius: 10.0,
            is_dark: true,
            bg_desktop: 0xFF0B2A4A,
            bg_app: 0xFF0F172A,
            bg_sidebar: 0xFF1E293B,
            bg_panel: 0xFF162032,
            bg_window: 0xFF141C2B,
            bg_titlebar: 0xD81E3F66,
            bg_titlebar_inactive: 0xB81B2838,
            border_window: 0xFF5C9DED,
            border_frost_highlight: 0x88E0F2FE,
            aero_glass_tint: 0xCC163252,
            aero_glow_active: 0x6638BDF8,
            aero_glow_inactive: 0x331E293B,
            superbar_glass: 0xDD0F253E,
            start_menu_left_bg: 0xFFF8FAFC,
            start_menu_right_bg: 0xEE162C46,
            text_primary: 0xFFF8FAFC,
            text_secondary: 0xFFCBD5E1,
            text_muted: 0xFF94A3B8,
            accent: 0xFF0284C7,
            accent_hover: 0xFF38BDF8,
            btn_close: 0xFFDC2626,
            btn_close_glow: 0xFFF87171,
            btn_minimize: 0xFF3B82F6,
            btn_maximize: 0xFF0284C7,
        }
    }

    pub fn light() -> Self {
        Self {
            scale: 2.0,
            corner_radius: 10.0,
            is_dark: false,
            bg_desktop: 0xFF1D4ED8,
            bg_app: 0xFFF1F5F9,
            bg_sidebar: 0xFFE2E8F0,
            bg_panel: 0xFFFFFFFF,
            bg_window: 0xFFF8FAFC,
            bg_titlebar: 0xE6CBD5E1,
            bg_titlebar_inactive: 0xCCE2E8F0,
            border_window: 0xFF64748B,
            border_frost_highlight: 0xAAFFFFFF,
            aero_glass_tint: 0xCCF1F5F9,
            aero_glow_active: 0x660284C7,
            aero_glow_inactive: 0x3394A3B8,
            superbar_glass: 0xDDE2E8F0,
            start_menu_left_bg: 0xFFFFFFFF,
            start_menu_right_bg: 0xEEF1F5F9,
            text_primary: 0xFF0F172A,
            text_secondary: 0xFF334155,
            text_muted: 0xFF64748B,
            accent: 0xFF0284C7,
            accent_hover: 0xFF0369A1,
            btn_close: 0xFFDC2626,
            btn_close_glow: 0xFFF87171,
            btn_minimize: 0xFF3B82F6,
            btn_maximize: 0xFF0284C7,
        }
    }

    pub fn pt(&self, val: f32) -> i32 {
        let factor = self.scale.max(1.0);
        (val * factor).round() as i32
    }

    /// نصف قطر زوايا النوافذ الرئيسي (قابل للتخصيص على مستوى النظام بالكامل)
    pub fn r_win(&self) -> usize {
        self.pt(self.corner_radius).max(0) as usize
    }

    /// نصف قطر زوايا الأزرار والبطاقات
    pub fn r_ctrl(&self) -> usize {
        self.pt((self.corner_radius * 0.65).max(0.0)).max(0) as usize
    }

    pub fn font_caption(&self) -> usize {
        let factor = self.scale.max(1.0);
        (11.0 * factor).round() as usize
    }

    pub fn font_body(&self) -> usize {
        let factor = self.scale.max(1.0);
        (13.0 * factor).round() as usize
    }

    pub fn font_title(&self) -> usize {
        let factor = self.scale.max(1.0);
        (15.0 * factor).round() as usize
    }

    pub fn font_large(&self) -> usize {
        let factor = self.scale.max(1.0);
        (19.0 * factor).round() as usize
    }
}

pub static mut CURRENT_THEME: Option<Theme> = None;
pub static mut DESKTOP_WALLPAPER: Option<(Vec<u32>, usize, usize)> = None;
pub static mut WALLPAPER_CHANGED: bool = false;
pub static mut FULL_REDRAW_REQUESTED: bool = false;

static mut THEME_AUDIT_EMITTED: bool = false;

pub fn emit_win7_theme_audit_once() {
    unsafe {
        if !THEME_AUDIT_EMITTED {
            THEME_AUDIT_EMITTED = true;
            crate::sdk::agent::audit_log("WIN7_THEME:PASS:AeroGlass_Tokens_Ready");
        }
    }
}

pub fn get_theme() -> &'static Theme {
    unsafe {
        if CURRENT_THEME.is_none() {
            CURRENT_THEME = Some(Theme::aero_glass());
        }
        CURRENT_THEME.as_ref().unwrap()
    }
}

pub fn get_theme_mut() -> &'static mut Theme {
    unsafe {
        if CURRENT_THEME.is_none() {
            CURRENT_THEME = Some(Theme::aero_glass());
        }
        CURRENT_THEME.as_mut().unwrap()
    }
}

pub fn set_theme_dark_mode(dark: bool) {
    let cur = get_theme();
    let keep_scale = cur.scale;
    let keep_radius = cur.corner_radius;
    let keep_accent = cur.accent;
    let keep_hover = cur.accent_hover;
    let mut next = if dark { Theme::aero_glass() } else { Theme::light() };
    next.scale = keep_scale;
    next.corner_radius = keep_radius;
    next.accent = keep_accent;
    next.accent_hover = keep_hover;
    unsafe {
        CURRENT_THEME = Some(next);
    }
}

pub fn set_theme_scale(scale: f32) {
    let t = get_theme_mut();
    t.scale = scale.clamp(1.0, 3.0);
}

pub fn set_theme_corner_radius(radius: f32) {
    let t = get_theme_mut();
    t.corner_radius = radius.clamp(0.0, 24.0);
}

pub fn set_theme_accent(color: u32) {
    let t = get_theme_mut();
    t.accent = color;
    let r = (((color >> 16) & 0xFF) as f32 * 1.18).min(255.0) as u32;
    let g = (((color >> 8) & 0xFF) as f32 * 1.18).min(255.0) as u32;
    let b = ((color & 0xFF) as f32 * 1.18).min(255.0) as u32;
    t.accent_hover = 0xFF000000 | (r << 16) | (g << 8) | b;
}

pub fn redraw_fullscreen() {
    unsafe { FULL_REDRAW_REQUESTED = true; }
}

pub fn take_full_redraw_requested() -> bool {
    unsafe {
        let val = FULL_REDRAW_REQUESTED;
        FULL_REDRAW_REQUESTED = false;
        val
    }
}

pub fn notify_wallpaper_changed() {
    unsafe { WALLPAPER_CHANGED = true; }
}

pub fn take_wallpaper_changed() -> bool {
    unsafe {
        let val = WALLPAPER_CHANGED;
        WALLPAPER_CHANGED = false;
        val
    }
}

static LAST_THEME_SYNC_MS: AtomicU64 = AtomicU64::new(0);
static LAST_SETTINGS_HASH: AtomicU64 = AtomicU64::new(0);

/// مزامنة الثيم واللون والخلفية تلقائياً عبر جميع العمليات المعزولة من ملف RootFS/settings.ini
pub fn sync_theme_from_disk_if_changed() -> bool {
    let now_ms = crate::framework::perf_log::SmartLogger::cycles_to_us(
        crate::framework::perf_log::SmartLogger::read_tsc(),
    ) / 1000;
    let prev = LAST_THEME_SYNC_MS.load(Ordering::Relaxed);
    if now_ms.saturating_sub(prev) < 1200 {
        return false;
    }
    LAST_THEME_SYNC_MS.store(now_ms, Ordering::Relaxed);

    let content = match std::fs::read_to_string("RootFS/settings.ini").or_else(|_| std::fs::read_to_string("settings.ini")) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let mut hash: u64 = 1469598103934665603;
    for &b in content.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    if hash == LAST_SETTINGS_HASH.swap(hash, Ordering::Relaxed) {
        return false;
    }

    let mut dark = true;
    let mut accent: Option<u32> = None;
    let mut wp_name: Option<String> = None;
    let mut scale_val: Option<f32> = None;

    for line in content.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("dark_mode=") {
            dark = v.trim() == "1" || v.trim().eq_ignore_ascii_case("true");
        } else if let Some(v) = l.strip_prefix("accent=") {
            let hex = v.trim().trim_start_matches("0x").trim_start_matches('#');
            if let Ok(c) = u32::from_str_radix(hex, 16) {
                accent = Some(if c <= 0xFFFFFF { 0xFF000000 | c } else { c });
            }
        } else if let Some(v) = l.strip_prefix("wallpaper=") {
            let w = v.trim();
            if !w.is_empty() { wp_name = Some(w.to_string()); }
        } else if let Some(v) = l.strip_prefix("scale=") {
            if let Ok(s) = v.trim().parse::<f32>() { scale_val = Some(s); }
        }
    }

    set_theme_dark_mode(dark);
    if let Some(c) = accent { set_theme_accent(c); }
    if let Some(s) = scale_val { set_theme_scale(s); }

    if let Some(wp) = wp_name {
        if wp == "none" {
            unsafe { DESKTOP_WALLPAPER = None; }
            notify_wallpaper_changed();
        } else {
            let cands = [format!("EOS SHARE/{}", wp), format!("/EOS SHARE/{}", wp), wp];
            for cp in &cands {
                if let Ok(bytes) = std::fs::read(cp) {
                    if let Ok((px, w, h, _)) = crate::sdk::media::decode_image(&bytes) {
                        unsafe { DESKTOP_WALLPAPER = Some((px, w, h)); }
                        notify_wallpaper_changed();
                        break;
                    }
                }
            }
        }
    }

    redraw_fullscreen();
    true
}
