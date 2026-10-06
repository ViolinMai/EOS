use crate::framework::canvas::Canvas;
use crate::framework::widget::Rect;
use crate::framework::theme::get_theme;

pub struct ColorPickerTool {
    pub bounds: Rect,
    pub selected_color: u32,
    pub active_hue: f32, // 0.0 .. 360.0
    is_dragging_hue: bool,
    is_dragging_sv: bool,
}

impl ColorPickerTool {
    pub fn new(x: i32, y: i32, w: i32, h: i32, initial_color: u32) -> Self {
        Self {
            bounds: Rect::new(x, y, w, h),
            selected_color: initial_color,
            active_hue: 210.0,
            is_dragging_hue: false,
            is_dragging_sv: false,
        }
    }

    pub fn draw(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let pad = theme.pt(8.0);
        let bar_h = theme.pt(16.0);
        let matrix_h = self.bounds.h - bar_h - (pad * 3);
        let matrix_w = self.bounds.w - (pad * 2);

        // خلفية الأداة
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF1E293B, theme.pt(8.0) as usize);
        canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.border_window, theme.pt(8.0) as usize);

        // 1. مصفوفة تدرج التشبع والإضاءة (Saturation / Value Matrix)
        let sv_x = self.bounds.x + pad;
        let sv_y = self.bounds.y + pad;
        let step_x = (matrix_w / 20).max(1);
        let step_y = (matrix_h / 12).max(1);

        for sy in 0..12 {
            let val = 1.0 - (sy as f32 / 11.0);
            for sx in 0..20 {
                let sat = sx as f32 / 19.0;
                let c = hsv_to_rgb(self.active_hue, sat, val);
                let bx = sv_x + (sx * step_x);
                let by = sv_y + (sy * step_y);
                canvas.draw_rect(bx, by, step_x, step_y, c, 0);
            }
        }
        canvas.draw_rect_outline(sv_x, sv_y, matrix_w, matrix_h, 0xFF475569, 0);

        // 2. شريط اختيار اللون الطيفي (Hue Slider)
        let hue_y = sv_y + matrix_h + pad;
        let seg_w = (matrix_w / 36).max(1);
        for i in 0..36 {
            let h_val = (i as f32) * 10.0;
            let c = hsv_to_rgb(h_val, 1.0, 1.0);
            canvas.draw_rect(sv_x + (i * seg_w), hue_y, seg_w, bar_h, c, 0);
        }
        canvas.draw_rect_outline(sv_x, hue_y, matrix_w, bar_h, 0xFF475569, 0);

        // 3. مؤشر اللون المحدد وعينة المعاينة
        let preview_w = theme.pt(28.0);
        let prev_x = self.bounds.x + self.bounds.w - preview_w - pad;
        let prev_y = self.bounds.y + pad;
        canvas.draw_rect(prev_x, prev_y, preview_w, preview_w, self.selected_color, theme.pt(6.0) as usize);
        canvas.draw_rect_outline(prev_x, prev_y, preview_w, preview_w, 0xFFFFFFFF, theme.pt(6.0) as usize);
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) {
            self.is_dragging_hue = false;
            self.is_dragging_sv = false;
            return false;
        }

        let theme = get_theme();
        let pad = theme.pt(8.0);
        let bar_h = theme.pt(16.0);
        let matrix_h = self.bounds.h - bar_h - (pad * 3);
        let matrix_w = self.bounds.w - (pad * 2);

        let sv_x = self.bounds.x + pad;
        let sv_y = self.bounds.y + pad;
        let hue_y = sv_y + matrix_h + pad;

        if pressed {
            // النقر على شريط Hue
            if mx >= sv_x && mx <= sv_x + matrix_w && my >= hue_y && my <= hue_y + bar_h {
                self.is_dragging_hue = true;
                let rel = (mx - sv_x) as f32 / matrix_w as f32;
                self.active_hue = (rel * 360.0).clamp(0.0, 360.0);
                self.selected_color = hsv_to_rgb(self.active_hue, 0.85, 0.95);
                return true;
            }

            // النقر على مصفوفة SV
            if mx >= sv_x && mx <= sv_x + matrix_w && my >= sv_y && my <= sv_y + matrix_h {
                self.is_dragging_sv = true;
                let sat = ((mx - sv_x) as f32 / matrix_w as f32).clamp(0.0, 1.0);
                let val = (1.0 - ((my - sv_y) as f32 / matrix_h as f32)).clamp(0.0, 1.0);
                self.selected_color = hsv_to_rgb(self.active_hue, sat, val);
                return true;
            }
        } else {
            self.is_dragging_hue = false;
            self.is_dragging_sv = false;
        }

        true
    }
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = v - c;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let r = ((r1 + m) * 255.0).round() as u32;
    let g = ((g1 + m) * 255.0).round() as u32;
    let b = ((b1 + m) * 255.0).round() as u32;

    0xFF000000 | (r << 16) | (g << 8) | b
}
