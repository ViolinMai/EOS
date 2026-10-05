#[derive(Clone, Copy)]
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
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub accent_active: u32,
}

pub static mut CURRENT_THEME: Theme = Theme {
    scale: 2.0, // مقياس 2x كافتراضي
    bg_desktop: 0xFF1E293B,
    bg_menubar: 0xEE1E293B,
    border_menubar: 0x33FFFFFF,
    bg_dock: 0xCC0F172A,
    border_dock: 0x44FFFFFF,
    bg_window: 0xFF0F172A,
    border_window: 0x33FFFFFF,
    bg_titlebar: 0xFF1E293B,
    text_primary: 0xFFF8FAFC,
    text_secondary: 0xFF94A3B8,
    text_muted: 0xFF64748B,
    accent: 0xFF0284C7,
    accent_hover: 0xFF0369A1,
    accent_active: 0xFF075985,
};

pub static mut DESKTOP_WALLPAPER: Option<(Vec<u32>, usize, usize)> = None;

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

    #[inline(always)]
    pub fn font_heading(&self) -> usize {
        (28.0 * self.scale).round().max(22.0) as usize
    }
}

pub fn get_theme() -> &'static Theme {
    unsafe { &*core::ptr::addr_of!(CURRENT_THEME) }
}

pub fn set_theme_scale(scale: f32) {
    unsafe {
        let t = &mut *core::ptr::addr_of_mut!(CURRENT_THEME);
        t.scale = scale.clamp(1.0, 4.0);
    }
}
