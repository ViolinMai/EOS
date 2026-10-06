use std::sync::Mutex;
use core::sync::atomic::{AtomicBool, Ordering};

pub static mut DESKTOP_WALLPAPER: Option<(Vec<u32>, usize, usize)> = None;

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub scale: f32,
    pub bg_desktop: u32,
    pub bg_window: u32,
    pub bg_titlebar: u32,
    pub border_window: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            scale: 2.0, // إرجاع الـ UI Scale إلى 2.0x السابق
            bg_desktop: 0xFF0F172A,
            bg_window: 0xFF1E293B,
            bg_titlebar: 0xFF334155,
            border_window: 0xFF475569,
            accent: 0xFF0284C7,
            accent_hover: 0xFF0369A1,
            text_primary: 0xFFF8FAFC,
            text_secondary: 0xFF94A3B8,
            text_muted: 0xFF64748B,
        }
    }
}

impl Theme {
    #[inline(always)]
    pub fn pt(&self, val: f32) -> i32 {
        (val * self.scale).round() as i32
    }

    #[inline(always)]
    pub fn font_caption(&self) -> usize {
        (11.0 * self.scale).round().max(10.0) as usize
    }

    #[inline(always)]
    pub fn font_body(&self) -> usize {
        (13.5 * self.scale).round().max(12.0) as usize
    }

    #[inline(always)]
    pub fn font_title(&self) -> usize {
        (16.0 * self.scale).round().max(14.0) as usize
    }

    #[inline(always)]
    pub fn font_large(&self) -> usize {
        (22.0 * self.scale).round().max(18.0) as usize
    }
}

static CURRENT_THEME: Mutex<Theme> = Mutex::new(Theme {
    scale: 2.0,
    bg_desktop: 0xFF0F172A,
    bg_window: 0xFF1E293B,
    bg_titlebar: 0xFF334155,
    border_window: 0xFF475569,
    accent: 0xFF0284C7,
    accent_hover: 0xFF0369A1,
    text_primary: 0xFFF8FAFC,
    text_secondary: 0xFF94A3B8,
    text_muted: 0xFF64748B,
});

pub fn get_theme() -> Theme {
    *CURRENT_THEME.lock().unwrap()
}

pub fn set_theme_scale(scale: f32) {
    let mut t = CURRENT_THEME.lock().unwrap();
    t.scale = scale.clamp(1.0, 4.0);
}

pub static WALLPAPER_CHANGED: AtomicBool = AtomicBool::new(false);

pub fn notify_wallpaper_changed() {
    WALLPAPER_CHANGED.store(true, Ordering::Release);
}

pub fn take_wallpaper_changed() -> bool {
    WALLPAPER_CHANGED.swap(false, Ordering::AcqRel)
}
