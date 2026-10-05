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
    pub is_fullscreen: bool,
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
            status: String::from("Reading image file..."),
            is_fullscreen: false,
        };
        app.load_image_safe();
        app
    }

    pub fn load_image_safe(&mut self) {
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

        if data.is_empty() {
            self.status = format!("File not found: {}", self.file_path);
            return;
        }

        let total_bytes = data.len();
        self.status = format!("Decoding image ({} KB)...", total_bytes / 1024);

        // 1. فك تشفير PNG
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

        // 2. فك تشفير JPEG عبر zune-jpeg مباشرة في مساحة الـ Userspace
        if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
            let mut decoder = zune_jpeg::JpegDecoder::new(&data);
            match decoder.decode() {
                Ok(raw) => {
                    if let Some(info) = decoder.info() {
                        let orig_w = info.width as usize;
                        let orig_h = info.height as usize;

                        // حماية الذاكرة: تقليص الدقة تلقائياً إذا كانت الصورة عملاقة (أكبر من 1080p)
                        let max_disp_w = 1920usize;
                        let max_disp_h = 1080usize;
                        let step = ((orig_w + max_disp_w - 1) / max_disp_w).max((orig_h + max_disp_h - 1) / max_disp_h).max(1);

                        let out_w = orig_w / step;
                        let out_h = orig_h / step;
                        let mut px = vec![0u32; out_w * out_h];

                        for dy in 0..out_h {
                            let sy = dy * step;
                            let src_row = sy * orig_w * 3;
                            let dst_row = dy * out_w;

                            for dx in 0..out_w {
                                let sx = dx * step;
                                let o = src_row + (sx * 3);
                                if o + 2 < raw.len() {
                                    px[dst_row + dx] = (0xFF << 24)
                                        | ((raw[o] as u32) << 16)
                                        | ((raw[o+1] as u32) << 8)
                                        | (raw[o+2] as u32);
                                }
                            }
                        }

                        self.pixels = px;
                        self.img_w = out_w;
                        self.img_h = out_h;
                        self.status = format!("JPEG (Orig: {}x{}, Fitted: {}x{})", orig_w, orig_h, out_w, out_h);
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
        let draw_x = if self.is_fullscreen { 0 } else { self.bounds.x };
        let draw_y = if self.is_fullscreen { 0 } else { self.bounds.y };
        let draw_w = if self.is_fullscreen { canvas.width as i32 } else { self.bounds.w };
        let draw_h = if self.is_fullscreen { canvas.height as i32 } else { self.bounds.h };

        canvas.draw_rect(draw_x, draw_y, draw_w, draw_h, 0xFF0B1120, 0);

        let sb_h = if self.is_fullscreen { 0 } else { theme.pt(32.0) };
        let content_h = (draw_h - sb_h).max(0);

        if self.img_w > 0 && self.img_h > 0 && !self.pixels.is_empty() {
            let avail_w = draw_w.max(1) as f32;
            let avail_h = content_h.max(1) as f32;

            let scale = (avail_w / self.img_w as f32).min(avail_h / self.img_h as f32);
            let disp_w = (self.img_w as f32 * scale).round() as i32;
            let disp_h = (self.img_h as f32 * scale).round() as i32;

            let off_x = draw_x + (draw_w - disp_w) / 2;
            let off_y = draw_y + (content_h - disp_h) / 2;

            for dy in 0..disp_h {
                let target_y = off_y + dy;
                if target_y < draw_y || target_y >= draw_y + content_h { continue; }
                let src_y = ((dy as f32 / scale) as usize).min(self.img_h - 1);

                for dx in 0..disp_w {
                    let target_x = off_x + dx;
                    if target_x < draw_x || target_x >= draw_x + draw_w { continue; }
                    let src_x = ((dx as f32 / scale) as usize).min(self.img_w - 1);

                    let color = self.pixels[src_y * self.img_w + src_x];
                    let idx = (target_y as usize) * canvas.width + (target_x as usize);
                    if idx < canvas.buffer.len() {
                        canvas.buffer[idx] = color;
                    }
                }
            }
        } else {
            let (tw, th) = canvas.measure_text(&self.status, theme.font_body());
            canvas.draw_text(draw_x + (draw_w - tw as i32) / 2, draw_y + (content_h - th as i32) / 2, &self.status, 0xFF94A3B8, theme.font_body());
        }

        if !self.is_fullscreen {
            let bar_y = self.bounds.y + self.bounds.h - sb_h;
            canvas.draw_rect(self.bounds.x, bar_y, self.bounds.w, sb_h, theme.bg_titlebar, 0);
            canvas.draw_line_h(self.bounds.x, bar_y, self.bounds.w, theme.border_window);

            let fs_btn_w = theme.pt(90.0);
            let fs_btn_x = self.bounds.x + self.bounds.w - fs_btn_w - theme.pt(8.0);
            canvas.draw_rect(fs_btn_x, bar_y + theme.pt(4.0), fs_btn_w, sb_h - theme.pt(8.0), theme.accent, theme.pt(4.0) as usize);
            let (ftw, fth) = canvas.measure_text("Full Screen", theme.font_caption());
            canvas.draw_text(fs_btn_x + (fs_btn_w - ftw as i32) / 2, bar_y + (sb_h - fth as i32) / 2, "Full Screen", 0xFFFFFFFF, theme.font_caption());

            canvas.draw_text_clipped(self.bounds.x + theme.pt(12.0), bar_y + theme.pt(6.0), self.bounds.w - fs_btn_w - theme.pt(28.0), &self.status, theme.text_muted, theme.font_caption());
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if self.is_fullscreen {
            if pressed {
                self.is_fullscreen = false;
                return true;
            }
            return true;
        }

        if !self.bounds.contains(mx, my) { return false; }

        if pressed {
            let theme = get_theme();
            let sb_h = theme.pt(32.0);
            let bar_y = self.bounds.y + self.bounds.h - sb_h;

            let fs_btn_w = theme.pt(90.0);
            let fs_btn_x = self.bounds.x + self.bounds.w - fs_btn_w - theme.pt(8.0);
            if mx >= fs_btn_x && mx <= fs_btn_x + fs_btn_w && my >= bar_y && my <= bar_y + sb_h {
                self.is_fullscreen = true;
                return true;
            }
        }
        true
    }

    fn handle_key(&mut self, keycode: u8, _mods: u8) -> bool {
        if keycode == 0x01 && self.is_fullscreen {
            self.is_fullscreen = false;
            return true;
        }
        false
    }
}
