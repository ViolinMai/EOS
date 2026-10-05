use crate::framework::widget::Rect;
use rusttype::{Font, Scale, point};
use std::collections::BTreeMap;

pub type FontGlyphCache = BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>;

pub struct Canvas<'a> {
    pub buffer: &'a mut [u32],
    pub width: usize,
    pub height: usize,
    pub font: Option<&'a Font<'a>>,
    pub font_cache: Option<&'a mut FontGlyphCache>,
    pub clip_stack: Vec<Rect>,
}

impl<'a> Canvas<'a> {
    pub fn new(
        buffer: &'a mut [u32],
        width: usize,
        height: usize,
        font: Option<&'a Font<'a>>,
        font_cache: &'a mut FontGlyphCache,
    ) -> Self {
        Self {
            buffer,
            width,
            height,
            font,
            font_cache: Some(font_cache),
            clip_stack: Vec::new(),
        }
    }

    pub fn push_clip(&mut self, rect: Rect) {
        let intersected = if let Some(current) = self.clip_stack.last() {
            let x1 = rect.x.max(current.x);
            let y1 = rect.y.max(current.y);
            let x2 = (rect.x + rect.w).min(current.x + current.w);
            let y2 = (rect.y + rect.h).min(current.y + current.h);
            Rect {
                x: x1,
                y: y1,
                w: (x2 - x1).max(0),
                h: (y2 - y1).max(0),
            }
        } else {
            rect
        };
        self.clip_stack.push(intersected);
    }

    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    #[inline(always)]
    fn is_clipped(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return true;
        }
        if let Some(clip) = self.clip_stack.last() {
            !clip.contains(x, y)
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn blend(bg: u32, fg: u32, alpha: u32) -> u32 {
        if alpha >= 255 { return fg | 0xFF000000; }
        if alpha == 0 { return bg | 0xFF000000; }
        let inv = 255 - alpha;
        let rb = (((fg & 0x00FF00FF) * alpha + (bg & 0x00FF00FF) * inv) >> 8) & 0x00FF00FF;
        let g = (((fg & 0x0000FF00) * alpha + (bg & 0x0000FF00) * inv) >> 8) & 0x0000FF00;
        0xFF000000 | rb | g
    }

    /// تطبيق تأثير Blur ضبابي شفاف (Frosted Glass Blur) على مساحة محددة
    pub fn apply_blur_rect(&mut self, x: i32, y: i32, w: i32, h: i32, radius: usize) {
        let sx = x.max(0) as usize;
        let sy = y.max(0) as usize;
        let ex = ((x + w).min(self.width as i32)).max(0) as usize;
        let ey = ((y + h).min(self.height as i32)).max(0) as usize;
        if sx >= ex || sy >= ey || radius == 0 { return; }

        let r = radius.min(12);
        let rw = ex - sx;
        let rh = ey - sy;
        let mut temp = vec![0u32; rw * rh];

        // تمرير أفقي (Horizontal Pass)
        for cy in 0..rh {
            let row_idx = (sy + cy) * self.width;
            for cx in 0..rw {
                let px_start = cx.saturating_sub(r);
                let px_end = (cx + r).min(rw - 1);
                let count = (px_end - px_start + 1) as u32;

                let mut sum_r = 0u32;
                let mut sum_g = 0u32;
                let mut sum_b = 0u32;

                for kx in px_start..=px_end {
                    let pixel = self.buffer[row_idx + (sx + kx)];
                    sum_r += (pixel >> 16) & 0xFF;
                    sum_g += (pixel >> 8) & 0xFF;
                    sum_b += pixel & 0xFF;
                }

                temp[cy * rw + cx] = (0xFF << 24)
                    | ((sum_r / count) << 16)
                    | ((sum_g / count) << 8)
                    | (sum_b / count);
            }
        }

        // تمرير رأسي (Vertical Pass) وتطبيق النتيجة مباشرة على الـ Buffer
        for cx in 0..rw {
            for cy in 0..rh {
                let py_start = cy.saturating_sub(r);
                let py_end = (cy + r).min(rh - 1);
                let count = (py_end - py_start + 1) as u32;

                let mut sum_r = 0u32;
                let mut sum_g = 0u32;
                let mut sum_b = 0u32;

                for ky in py_start..=py_end {
                    let pixel = temp[ky * rw + cx];
                    sum_r += (pixel >> 16) & 0xFF;
                    sum_g += (pixel >> 8) & 0xFF;
                    sum_b += pixel & 0xFF;
                }

                let idx = (sy + cy) * self.width + (sx + cx);
                self.buffer[idx] = (0xFF << 24)
                    | ((sum_r / count) << 16)
                    | ((sum_g / count) << 8)
                    | (sum_b / count);
            }
        }
    }

    /// رسم مستطيل شفاف مع تأثير Blur زجاجي مدمج
    pub fn draw_frosted_glass_rect(&mut self, x: i32, y: i32, w: i32, h: i32, tint: u32, corner_r: usize) {
        self.apply_blur_rect(x, y, w, h, 6);
        self.draw_rect(x, y, w, h, tint, corner_r);
    }

    pub fn draw_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32, radius: usize) {
        let ex = (x + w).min(self.width as i32);
        let ey = (y + h).min(self.height as i32);
        let sx = x.max(0);
        let sy = y.max(0);
        if sx >= ex || sy >= ey { return; }

        let r = radius as i32;
        let r_sq = r * r;

        for cy in sy..ey {
            for cx in sx..ex {
                if self.is_clipped(cx, cy) { continue; }

                if r > 0 {
                    let is_top = cy < y + r;
                    let is_bottom = cy >= ey - r;
                    let is_left = cx < x + r;
                    let is_right = cx >= ex - r;

                    if (is_top || is_bottom) && (is_left || is_right) {
                        let dx = if is_left { (x + r) - cx - 1 } else { cx - (ex - r) };
                        let dy = if is_top { (y + r) - cy - 1 } else { cy - (ey - r) };
                        if dx * dx + dy * dy >= r_sq {
                            continue;
                        }
                    }
                }

                let idx = (cy as usize) * self.width + (cx as usize);
                let alpha = (color >> 24) & 0xFF;
                self.buffer[idx] = Self::blend(self.buffer[idx], color, alpha);
            }
        }
    }

