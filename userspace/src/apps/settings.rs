use crate::framework::*;
use std::fs;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Appearance,
    Display,
    About,
}

pub struct SettingsApp {
    bounds: Rect,
    current_tab: SettingsTab,
    available_wallpapers: Vec<String>,
    selected_wp: Option<usize>,
    status: String,
}

impl SettingsApp {
    pub fn new() -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            current_tab: SettingsTab::General,
            available_wallpapers: Vec::new(),
            selected_wp: None,
            status: String::new(),
        };
        app.scan_wallpapers();
        app
    }

    fn scan_wallpapers(&mut self) {
        self.available_wallpapers.clear();
        if let Ok(entries) = fs::read_dir("/EOS SHARE") {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let lower = name.to_lowercase();
                if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                    self.available_wallpapers.push(name);
                }
            }
        }
    }

    fn apply_wallpaper(&mut self, idx: usize) {
        if idx >= self.available_wallpapers.len() { return; }
        let fname = &self.available_wallpapers[idx];
        let path = format!("/EOS SHARE/{}", fname);

        if let Ok(data) = fs::read(&path) {
            if data.len() >= 8 && &data[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
                if let Ok((px, w, h)) = crate::png::decode_png(&data) {
                    unsafe { crate::framework::theme::DESKTOP_WALLPAPER = Some((px, w, h)); }
                    self.status = format!("Wallpaper applied: {}", fname);
                    self.selected_wp = Some(idx);
                    return;
                }
            }
            if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
                let mut dec = zune_jpeg::JpegDecoder::new(&data);
                if let Ok(raw) = dec.decode() {
                    if let Some(info) = dec.info() {
                        let w = info.width as usize;
                        let h = info.height as usize;
                        let mut px = vec![0u32; w * h];
                        for i in 0..(w * h) {
                            let o = i * 3;
                            if o + 2 < raw.len() {
                                px[i] = (0xFF << 24) | ((raw[o] as u32) << 16) | ((raw[o+1] as u32) << 8) | (raw[o+2] as u32);
                            }
                        }
                        unsafe { crate::framework::theme::DESKTOP_WALLPAPER = Some((px, w, h)); }
                        self.status = format!("Wallpaper applied: {}", fname);
                        self.selected_wp = Some(idx);
                        return;
                    }
                }
            }
        }
        self.status = format!("Failed to apply: {}", fname);
    }
}

