use crate::framework::canvas::Canvas;
use crate::framework::widget::Rect;
use crate::framework::theme::get_theme;
use std::fs;

pub struct ColorPickerTool {
    pub bounds: Rect,
    pub selected_color: u32,
    pub active_hue: f32,
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

        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF18181B, theme.pt(8.0) as usize);
        canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.border_window, theme.pt(8.0) as usize);

        let sv_x = self.bounds.x + pad;
        let sv_y = self.bounds.y + pad;
        let step_x = (matrix_w / 20).max(1);
        let step_y = (matrix_h / 12).max(1);

        for sy in 0..12 {
            for sx in 0..20 {
                let col = hsv_to_rgb(self.active_hue, (sx as f32) / 20.0, 1.0 - ((sy as f32) / 12.0));
                canvas.draw_rect(sv_x + (sx * step_x), sv_y + (sy * step_y), step_x, step_y, col, 0);
            }
        }

        let bar_y = sv_y + matrix_h + pad;
        let hue_step = (matrix_w / 24).max(1);
        for i in 0..24 {
            let h_val = (i as f32 / 24.0) * 360.0;
            let col = hsv_to_rgb(h_val, 1.0, 1.0);
            canvas.draw_rect(sv_x + (i * hue_step), bar_y, hue_step, bar_h, col, 0);
        }
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) {
            if !pressed {
                self.is_dragging_hue = false;
                self.is_dragging_sv = false;
            }
            return false;
        }

        let theme = get_theme();
        let pad = theme.pt(8.0);
        let bar_h = theme.pt(16.0);
        let matrix_h = self.bounds.h - bar_h - (pad * 3);
        let matrix_w = self.bounds.w - (pad * 2);

        let sv_x = self.bounds.x + pad;
        let sv_y = self.bounds.y + pad;
        let bar_y = sv_y + matrix_h + pad;

        if pressed {
            if mx >= sv_x && mx <= sv_x + matrix_w && my >= bar_y && my <= bar_y + bar_h {
                self.is_dragging_hue = true;
            } else if mx >= sv_x && mx <= sv_x + matrix_w && my >= sv_y && my <= sv_y + matrix_h {
                self.is_dragging_sv = true;
            }
        } else {
            self.is_dragging_hue = false;
            self.is_dragging_sv = false;
        }

        if self.is_dragging_hue {
            let rel_x = (mx - sv_x).clamp(0, matrix_w) as f32;
            self.active_hue = (rel_x / matrix_w as f32) * 360.0;
            self.selected_color = hsv_to_rgb(self.active_hue, 1.0, 1.0);
            return true;
        }

        if self.is_dragging_sv {
            let s = (mx - sv_x).clamp(0, matrix_w) as f32 / matrix_w as f32;
            let v = 1.0 - ((my - sv_y).clamp(0, matrix_h) as f32 / matrix_h as f32);
            self.selected_color = hsv_to_rgb(self.active_hue, s, v);
            return true;
        }

        false
    }
}

pub struct FileDialog {
    pub bounds: Rect,
    pub is_save_mode: bool,
    pub current_dir: String,
    pub selected_file: String,
    pub input_filename: String,
    pub is_open: bool,
    pub entries: Vec<String>,
}

impl FileDialog {
    pub fn new(is_save_mode: bool) -> Self {
        let mut d = Self {
            bounds: Rect::new(400, 250, 520, 360),
            is_save_mode,
            current_dir: String::from("EOS SHARE"),
            selected_file: String::new(),
            input_filename: if is_save_mode { String::from("document.txt") } else { String::new() },
            is_open: true,
            entries: Vec::new(),
        };
        d.refresh_entries();
        d
    }

    pub fn refresh_entries(&mut self) {
        self.entries.clear();
        if let Ok(rd) = fs::read_dir(&self.current_dir) {
            for e in rd.flatten() {
                self.entries.push(e.file_name().to_string_lossy().to_string());
            }
        }
    }

    pub fn draw(&self, canvas: &mut Canvas) {
        if !self.is_open { return; }
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF18181B, theme.pt(8.0) as usize);
        canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.border_window, theme.pt(8.0) as usize);

        let title = if self.is_save_mode { "Save File Dialog" } else { "Open File Dialog" };
        canvas.draw_text(self.bounds.x + 16, self.bounds.y + 12, title, 0xFFFFFFFF, theme.font_body());

        let list_y = self.bounds.y + 44;
        let list_h = self.bounds.h - 100;
        canvas.draw_rect(self.bounds.x + 16, list_y, self.bounds.w - 32, list_h, 0xFF27272A, 4);

        for (i, entry) in self.entries.iter().take(8).enumerate() {
            let row_y = list_y + 8 + (i as i32 * 24);
            canvas.draw_text(self.bounds.x + 28, row_y, entry, 0xFFF4F4F5, theme.font_caption());
        }

        let input_y = self.bounds.y + self.bounds.h - 44;
        canvas.draw_rect(self.bounds.x + 16, input_y, self.bounds.w - 140, 32, 0xFF27272A, 4);
        canvas.draw_text(self.bounds.x + 24, input_y + 8, &self.input_filename, 0xFFFFFFFF, theme.font_caption());

        let btn_x = self.bounds.x + self.bounds.w - 110;
        canvas.draw_rect(btn_x, input_y, 94, 32, theme.accent, 4);
        let btn_txt = if self.is_save_mode { "Save" } else { "Open" };
        canvas.draw_text(btn_x + 28, input_y + 8, btn_txt, 0xFFFFFFFF, theme.font_body());
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> Option<String> {
        if !self.is_open || !self.bounds.contains(mx, my) { return None; }
        if pressed {
            let btn_x = self.bounds.x + self.bounds.w - 110;
            let input_y = self.bounds.y + self.bounds.h - 44;
            if mx >= btn_x && mx <= btn_x + 94 && my >= input_y && my <= input_y + 32 {
                self.is_open = false;
                let path = format!("{}/{}", self.current_dir, self.input_filename);
                return Some(path);
            }
        }
        None
    }
}

pub struct ContextMenu {
    pub bounds: Rect,
    pub items: Vec<String>,
    pub is_visible: bool,
    pub target_item: Option<String>,
}

impl ContextMenu {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            items: Vec::new(),
            is_visible: false,
            target_item: None,
        }
    }

    pub fn show(&mut self, x: i32, y: i32, items: Vec<String>, target: Option<String>) {
        self.bounds = Rect::new(x, y, 160, (items.len() as i32 * 28) + 8);
        self.items = items;
        self.is_visible = true;
        self.target_item = target;
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
        self.target_item = None;
    }

    pub fn draw(&self, canvas: &mut Canvas) {
        if !self.is_visible { return; }
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF18181B, 6);
        canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.border_window, 6);

        for (i, item) in self.items.iter().enumerate() {
            let ry = self.bounds.y + 4 + (i as i32 * 28);
            canvas.draw_text(self.bounds.x + 14, ry + 6, item, 0xFFF4F4F5, theme.font_caption());
        }
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> Option<String> {
        if !self.is_visible { return None; }
        if pressed {
            if self.bounds.contains(mx, my) {
                let idx = ((my - self.bounds.y - 4) / 28) as usize;
                if idx < self.items.len() {
                    let chosen = self.items[idx].clone();
                    self.hide();
                    return Some(chosen);
                }
            } else {
                self.hide();
            }
        }
        None
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let ri = ((r + m) * 255.0) as u32;
    let gi = ((g + m) * 255.0) as u32;
    let bi = ((b + m) * 255.0) as u32;
    0xFF000000 | (ri << 16) | (gi << 8) | bi
}
