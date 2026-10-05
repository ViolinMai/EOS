use crate::framework::*;

pub struct ActivityMonitorApp {
    bounds: Rect,
    selected_tab: usize,
}

impl ActivityMonitorApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            selected_tab: 0,
        }
    }
}

impl Widget for ActivityMonitorApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_window, 0);

        let tb_h = theme.pt(44.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);

        let tabs = ["CPU", "Memory", "Disk", "Network"];
        let seg_w = theme.pt(80.0);
        let start_x = self.bounds.x + (self.bounds.w - (tabs.len() as i32 * seg_w)) / 2;

        for (i, &t) in tabs.iter().enumerate() {
            let bx = start_x + (i as i32 * seg_w);
            let by = self.bounds.y + theme.pt(8.0);
            if self.selected_tab == i {
                canvas.draw_rect(bx, by, seg_w, theme.pt(28.0), theme.accent, theme.pt(6.0) as usize);
                let (tw, _) = canvas.measure_text(t, theme.font_body());
                canvas.draw_text(bx + (seg_w - tw as i32) / 2, by + theme.pt(5.0), t, 0xFFFFFFFF, theme.font_body());
            } else {
                canvas.draw_rect_outline(bx, by, seg_w, theme.pt(28.0), theme.border_window, theme.pt(6.0) as usize);
                let (tw, _) = canvas.measure_text(t, theme.font_body());
                canvas.draw_text(bx + (seg_w - tw as i32) / 2, by + theme.pt(5.0), t, theme.text_secondary, theme.font_body());
            }
        }

        let content_y = self.bounds.y + tb_h + theme.pt(16.0);
        let pad_x = self.bounds.x + theme.pt(24.0);

        match self.selected_tab {
            0 => {
                let card_h = theme.pt(120.0);
                let card_w = self.bounds.w - theme.pt(48.0);
                canvas.draw_rect(pad_x, content_y, card_w, card_h, theme.bg_titlebar, theme.pt(10.0) as usize);
                canvas.draw_rect_outline(pad_x, content_y, card_w, card_h, theme.border_window, theme.pt(10.0) as usize);

                canvas.draw_text(pad_x + theme.pt(20.0), content_y + theme.pt(16.0), "Processor: 8 Symmetrical Cores (x86_64 SMP)", theme.text_primary, theme.font_title());
                canvas.draw_text(pad_x + theme.pt(20.0), content_y + theme.pt(48.0), "System Load: 14% | User Space: 8% | Idle: 78%", theme.accent_hover, theme.font_body());

                let bar_w = (card_w - theme.pt(40.0)) / 8;
                for c in 0..8 {
                    let bx = pad_x + theme.pt(20.0) + (c * bar_w);
                    let by = content_y + theme.pt(80.0);
                    canvas.draw_rect(bx, by, bar_w - theme.pt(6.0), theme.pt(24.0), 0xFF141E2E, theme.pt(4.0) as usize);
                    let load_fill = ((c * 11 + 25) % 85) * (bar_w - theme.pt(6.0)) / 100;
                    canvas.draw_rect(bx, by, load_fill, theme.pt(24.0), theme.accent, theme.pt(4.0) as usize);
                }
            }
            _ => {
                canvas.draw_text(pad_x, content_y, "System Monitor Active", theme.text_primary, theme.font_large());
            }
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let tb_h = theme.pt(44.0);

        if pressed && my <= self.bounds.y + tb_h {
            let seg_w = theme.pt(80.0);
            let start_x = self.bounds.x + (self.bounds.w - (4 * seg_w)) / 2;
            if mx >= start_x && mx <= start_x + (4 * seg_w) {
                self.selected_tab = ((mx - start_x) / seg_w) as usize;
                return true;
            }
        }
        true
    }
}
