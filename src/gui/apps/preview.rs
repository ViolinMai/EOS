use alloc::string::String;
use alloc::vec::Vec;
use crate::input::InputEvent;
use super::{App, AppAction};
use crate::gui::draw::{self, FrameBuffer};
use crate::gui::theme::Theme;

pub struct PreviewApp {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

impl PreviewApp {
    pub fn new(name: String, width: usize, height: usize, pixels: Vec<u32>) -> Self {
        Self { name, width, height, pixels }
    }
}

impl App for PreviewApp {
    fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        draw::draw_rect(fb, x, y, w, h, 0xFF0F172A);

        if self.width == 0 || self.height == 0 || self.pixels.is_empty() {
            draw::draw_text(fb, x + w / 2 - 50, y + h / 2, "No Image Loaded", theme.text_dim, 1);
            return;
        }

        let scale_w = (self.width + w - 1) / w;
        let scale_h = (self.height + h - 1) / h;
        let step = core::cmp::max(1, core::cmp::max(scale_w, scale_h));

        let disp_w = (self.width / step).min(w);
        let disp_h = (self.height / step).min(h);
        let off_x = x + (w - disp_w) / 2;
        let off_y = y + (h - disp_h) / 2;

        for dy in 0..disp_h {
            let src_y = dy * step;
            let dst_y = off_y + dy;
            if dst_y >= fb.height { break; }
            for dx in 0..disp_w {
                let src_x = dx * step;
                let dst_x = off_x + dx;
                if dst_x >= fb.width { break; }
                let color = self.pixels[src_y * self.width + src_x];
                fb.pixels[dst_y * fb.pitch_pixels + dst_x] = color;
            }
        }

        let bar_h = 26usize;
        let bar_y = y + h.saturating_sub(bar_h);
        draw::draw_rect_alpha(fb, x, bar_y, w, bar_h, 0xFF1E293B, 220);
        let info = alloc::format!("{} ({}x{} px | Fit: {}x)", self.name, self.width, self.height, step);
        draw::draw_text(fb, x + 12, bar_y + 5, &info, 0xFFF8FAFC, 1);
    }

    fn on_event(&mut self, _event: &InputEvent, _mx: isize, _my: isize) {}
    fn on_resize(&mut self, _w: usize, _h: usize) {}
    fn title(&self) -> &str { "Preview" }
    fn icon(&self) -> &'static str { "IMG" }
    fn poll_action(&mut self) -> Option<AppAction> { None }
}
