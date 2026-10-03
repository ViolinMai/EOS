use crate::framework::*;
use crate::{f_info, f_error};

pub struct PreviewApp {
    bounds: Rect,
    pub title: String,
    pub image_data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub raw_pixels: Vec<u32>,
    pub error_msg: Option<String>,
}

impl PreviewApp {
    pub fn new(title: String, data: Vec<u8>) -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            title,
            image_data: data,
            width: 0,
            height: 0,
            raw_pixels: Vec::new(),
            error_msg: None,
        };
        app.decode();
        app
    }

    fn decode(&mut self) {
        if self.image_data.is_empty() {
            let err = "Image byte buffer is empty (0 bytes).".to_string();
            f_error!("PREVIEW", "{}", err);
            self.error_msg = Some(err);
            return;
        }

        f_info!("PREVIEW", "Attempting decode for '{}' ({} bytes, magic: {:02X?})",
            self.title, self.image_data.len(), &self.image_data[..core::cmp::min(8, self.image_data.len())]
        );

        // 1. JPEG Format
        if self.image_data.len() >= 2 && self.image_data[0] == 0xFF && self.image_data[1] == 0xD8 {
            let mut decoder = zune_jpeg::JpegDecoder::new(&self.image_data);
            match decoder.decode() {
                Ok(px) => {
                    if let Some(info) = decoder.info() {
                        self.width = info.width as usize;
                        self.height = info.height as usize;
                        let total_pixels = self.width * self.height;
                        self.raw_pixels = vec![0u32; total_pixels];

                        let channels = if px.len() >= total_pixels * 4 { 4 } else if px.len() >= total_pixels * 3 { 3 } else { 1 };
                        for i in 0..total_pixels {
                            let o = i * channels;
                            if o + channels <= px.len() {
                                if channels == 4 {
                                    self.raw_pixels[i] = ((px[o + 3] as u32) << 24) | ((px[o] as u32) << 16) | ((px[o + 1] as u32) << 8) | (px[o + 2] as u32);
                                } else if channels == 3 {
                                    self.raw_pixels[i] = (0xFF << 24) | ((px[o] as u32) << 16) | ((px[o + 1] as u32) << 8) | (px[o + 2] as u32);
                                } else {
                                    let v = px[o] as u32;
                                    self.raw_pixels[i] = (0xFF << 24) | (v << 16) | (v << 8) | v;
                                }
                            }
                        }
                        f_info!("PREVIEW", "JPEG successfully decoded: {}x{} px", self.width, self.height);
                        return;
                    }
                }
                Err(e) => {
                    let err = format!("JPEG decoder error: {:?}", e);
                    f_error!("PREVIEW", "{}", err);
                    self.error_msg = Some(err);
                }
            }
        }

        // 2. PNG Format
        if self.image_data.starts_with(b"\x89PNG\r\n\x1a\n") {
            match crate::png::decode_png(&self.image_data) {
                Ok((px, w, h)) => {
                    self.width = w;
                    self.height = h;
                    self.raw_pixels = px;
                    f_info!("PREVIEW", "PNG successfully decoded: {}x{} px", self.width, self.height);
                    return;
                }
                Err(e) => {
                    let err = format!("PNG decoder error: {}", e);
                    f_error!("PREVIEW", "{}", err);
                    self.error_msg = Some(err);
                }
            }
        }

        // 3. BMP Format Fallback
        if self.image_data.starts_with(b"BM") && self.image_data.len() > 54 {
            let data_offset = u32::from_le_bytes([self.image_data[10], self.image_data[11], self.image_data[12], self.image_data[13]]) as usize;
            let w = i32::from_le_bytes([self.image_data[18], self.image_data[19], self.image_data[20], self.image_data[21]]).abs() as usize;
            let h = i32::from_le_bytes([self.image_data[22], self.image_data[23], self.image_data[24], self.image_data[25]]).abs() as usize;
            let bpp = u16::from_le_bytes([self.image_data[28], self.image_data[29]]) as usize;

            if bpp == 24 || bpp == 32 {
                let bytes_per_pixel = bpp / 8;
                let row_stride = (w * bytes_per_pixel + 3) & !3;
                let mut pixels = vec![0u32; w * h];
                for y in 0..h {
                    let row_start = data_offset + (h - 1 - y) * row_stride;
                    for x in 0..w {
                        let px_offset = row_start + x * bytes_per_pixel;
                        if px_offset + bytes_per_pixel <= self.image_data.len() {
                            let b = self.image_data[px_offset] as u32;
                            let g = self.image_data[px_offset + 1] as u32;
                            let r = self.image_data[px_offset + 2] as u32;
                            let a = if bytes_per_pixel == 4 { self.image_data[px_offset + 3] as u32 } else { 0xFF };
                            pixels[y * w + x] = (a << 24) | (r << 16) | (g << 8) | b;
                        }
                    }
                }
                self.width = w;
                self.height = h;
                self.raw_pixels = pixels;
                f_info!("PREVIEW", "BMP successfully decoded: {}x{} px", w, h);
                return;
            }
        }

        if self.error_msg.is_none() {
            let err = format!("Unsupported image format or corrupt header (Magic: {:02X?})", &self.image_data[..core::cmp::min(4, self.image_data.len())]);
            f_error!("PREVIEW", "{}", err);
            self.error_msg = Some(err);
        }
    }
}

impl Widget for PreviewApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF050811, 0);

        if self.width == 0 || self.height == 0 || self.raw_pixels.is_empty() {
            let msg = format!("Image: {} ({} bytes)", self.title, self.image_data.len());
            canvas.draw_text(self.bounds.x + theme.pt(20.0), self.bounds.y + theme.pt(30.0), &msg, theme.text_primary, theme.font_title());
            if let Some(err) = &self.error_msg {
                canvas.draw_text(self.bounds.x + theme.pt(20.0), self.bounds.y + theme.pt(60.0), &format!("Decode Error: {}", err), 0xFFEF4444, theme.font_body());
            } else {
                canvas.draw_text(self.bounds.x + theme.pt(20.0), self.bounds.y + theme.pt(60.0), "Decoding failed or no pixel buffer produced.", 0xFFF59E0B, theme.font_body());
            }
            return;
        }

        let step_x = (self.width + self.bounds.w - 1) / self.bounds.w;
        let step_y = (self.height + self.bounds.h - 1) / self.bounds.h;
        let step = step_x.max(step_y).max(1);

        let disp_w = (self.width / step).min(self.bounds.w);
        let disp_h = (self.height / step).min(self.bounds.h);
        let off_x = self.bounds.x + (self.bounds.w.saturating_sub(disp_w) / 2);
        let off_y = self.bounds.y + (self.bounds.h.saturating_sub(disp_h) / 2);

        for dy in 0..disp_h {
            let src_y = dy * step;
            let dst_y = off_y + dy;
            if dst_y >= canvas.height { break; }
            for dx in 0..disp_w {
                let src_x = dx * step;
                let dst_x = off_x + dx;
                if dst_x >= canvas.width { break; }
                let col = self.raw_pixels[src_y * self.width + src_x];
                canvas.buffer[dst_y * canvas.width + dst_x] = col;
            }
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, _pressed: bool) -> bool {
        // Return true if mouse is inside bounds so clicks never fall through
        self.bounds.contains(mx, my)
    }
}
