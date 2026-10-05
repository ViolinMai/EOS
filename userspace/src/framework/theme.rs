use std::sync::atomic::{AtomicU32, Ordering};

pub struct Theme {
    pub scale: f32,
    pub bg_desktop: u32,
    pub bg_menubar: u32,
    pub border_menubar: u32,
    pub bg_dock: u32,
    pub border_dock: u32,
    pub bg_window: u32,
    pub border_window: u32,
    pub bg_titlebar: u32,
    pub border_titlebar: u32,
    pub separator: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub btn_close: u32,
    pub btn_min: u32,
    pub btn_max: u32,
    pub radius_window: usize,
    pub radius_dock: usize,
}

impl Theme {
    #[inline(always)]
    pub fn pt(&self, val: f32) -> usize {
        (val * self.scale).round() as usize
    }

    pub fn font_caption(&self) -> usize { self.pt(12.0) }
    pub fn font_body(&self) -> usize { self.pt(14.0) }
    pub fn font_title(&self) -> usize { self.pt(16.0) }
    pub fn font_large(&self) -> usize { self.pt(20.0) }
}

static CURRENT_SCALE_BITS: AtomicU32 = AtomicU32::new(0x3FE00000); // 1.75f32

pub fn set_theme_scale(scale: f32) {
    CURRENT_SCALE_BITS.store(scale.to_bits(), Ordering::Relaxed);
}

pub fn get_theme() -> Theme {
    let scale = f32::from_bits(CURRENT_SCALE_BITS.load(Ordering::Relaxed));
    let s_pt = |v: f32| (v * scale).round() as usize;

    Theme {
        scale,
        bg_desktop: 0xFF0F172A,
        bg_menubar: 0xF018181B,
        border_menubar: 0x25FFFFFF,
        bg_dock: 0xE01E293B,
        border_dock: 0x30FFFFFF,
        bg_window: 0xFF18181B,
        border_window: 0xFF334155,
        bg_titlebar: 0xFF27272A,
        border_titlebar: 0xFF3F3F46,
        separator: 0xFF3F3F46,
        text_primary: 0xFFF8FAFC,
        text_secondary: 0xFFCBD5E1,
        text_muted: 0xFF94A3B8,
        accent: 0xFF0284C7,
        accent_hover: 0xFF38BDF8,
        btn_close: 0xFFFF5F56,
        btn_min: 0xFFFFBD2E,
        btn_max: 0xFF27C93F,
        radius_window: s_pt(12.0),
        radius_dock: s_pt(20.0),
    }
}
