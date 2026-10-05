use crate::framework::*;
use std::fs;

pub struct PreviewApp {
    bounds: Rect,
    pub title: String,
    pub file_path: String,
    pub pixels: Vec<u32>,
    pub img_w: usize,
    pub img_h: usize,
    pub status: String,
}

impl PreviewApp {
    pub fn new(title: String, file_path: String) -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            title: title.clone(),
            file_path: file_path.clone(),
            pixels: Vec::new(),
            img_w: 0,
            img_h: 0,
            status: String::from("Loading image..."),
        };
        app.load_image();
        app
    }

    pub fn load_image(&mut self) {
        let attempts = [
            format!("/{}", self.file_path.trim_start_matches('/')),
            self.file_path.clone(),
        ];

        let mut data = Vec::new();
        for p in &attempts {
            if let Ok(bytes) = fs::read(p) {
                if !bytes.is_empty() {
                    data = bytes;
                    break;
                }
            }
        }

        if data.len() == 0 {
            self.status = format!("Failed to read file: {}", self.file_path);
            return;
        }

        let total_bytes = data.len();

        if data.len() >= 8 && &data[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
            match crate::png::decode_png(&data) {
                Ok((px, w, h)) => {
                    self.pixels = px;
                    self.img_w = w;
                    self.img_h = h;
                    self.status = format!("PNG ({}x{} px | {} KB)", w, h, total_bytes / 1024);
                    return;
                }
                Err(e) => {
                    self.status = format!("PNG Decode Error: {}", e);
                    return;
                }
            }
        }

        if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
            let mut decoder = zune_jpeg::JpegDecoder::new(&data);
            match decoder.decode() {
                Ok(raw) => {
                    if let Some(info) = decoder.info() {
                        let w = info.width as usize;
                        let h = info.height as usize;
                        let mut px = vec![0u32; w * h];
                        for i in 0..(w * h) {
                            let o = i * 3;
                            if o + 2 < raw.len() {
                                px[i] = (0xFF << 24) | ((raw[o] as u32) << 16) | ((raw[o+1] as u32) << 8) | (raw[o+2] as u32);
                            }
                        }
                        self.pixels = px;
                        self.img_w = w;
                        self.img_h = h;
                        self.status = format!("JPEG ({}x{} px | {} KB)", w, h, total_bytes / 1024);
                        return;
                    }
                }
                Err(e) => {
                    self.status = format!("JPEG Decode Error: {:?}", e);
                    return;
                }
            }
        }

        self.status = format!("Unsupported image format ({} bytes)", total_bytes);
    }
}

impl Widget for PreviewApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF0F172A, 0);

        let sb_h = theme.pt(28.0);
        let content_h = (self.bounds.h - sb_h).max(0);

        if self.img_w > 0 && self.img_h > 0 && !self.pixels.is_empty() {
            let avail_w = self.bounds.w.max(1) as usize;
            let avail_h = content_h.max(1) as usize;

            let step_w = (self.img_w + avail_w - 1) / avail_w;
            let step_h = (self.img_h + avail_h - 1) / avail_h;
            let step = step_w.max(step_h).max(1);

            let disp_w = (self.img_w / step).min(avail_w);
            let disp_h = (self.img_h / step).min(avail_h);

            let off_x = self.bounds.x + ((self.bounds.w - disp_w as i32) / 2);
            let off_y = self.bounds.y + ((content_h - disp_h as i32) / 2);

            for dy in 0..disp_h {
                let src_y = dy * step;
                let dst_y = off_y + dy as i32;
                if dst_y < self.bounds.y || dst_y >= self.bounds.y + content_h { continue; }

                for dx in 0..disp_w {
                    let src_x = dx * step;
                    let dst_x = off_x + dx as i32;
                    if dst_x < self.bounds.x || dst_x >= self.bounds.x + self.bounds.w { continue; }

                    let color = self.pixels[src_y * self.img_w + src_x];
                    let idx = (dst_y as usize) * canvas.width + (dst_x as usize);
                    if idx < canvas.buffer.len() {
                        canvas.buffer[idx] = color;
                    }
                }
            }
        } else {
            let (tw, th) = canvas.measure_text(&self.status, theme.font_body());
            let tx = self.bounds.x + ((self.bounds.w - tw as i32) / 2);
            let ty = self.bounds.y + ((content_h - th as i32) / 2);
            canvas.draw_text(tx, ty, &self.status, 0xFF94A3B8, theme.font_body());
        }

        let sb_y = self.bounds.y + self.bounds.h - sb_h;
        canvas.draw_rect(self.bounds.x, sb_y, self.bounds.w, sb_h, 0xFF1E293B, 0);
        canvas.draw_line_h(self.bounds.x, sb_y, self.bounds.w, 0xFF334155);
        canvas.draw_text_clipped(self.bounds.x + theme.pt(12.0), sb_y + theme.pt(6.0), self.bounds.w - theme.pt(24.0), &self.status, 0xFFF8FAFC, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, _pressed: bool) -> bool {
        self.bounds.contains(mx, my)
    }
}
