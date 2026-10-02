
#[derive(Clone, Copy)]
pub struct Theme {
    pub desktop_bg: u32, pub menubar_bg: u32, pub menubar_border: u32,
    pub dock_bg: u32, pub dock_border: u32, pub window_bg: u32, pub window_border: u32,
    pub titlebar_bg: u32, pub titlebar_border: u32, pub separator: u32,
    pub text_main: u32, pub text_dim: u32, pub accent: u32, pub accent_hover: u32,
    pub border: u32, pub btn_close: u32, pub btn_min: u32, pub btn_max: u32,
}

pub const LIGHT_THEME: Theme = Theme {
    desktop_bg: 0xFF1E293B, menubar_bg: 0xFFF8FAFC, menubar_border: 0xFFCBD5E1,
    dock_bg: 0xFFFFFFFF, dock_border: 0xFFCBD5E1, window_bg: 0xFFFFFFFF, window_border: 0xFF94A3B8,
    titlebar_bg: 0xFFF1F5F9, titlebar_border: 0xFFE2E8F0, separator: 0xFFE2E8F0,
    text_main: 0xFF0F172A, text_dim: 0xFF64748B, accent: 0xFF0284C7, accent_hover: 0xFF0369A1,
    border: 0xFFCBD5E1, btn_close: 0xFFFF5F56, btn_min: 0xFFFFBD2E, btn_max: 0xFF27C93F,
};

pub const DARK_THEME: Theme = Theme {
    desktop_bg: 0xFF0F172A, menubar_bg: 0xFF18181B, menubar_border: 0xFF27272A,
    dock_bg: 0xFF1E293B, dock_border: 0xFF27272A, window_bg: 0xFF18181B, window_border: 0xFF3F3F46,
    titlebar_bg: 0xFF27272A, titlebar_border: 0xFF3F3F46, separator: 0xFF27272A,
    text_main: 0xFFF8FAFC, text_dim: 0xFF94A3B8, accent: 0xFF38BDF8, accent_hover: 0xFF0284C7,
    border: 0xFF3F3F46, btn_close: 0xFFFF5F56, btn_min: 0xFFFFBD2E, btn_max: 0xFF27C93F,
};

pub fn get_current_theme() -> &'static Theme {
    if crate::config::CONFIG.is_dark_mode() { &DARK_THEME } else { &LIGHT_THEME }
}