impl Widget for SettingsApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_window, 0);

        let sidebar_w = theme.pt(160.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, sidebar_w, self.bounds.h, theme.bg_titlebar, 0);
        canvas.draw_line_v(self.bounds.x + sidebar_w, self.bounds.y, self.bounds.h, theme.border_window);

        let tabs = [
            (SettingsTab::General, "General"),
            (SettingsTab::Appearance, "Wallpaper"),
            (SettingsTab::Display, "Display & Scale"),
            (SettingsTab::About, "About EOS"),
        ];

        let tab_h = theme.pt(32.0);
        for (i, (tab, name)) in tabs.iter().enumerate() {
            let ty = self.bounds.y + theme.pt(16.0) + (i as i32 * (tab_h + theme.pt(4.0)));
            let is_sel = self.current_tab == *tab;

            if is_sel {
                canvas.draw_rect(self.bounds.x + theme.pt(8.0), ty, sidebar_w - theme.pt(16.0), tab_h, theme.accent, theme.pt(6.0) as usize);
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ty + theme.pt(7.0), name, 0xFFFFFFFF, theme.font_body());
            } else {
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ty + theme.pt(7.0), name, theme.text_primary, theme.font_body());
            }
        }

        let content_x = self.bounds.x + sidebar_w + theme.pt(24.0);
        let content_y = self.bounds.y + theme.pt(24.0);

        match self.current_tab {
            SettingsTab::General => {
                canvas.draw_text(content_x, content_y, "System Preferences", theme.text_primary, theme.font_large());
                canvas.draw_text(content_x, content_y + theme.pt(36.0), "EOS 64-bit Musl Desktop on Symmetrical Cores", theme.text_secondary, theme.font_body());
            }
            SettingsTab::Appearance => {
                canvas.draw_text(content_x, content_y, "Desktop Wallpaper", theme.text_primary, theme.font_large());
                canvas.draw_text(content_x, content_y + theme.pt(32.0), "Choose an image from /EOS SHARE:", theme.text_secondary, theme.font_body());

                let mut item_y = content_y + theme.pt(60.0);
                for (idx, wp) in self.available_wallpapers.iter().enumerate() {
                    let is_sel = self.selected_wp == Some(idx);
                    let row_w = self.bounds.w - sidebar_w - theme.pt(48.0);
                    let bg = if is_sel { theme.accent } else { 0xFF141E2E };
                    canvas.draw_rect(content_x, item_y, row_w, theme.pt(32.0), bg, theme.pt(5.0) as usize);
                    let txt_c = if is_sel { 0xFFFFFFFF } else { theme.text_primary };
                    canvas.draw_text_clipped(content_x + theme.pt(12.0), item_y + theme.pt(7.0), row_w - theme.pt(24.0), &format!("🖼️  {}", wp), txt_c, theme.font_body());
                    item_y += theme.pt(36.0);
                }

                if !self.status.is_empty() {
                    canvas.draw_text(content_x, item_y + theme.pt(10.0), &self.status, theme.accent, theme.font_body());
                }
            }
            SettingsTab::Display => {
                canvas.draw_text(content_x, content_y, "Interface Scale", theme.text_primary, theme.font_large());
                canvas.draw_text(content_x, content_y + theme.pt(36.0), "Select typography & UI scaling factor (Live):", theme.text_secondary, theme.font_body());

                for s in 1..=3 {
                    let bx = content_x + ((s as i32 - 1) * theme.pt(70.0));
                    let by = content_y + theme.pt(70.0);
                    let cur_scale = get_theme().scale.round() as usize;
                    let (bg, fg) = if cur_scale == s {
                        (theme.accent, 0xFFFFFFFF)
                    } else {
                        (theme.bg_titlebar, theme.text_primary)
                    };
                    canvas.draw_rect(bx, by, theme.pt(60.0), theme.pt(32.0), bg, theme.pt(6.0) as usize);
                    canvas.draw_rect_outline(bx, by, theme.pt(60.0), theme.pt(32.0), theme.border_window, theme.pt(6.0) as usize);
                    canvas.draw_text(bx + theme.pt(20.0), by + theme.pt(7.0), &format!("{}x", s), fg, theme.font_body());
                }
            }
            SettingsTab::About => {
                canvas.draw_text(content_x, content_y, "About EOS", theme.text_primary, theme.font_large());
                canvas.draw_text(content_x, content_y + theme.pt(36.0), "Kernel: x86_64 SMP Monolithic + Ring 3 UI", theme.text_secondary, theme.font_body());
                canvas.draw_text(content_x, content_y + theme.pt(66.0), "Hardware: 8 Cores Online & Active", theme.text_secondary, theme.font_body());
            }
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let sidebar_w = theme.pt(160.0);

        if pressed && mx <= self.bounds.x + sidebar_w {
            let tab_h = theme.pt(32.0);
            let rel_y = (my - (self.bounds.y + theme.pt(16.0))).max(0);
            let idx = rel_y / (tab_h + theme.pt(4.0));
            match idx {
                0 => self.current_tab = SettingsTab::General,
                1 => { self.current_tab = SettingsTab::Appearance; self.scan_wallpapers(); }
                2 => self.current_tab = SettingsTab::Display,
                3 => self.current_tab = SettingsTab::About,
                _ => {}
            }
            return true;
        }

        if pressed && self.current_tab == SettingsTab::Appearance {
            let content_x = self.bounds.x + sidebar_w + theme.pt(24.0);
            let content_y = self.bounds.y + theme.pt(24.0);
            let item_start_y = content_y + theme.pt(60.0);
            let row_w = self.bounds.w - sidebar_w - theme.pt(48.0);

            if mx >= content_x && mx <= content_x + row_w && my >= item_start_y {
                let idx = ((my - item_start_y) / theme.pt(36.0)) as usize;
                if idx < self.available_wallpapers.len() {
                    self.apply_wallpaper(idx);
                    return true;
                }
            }
        }

        if pressed && self.current_tab == SettingsTab::Display {
            let content_x = self.bounds.x + sidebar_w + theme.pt(24.0);
            let content_y = self.bounds.y + theme.pt(24.0);
            let by = content_y + theme.pt(70.0);
            if my >= by && my <= by + theme.pt(32.0) {
                for s in 1..=3 {
                    let bx = content_x + ((s as i32 - 1) * theme.pt(70.0));
                    if mx >= bx && mx <= bx + theme.pt(60.0) {
                        set_theme_scale(s as f32);
                        return true;
                    }
                }
            }
        }

        true
    }
}
