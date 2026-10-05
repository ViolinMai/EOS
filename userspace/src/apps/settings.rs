use crate::framework::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Display,
    Network,
    About,
}

pub struct SettingsApp {
    bounds: Rect,
    current_tab: SettingsTab,
    dark_mode_active: bool,
    ui_scale_val: usize,
}

impl SettingsApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            current_tab: SettingsTab::General,
            dark_mode_active: false,
            ui_scale_val: 2,
        }
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

        let sidebar_w = theme.pt(160.0) as i32;
        canvas.draw_rect(self.bounds.x, self.bounds.y, sidebar_w, self.bounds.h, theme.bg_titlebar, 0);
        canvas.draw_line_v(self.bounds.x + sidebar_w, self.bounds.y, self.bounds.h, theme.border_window);

        let tabs = [
            (SettingsTab::General, "General"),
            (SettingsTab::Display, "Display"),
            (SettingsTab::Network, "Network"),
            (SettingsTab::About, "About EOS"),
        ];

        let tab_h = theme.pt(32.0) as i32;
        for (i, (tab, name)) in tabs.iter().enumerate() {
            let ty = self.bounds.y + (theme.pt(16.0) as i32) + (i as i32 * (tab_h + (theme.pt(4.0) as i32)));
            let is_sel = self.current_tab == *tab;

            if is_sel {
                canvas.draw_rect(self.bounds.x + (theme.pt(8.0) as i32), ty, (sidebar_w - (theme.pt(16.0) as i32)).max(0), tab_h, theme.accent, theme.pt(6.0) as usize);
                canvas.draw_text(self.bounds.x + (theme.pt(20.0) as i32), ty + (theme.pt(7.0) as i32), name, 0xFFFFFFFF, theme.font_body());
            } else {
                canvas.draw_text(self.bounds.x + (theme.pt(20.0) as i32), ty + (theme.pt(7.0) as i32), name, theme.text_primary, theme.font_body());
            }
        }

        let content_x = self.bounds.x + sidebar_w + (theme.pt(24.0) as i32);
        let content_y = self.bounds.y + (theme.pt(24.0) as i32);

        match self.current_tab {
            SettingsTab::General => {
                canvas.draw_text(content_x, content_y, "System Preferences", theme.text_primary, theme.font_large());
                canvas.draw_text(content_x, content_y + (theme.pt(40.0) as i32), "Appearance & Interface Options", theme.text_secondary, theme.font_body());

                let btn_y = content_y + (theme.pt(80.0) as i32);
                canvas.draw_rect(content_x, btn_y, theme.pt(160.0) as i32, theme.pt(32.0) as i32, theme.accent, theme.pt(6.0) as usize);
                let mode_str = if self.dark_mode_active { "Theme: Dark" } else { "Theme: Light" };
                canvas.draw_text(content_x + (theme.pt(16.0) as i32), btn_y + (theme.pt(7.0) as i32), mode_str, 0xFFFFFFFF, theme.font_body());
            }
            SettingsTab::Display => {
                canvas.draw_text(content_x, content_y, "Display & Typography", theme.text_primary, theme.font_large());
                canvas.draw_text(content_x, content_y + (theme.pt(40.0) as i32), "UI Scaling Factor:", theme.text_secondary, theme.font_body());

                for s in 1..=3 {
                    let bx = content_x + ((s as i32 - 1) * (theme.pt(60.0) as i32));
                    let by = content_y + (theme.pt(70.0) as i32);
                    let (bg, fg) = if self.ui_scale_val == s {
                        (theme.accent, 0xFFFFFFFF)
                    } else {
                        (theme.bg_titlebar, theme.text_primary)
                    };
                    canvas.draw_rect(bx, by, theme.pt(50.0) as i32, theme.pt(28.0) as i32, bg, theme.pt(6.0) as usize);
                    canvas.draw_rect_outline(bx, by, theme.pt(50.0) as i32, theme.pt(28.0) as i32, theme.border_window, theme.pt(6.0) as usize);
                    canvas.draw_text(bx + (theme.pt(16.0) as i32), by + (theme.pt(5.0) as i32), &format!("{}x", s), fg, theme.font_body());
                }
            }
            SettingsTab::Network => {
                canvas.draw_text(content_x, content_y, "Network Adapter", theme.text_primary, theme.font_large());
                canvas.draw_text(content_x, content_y + (theme.pt(40.0) as i32), "Interface: Intel E1000 Fast Ethernet", theme.text_secondary, theme.font_body());
                canvas.draw_text(content_x, content_y + (theme.pt(70.0) as i32), "IP Address: 10.0.2.15 (DHCP / Static)", theme.text_secondary, theme.font_body());
                canvas.draw_text(content_x, content_y + (theme.pt(100.0) as i32), "Gateway: 10.0.2.2 | DNS: 10.0.2.3", theme.text_secondary, theme.font_body());
            }
            SettingsTab::About => {
                canvas.draw_text(content_x, content_y, "EOS Operating System", theme.text_primary, theme.font_large());
                canvas.draw_text(content_x, content_y + (theme.pt(36.0) as i32), "Kernel: x86_64 SMP Monolithic + Ring 3 UI", theme.text_secondary, theme.font_body());
                canvas.draw_text(content_x, content_y + (theme.pt(66.0) as i32), "Architecture: 8 Hardware Symmetrical Cores", theme.text_secondary, theme.font_body());
                canvas.draw_text(content_x, content_y + (theme.pt(96.0) as i32), "Compositor: Userspace Musl Frame Engine", theme.text_secondary, theme.font_body());
            }
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let sidebar_w = theme.pt(160.0) as i32;

        if pressed && mx <= self.bounds.x + sidebar_w {
            let tab_h = theme.pt(32.0) as i32;
            let rel_y = (my - (self.bounds.y + (theme.pt(16.0) as i32))).max(0);
            let idx = rel_y / (tab_h + (theme.pt(4.0) as i32));
            match idx {
                0 => self.current_tab = SettingsTab::General,
                1 => self.current_tab = SettingsTab::Display,
                2 => self.current_tab = SettingsTab::Network,
                3 => self.current_tab = SettingsTab::About,
                _ => {}
            }
            return true;
        }

        if pressed && self.current_tab == SettingsTab::Display {
            let content_x = self.bounds.x + sidebar_w + (theme.pt(24.0) as i32);
            let content_y = self.bounds.y + (theme.pt(24.0) as i32);
            let by = content_y + (theme.pt(70.0) as i32);
            if my >= by && my <= by + (theme.pt(28.0) as i32) {
                for s in 1..=3 {
                    let bx = content_x + ((s as i32 - 1) * (theme.pt(60.0) as i32));
                    if mx >= bx && mx <= bx + (theme.pt(50.0) as i32) {
                        self.ui_scale_val = s;
                        set_theme_scale(s as f32);
                        return true;
                    }
                }
            }
        }

        true
    }
}
