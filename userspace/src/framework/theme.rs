#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub scale: f32,
    pub bg_desktop: u32,
    pub bg_menubar: u32,
    pub border_menubar: u32,
    pub bg_dock: u32,
    pub border_dock: u32,
    pub bg_window: u32,
    pub bg_titlebar: u32,
    pub border_window: u32,
    pub bg_input: u32,
    pub border_input: u32,
    pub separator: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub accent_active: u32,
    pub btn_close: u32,
    pub btn_min: u32,
    pub btn_max: u32,
    pub scrollbar_track: u32,
    pub scrollbar_thumb: u32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            scale: 1.5,
            bg_desktop: 0xFF0F172A,
            bg_menubar: 0xFF18181B,
            border_menubar: 0xFF27272A,
            bg_dock: 0xFF1E293B,
            border_dock: 0xFF334155,
            bg_window: 0xFF0F172A,
            bg_titlebar: 0xFF1E293B,
            border_window: 0xFF334155,
            bg_input: 0xFF141E2E,
            border_input: 0xFF2A3B53,
            separator: 0xFF27272A,
            text_primary: 0xFFF8FAFC,
            text_secondary: 0xFFCBD5E1,
            text_muted: 0xFF64748B,
            accent: 0xFF0284C7,
            accent_hover: 0xFF38BDF8,
            accent_active: 0xFF0369A1,
            btn_close: 0xFFFF5F56,
            btn_min: 0xFFFFBD2E,
            btn_max: 0xFF27C93F,
            scrollbar_track: 0xFF141E2E,
            scrollbar_thumb: 0xFF475569,
        }
    }
}

impl Theme {
    #[inline(always)]
    pub fn pt(&self, val: f32) -> usize {
        (val * self.scale).round() as usize
    }

    pub fn font_title(&self) -> usize { self.pt(16.0) }
    pub fn font_body(&self) -> usize { self.pt(13.0) }
    pub fn font_caption(&self) -> usize { self.pt(11.0) }
    pub fn font_large(&self) -> usize { self.pt(20.0) }
}

pub static mut CURRENT_THEME: Theme = Theme {
    scale: 1.5,
    bg_desktop: 0xFF0F172A,
    bg_menubar: 0xFF18181B,
    border_menubar: 0xFF27272A,
    bg_dock: 0xFF1E293B,
    border_dock: 0xFF334155,
    bg_window: 0xFF0F172A,
    bg_titlebar: 0xFF1E293B,
    border_window: 0xFF334155,
    bg_input: 0xFF141E2E,
    border_input: 0xFF2A3B53,
    separator: 0xFF27272A,
    text_primary: 0xFFF8FAFC,
    text_secondary: 0xFFCBD5E1,
    text_muted: 0xFF64748B,
    accent: 0xFF0284C7,
    accent_hover: 0xFF38BDF8,
    accent_active: 0xFF0369A1,
    btn_close: 0xFFFF5F56,
    btn_min: 0xFFFFBD2E,
    btn_max: 0xFF27C93F,
    scrollbar_track: 0xFF141E2E,
    scrollbar_thumb: 0xFF475569,
};

pub fn get_theme() -> Theme {
    unsafe { CURRENT_THEME }
}

pub fn set_theme_scale(scale: f32) {
    unsafe { CURRENT_THEME.scale = scale.clamp(1.0, 3.0); }
}