    pub fn draw_rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32, radius: usize) {
        if radius == 0 {
            self.draw_line_h(x, y, w, color);
            self.draw_line_h(x, y + h - 1, w, color);
            self.draw_line_v(x, y, h, color);
            self.draw_line_v(x + w - 1, y, h, color);
        } else {
            let r = radius as i32;
            let ex = (x + w).min(self.width as i32);
            let ey = (y + h).min(self.height as i32);
            let sx = x.max(0);
            let sy = y.max(0);
            let r_sq = r * r;
            let inner_r = (r - 1).max(0);
            let inner_r_sq = inner_r * inner_r;

            for cy in sy..ey {
                for cx in sx..ex {
                    if self.is_clipped(cx, cy) { continue; }
                    let is_top = cy < y + r;
                    let is_bottom = cy >= ey - r;
                    let is_left = cx < x + r;
                    let is_right = cx >= ex - r;

                    let mut draw_px = false;
                    if (is_top || is_bottom) && (is_left || is_right) {
                        let dx = if is_left { (x + r) - cx - 1 } else { cx - (ex - r) };
                        let dy = if is_top { (y + r) - cy - 1 } else { cy - (ey - r) };
                        let d_sq = dx * dx + dy * dy;
                        if d_sq < r_sq && d_sq >= inner_r_sq { draw_px = true; }
                    } else if cx == x || cx == ex - 1 || cy == y || cy == ey - 1 {
                        draw_px = true;
                    }

                    if draw_px {
                        let idx = (cy as usize) * self.width + (cx as usize);
                        let alpha = (color >> 24) & 0xFF;
                        self.buffer[idx] = Self::blend(self.buffer[idx], color, alpha);
                    }
                }
            }
        }
    }

    pub fn draw_line_h(&mut self, x: i32, y: i32, w: i32, color: u32) {
        let ex = (x + w).min(self.width as i32);
        let sx = x.max(0);
        if y < 0 || y >= self.height as i32 || sx >= ex { return; }
        for cx in sx..ex {
            if !self.is_clipped(cx, y) {
                let idx = (y as usize) * self.width + (cx as usize);
                let alpha = (color >> 24) & 0xFF;
                self.buffer[idx] = Self::blend(self.buffer[idx], color, alpha);
            }
        }
    }

    pub fn draw_line_v(&mut self, x: i32, y: i32, h: i32, color: u32) {
        let ey = (y + h).min(self.height as i32);
        let sy = y.max(0);
        if x < 0 || x >= self.width as i32 || sy >= ey { return; }
        for cy in sy..ey {
            if !self.is_clipped(x, cy) {
                let idx = (cy as usize) * self.width + (x as usize);
                let alpha = (color >> 24) & 0xFF;
                self.buffer[idx] = Self::blend(self.buffer[idx], color, alpha);
            }
        }
    }

    pub fn measure_text(&self, text: &str, size_px: usize) -> (usize, usize) {
        if let Some(font) = self.font {
            let scale = Scale::uniform(size_px as f32);
            let v_metrics = font.v_metrics(scale);
            let h = (v_metrics.ascent - v_metrics.descent).ceil() as usize;
            let mut w = 0.0;
            for c in text.chars() {
                let g = font.glyph(c).scaled(scale);
                w += g.h_metrics().advance_width;
            }
            (w.ceil() as usize, h.max(size_px))
        } else {
            (text.chars().count() * ((size_px * 6) / 10), size_px)
        }
    }

    pub fn draw_text(&mut self, x: i32, y: i32, text: &str, color: u32, size_px: usize) {
        let font = match self.font {
            Some(f) => f,
            None => return,
        };

        let scale = Scale::uniform(size_px as f32);
        let v_metrics = font.v_metrics(scale);
        let mut cur_x = x as f32;

        for c in text.chars() {
            if c == '\n' { continue; }
            let glyph = font.glyph(c).scaled(scale).positioned(point(cur_x, y as f32 + v_metrics.ascent));
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, v| {
                    let px = bb.min.x + gx as i32;
                    let py = bb.min.y + gy as i32;
                    if !self.is_clipped(px, py) {
                        let alpha = ((v * 255.0) as u32).min(255);
                        if alpha > 0 {
                            let idx = (py as usize) * self.width + (px as usize);
                            self.buffer[idx] = Self::blend(self.buffer[idx], color, alpha);
                        }
                    }
                });
            }
            cur_x += glyph.unpositioned().h_metrics().advance_width;
        }
    }

    pub fn draw_text_clipped(&mut self, x: i32, y: i32, max_w: i32, text: &str, color: u32, size_px: usize) {
        if max_w <= 0 { return; }
        let clip_rect = Rect { x, y, w: max_w, h: (size_px + 8) as i32 };
        self.push_clip(clip_rect);
        self.draw_text(x, y, text, color, size_px);
        self.pop_clip();
    }

    pub fn draw_linux_cursor(&mut self, mx: i32, my: i32) {
        #[rustfmt::skip]
        const LINUX_CURSOR_BITMAP: [[u8; 11]; 16] = [
            [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 2, 2, 1, 0, 0, 0, 0, 0, 0],
            [1, 2, 2, 2, 2, 1, 0, 0, 0, 0, 0],
            [1, 2, 2, 2, 2, 2, 1, 0, 0, 0, 0],
            [1, 2, 2, 2, 2, 2, 2, 1, 0, 0, 0],
            [1, 2, 2, 2, 2, 2, 2, 2, 1, 0, 0],
            [1, 2, 2, 2, 2, 2, 2, 2, 2, 1, 0],
            [1, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1],
            [1, 2, 2, 1, 2, 2, 1, 0, 0, 0, 0],
            [1, 2, 1, 0, 1, 2, 2, 1, 0, 0, 0],
            [1, 1, 0, 0, 0, 1, 2, 2, 1, 0, 0],
            [1, 0, 0, 0, 0, 0, 1, 2, 1, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0],
        ];

        for (r, row) in LINUX_CURSOR_BITMAP.iter().enumerate() {
            let py = my + r as i32;
            if py < 0 || py >= self.height as i32 { continue; }
            for (c, &val) in row.iter().enumerate() {
                let px = mx + c as i32;
                if px < 0 || px >= self.width as i32 { continue; }
                let color = match val {
                    1 => 0xFF000000,
                    2 => 0xFFFFFFFF,
                    _ => continue,
                };
                let idx = (py as usize) * self.width + (px as usize);
                self.buffer[idx] = color;
            }
        }
    }
}
