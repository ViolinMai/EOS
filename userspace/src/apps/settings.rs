use crate::framework::*;

pub struct SettingsApp {
    bounds: Rect,
    selected_tab: usize,
}

impl SettingsApp {
    pub fn new() -> Self {
        Self { bounds: Rect::default(), selected_tab: 0 }
    }
}

impl Widget for SettingsApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let sb_w = theme.pt(160.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, sb_w, self.bounds.h, theme.bg_titlebar, 0);
        canvas.draw_line_v(self.bounds.x + sb_w, self.bounds.y, self.bounds.h, theme.border_window);

        let tabs = ["Appearance", "Display", "Hardware & SMP"];
        for (i, t) in tabs.iter().enumerate() {
            let ry = self.bounds.y + theme.pt(24.0) + (i * theme.pt(38.0));
            if self.selected_tab == i {
                canvas.draw_rect(self.bounds.x + theme.pt(10.0), ry, sb_w - theme.pt(20.0), theme.pt(32.0), theme.accent, theme.pt(8.0));
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ry + theme.pt(6.0), t, 0xFFFFFFFF, theme.font_body());
            } else {
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ry + theme.pt(6.0), t, theme.text_secondary, theme.font_body());
            }
        }

        let content_x = self.bounds.x + sb_w + theme.pt(24.0);
        let content_y = self.bounds.y + theme.pt(24.0);
        canvas.draw_rect(self.bounds.x + sb_w + 1, self.bounds.y, self.bounds.w.saturating_sub(sb_w + 1), self.bounds.h, theme.bg_window, 0);

        match self.selected_tab {
            0 => {
                canvas.draw_text(content_x, content_y, "Appearance & Interface", theme.text_primary, theme.font_title());
                canvas.draw_text(content_x, content_y + theme.pt(36.0), "UI Scale Factor: 1.5x Dynamic HiDPI [Active]", theme.text_secondary, theme.font_body());
                canvas.draw_rect(content_x, content_y + theme.pt(68.0), theme.pt(150.0), theme.pt(36.0), theme.accent, theme.pt(8.0));
                canvas.draw_text(content_x + theme.pt(20.0), content_y + theme.pt(76.0), "Dark Mode [ON]", 0xFFFFFFFF, theme.font_body());
            }
            1 => {
                canvas.draw_text(content_x, content_y, "Display Preferences", theme.text_primary, theme.font_title());
                canvas.draw_text(content_x, content_y + theme.pt(36.0), "Resolution: 1920x1080 @ 60 FPS", theme.text_secondary, theme.font_body());
                canvas.draw_text(content_x, content_y + theme.pt(66.0), "Frame Buffer: 32-bit TrueColor ARGB8888", theme.accent_hover, theme.font_body());
            }
            2 => {
                canvas.draw_text(content_x, content_y, "System & Hardware Overview", theme.text_primary, theme.font_title());
                canvas.draw_text(content_x, content_y + theme.pt(36.0), "Architecture: x86_64 SMP (8 Cores)", 0xFF22C55E, theme.font_body());
                canvas.draw_text(content_x, content_y + theme.pt(66.0), "Privilege: Ring-3 Musl Userspace", theme.text_secondary, theme.font_body());
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let sb_w = theme.pt(160.0);
        if pressed && mx < self.bounds.x + sb_w {
            let idx = (my.saturating_sub(self.bounds.y + theme.pt(24.0))) / theme.pt(38.0);
            if idx < 3 {
                self.selected_tab = idx;
                return true;
            }
        }
        true
    }
}
