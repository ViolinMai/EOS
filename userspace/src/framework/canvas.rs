use std::collections::BTreeMap;
use rusttype::{Font, Scale, point};
use crate::framework::widget::Rect;
use crate::framework::vector::draw_vector_icon;

pub struct Canvas<'a> {
    pub buffer: &'a mut [u32],
    pub width: usize,
    pub height: usize,
    pub font: Option<&'a Font<'a>>,
    pub font_cache: Option<&'a mut BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>>,
    pub clip_stack: Vec<Rect>,
}

#[inline(always)]
fn blend(bg: u32, fg: u32, alpha: u32) -> u32 {
    if alpha >= 255 { return fg | 0xFF000000; }
    if alpha == 0 { return bg | 0xFF000000; }
    let inv = 255 - alpha;
    let rb = (((fg & 0x00FF00FF) * alpha + (bg & 0x00FF00FF) * inv) >> 8) & 0x00FF00FF;
    let g = (((fg & 0x0000FF00) * alpha + (bg & 0x0000FF00) * inv) >> 8) & 0x0000FF00;
    0xFF000000 | rb | g
}

impl<'a> Canvas<'a> {
    pub fn new(
        buffer: &'a mut [u32],
        width: usize,
        height: usize,
        font: Option<&'a Font<'a>>,
        font_cache: &'a mut BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>,
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

    pub fn clear(&mut self, color: u32) {
        self.buffer.fill(color);
    }

    pub fn push_clip(&mut self, rect: Rect) {
        let current = self.current_clip();
        let clipped = current.intersect(&rect);
        self.clip_stack.push(clipped);
    }

    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    pub fn current_clip(&self) -> Rect {
        self.clip_stack.last().copied().unwrap_or(Rect::new(0, 0, self.width as i32, self.height as i32))
    }

    pub fn draw_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32, radius: usize) {
        let clip = self.current_clip();
        let ex = (x + w).min(clip.x + clip.w).min(self.width as i32);
        let ey = (y + h).min(clip.y + clip.h).min(self.height as i32);
        let sx = x.max(clip.x).max(0);
        let sy = y.max(clip.y).max(0);

        if sx >= ex || sy >= ey { return; }
        let a = (color >> 24) & 0xFF;

        if radius == 0 {
            for cy in sy..ey {
                let row = (cy as usize) * self.width;
                if a == 255 || a == 0 {
                    let fill_c = if a == 0 { color | 0xFF000000 } else { color };
                    for cx in sx..ex {
                        self.buffer[row + (cx as usize)] = fill_c;
                    }
                } else {
                    for cx in sx..ex {
                        let idx = row + (cx as usize);
                        self.buffer[idx] = blend(self.buffer[idx], color, a);
                    }
                }
            }
        } else {
            let r_sq = (radius * radius) as i32;
            for cy in sy..ey {
                let row = (cy as usize) * self.width;
                let is_top = cy < y + radius as i32;
                let is_bottom = cy >= y + h - radius as i32;
                let dy = if is_top { (y + radius as i32 - 1) - cy } else if is_bottom { cy - (y + h - radius as i32) } else { 0 };

                for cx in sx..ex {
                    let is_left = cx < x + radius as i32;
                    let is_right = cx >= x + w - radius as i32;
                    if (is_top || is_bottom) && (is_left || is_right) {
                        let dx = if is_left { (x + radius as i32 - 1) - cx } else { cx - (x + w - radius as i32) };
                        if dx * dx + dy * dy >= r_sq { continue; }
                    }
                    let idx = row + (cx as usize);
                    if a == 255 || a == 0 {
                        self.buffer[idx] = if a == 0 { color | 0xFF000000 } else { color };
                    } else {
                        self.buffer[idx] = blend(self.buffer[idx], color, a);
                    }
                }
            }
        }
    }

    pub fn draw_frosted_glass_rect(&mut self, x: i32, y: i32, w: i32, h: i32, tint: u32, radius: usize) {
        let alpha = ((tint >> 24) & 0xFF).max(180);
        self.draw_rect(x, y, w, h, (tint & 0x00FFFFFF) | (alpha << 24), radius);
    }

    pub fn draw_rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32, radius: usize) {
        if radius == 0 {
            self.draw_line_h(x, y, w, color);
            self.draw_line_h(x, y + h - 1, w, color);
            self.draw_line_v(x, y, h, color);
            self.draw_line_v(x + w - 1, y, h, color);
        } else {
            self.draw_rect(x, y, w, h, color, radius);
        }
    }

    pub fn draw_line_h(&mut self, x: i32, y: i32, w: i32, color: u32) {
        self.draw_rect(x, y, w, 1, color, 0);
    }

    pub fn draw_line_v(&mut self, x: i32, y: i32, h: i32, color: u32) {
        self.draw_rect(x, y, 1, h, color, 0);
    }

    pub fn draw_text(&mut self, x: i32, y: i32, text: &str, color: u32, size: usize) {
        let (w, _) = self.measure_text(text, size);
        self.draw_text_clipped(x, y, w as i32, text, color, size);
    }

    pub fn draw_text_clipped(&mut self, x: i32, y: i32, max_w: i32, text: &str, color: u32, size: usize) {
        let clip = self.current_clip();
        if self.font.is_some() && self.font_cache.is_some() {
            let font = self.font.unwrap();
            let cache = self.font_cache.as_mut().unwrap();
            let scale = Scale::uniform(size as f32);
            let v_metrics = font.v_metrics(scale);
            let mut cur_x = x;

            for c in text.chars() {
                if cur_x - x >= max_w { break; }
                let key = (size, c);
                if !cache.contains_key(&key) {
                    let glyph = font.glyph(c).scaled(scale).positioned(point(0.0, v_metrics.ascent));
                    let adv = glyph.unpositioned().h_metrics().advance_width.round() as usize;
                    if let Some(bb) = glyph.pixel_bounding_box() {
                        let mut cov = vec![0u8; bb.width() as usize * bb.height() as usize];
                        glyph.draw(|gx, gy, v| cov[gy as usize * bb.width() as usize + gx as usize] = (v * 255.0) as u8);
                        cache.insert(key, (bb.width() as usize, bb.height() as usize, bb.min.x as isize, bb.min.y as isize, adv.max(1), cov));
                    } else {
                        cache.insert(key, (0, 0, 0, 0, adv.max(size / 3), Vec::new()));
                    }
                }

                let (gw, gh, bx, by, adv, cov) = cache.get(&key).unwrap();
                let gx = cur_x + *bx as i32;
                let gy = y + *by as i32;

                for row in 0..*gh {
                    let py = gy + row as i32;
                    if py < clip.y || py >= clip.y + clip.h || py < 0 || py >= self.height as i32 { continue; }
                    for col in 0..*gw {
                        let px = gx + col as i32;
                        if px < clip.x || px >= clip.x + clip.w || px < 0 || px >= self.width as i32 { continue; }
                        let alpha = cov[row * gw + col] as u32;
                        if alpha > 0 {
                            let idx = (py as usize) * self.width + (px as usize);
                            self.buffer[idx] = blend(self.buffer[idx], color, alpha);
                        }
                    }
                }
                cur_x += *adv as i32;
            }
        }
    }

    pub fn draw_text_with_icons(&mut self, x: i32, y: i32, text: &str, color: u32, size: usize) {
        let mut cur_x = x;
        let icon_sz = (size as i32 + 2).max(12);

        let mut parts = text.split(':');
        if let Some(first) = parts.next() {
            if !first.is_empty() {
                self.draw_text(cur_x, y, first, color, size);
                let (w, _) = self.measure_text(first, size);
                cur_x += w as i32;
            }
        }

        while let Some(tag) = parts.next() {
            let next_text = parts.next().unwrap_or("");
            let is_known_icon = matches!(tag, "folder" | "close" | "back" | "gear" | "doc" | "img" | "check" | "terminal" | "monitor" | "browser");

            if is_known_icon {
                cur_x += 2;
                draw_vector_icon(self, tag, cur_x, y - 1, icon_sz, icon_sz, Some(color));
                cur_x += icon_sz + 4;
            } else {
                let restored = format!(":{}:", tag);
                self.draw_text(cur_x, y, &restored, color, size);
                let (w, _) = self.measure_text(&restored, size);
                cur_x += w as i32;
            }

            if !next_text.is_empty() {
                self.draw_text(cur_x, y, next_text, color, size);
                let (w, _) = self.measure_text(next_text, size);
                cur_x += w as i32;
            }
        }
    }

    pub fn measure_text(&self, text: &str, size: usize) -> (usize, usize) {
        if let (Some(font), Some(cache)) = (self.font, &self.font_cache) {
            let scale = Scale::uniform(size as f32);
            let mut total_w = 0;
            for c in text.chars() {
                let key = (size, c);
                if let Some((_, _, _, _, adv, _)) = cache.get(&key) {
                    total_w += adv;
                } else {
                    let adv = font.glyph(c).scaled(scale).h_metrics().advance_width.round() as usize;
                    total_w += adv.max(size / 3);
                }
            }
            (total_w, size)
        } else {
            let char_w = (size * 6) / 10;
            (text.len() * char_w, size)
        }
    }

    /// مؤشر لينكس الحقيقي المتطابق مع Breeze / Adwaita
    pub fn draw_linux_cursor(&mut self, mx: i32, my: i32) {
        // مصفوفة مؤشر لينكس بدقة 19x12 بكسل حقيقية (حواف بيضاء، قلب أسود، ظل شفاف)
        #[rustfmt::skip]
        let shape: [&[u8; 12]; 19] = [
            b"B...........",
            b"WB..........",
            b"WWB.........",
            b"WWWB........",
            b"WWWWB.......",
            b"WWWWWB......",
            b"WWWWWWB.....",
            b"WWWWWWWB....",
            b"WWWWWWWWB...",
            b"WWWWWWWWWB..",
            b"WWWWWWWWWWBD",
            b"WWWWWB......",
            b"WWWB.WB.....",
            b"WWB...WB....",
            b"WB....WB....",
            b"B......WB...",
            b".......WB...",
            b"........B...",
            b"............",
        ];

        for (ry, row) in shape.iter().enumerate() {
            let py = my + ry as i32;
            if py < 0 || py >= self.height as i32 { continue; }
            for (rx, &pixel) in row.iter().enumerate() {
                let px = mx + rx as i32;
                if px < 0 || px >= self.width as i32 { continue; }
                let col = match pixel {
                    b'W' => Some(0xFF0F172A), // قلب السهم أسود
                    b'B' => Some(0xFFFFFFFF), // الحدود الخارجية بيضاء نقية
                    b'D' => Some(0x88000000), // ظل خفيف
                    _ => None,
                };
                if let Some(c) = col {
                    let idx = (py as usize) * self.width + (px as usize);
                    self.buffer[idx] = c;
                }
            }
        }
    }

    pub fn draw_pointer_hand_cursor(&mut self, mx: i32, my: i32) {
        #[rustfmt::skip]
        let shape: [&[u8; 14]; 17] = [
            b"...BB.........",
            b"..BWWBD.......",
            b"..BWWBD.......",
            b"..BWWBD.......",
            b"..BWWBBBBBD...",
            b"..BWWWWWWWWBD.",
            b".BBWWWWWWWWBD.",
            b"BWWWWWWWWWWBD.",
            b"BWWWWWWWWWWBD.",
            b".BWWWWWWWWBD..",
            b"..BWWWWWWBD...",
            b"..BWWWWWWBD...",
            b"..BWWWWWWBD...",
            b"...BWWWWBD....",
            b"...BBBBBD.....",
            b"....DDDD......",
            b"..............",
        ];

        for (ry, row) in shape.iter().enumerate() {
            let py = my + ry as i32;
            if py < 0 || py >= self.height as i32 { continue; }
            for (rx, &pixel) in row.iter().enumerate() {
                let px = mx + rx as i32;
                if px < 0 || px >= self.width as i32 { continue; }
                let col = match pixel {
                    b'W' => Some(0xFFFFFFFF),
                    b'B' => Some(0xFF0F172A),
                    b'D' => Some(0x66000000),
                    _ => None,
                };
                if let Some(c) = col {
                    let idx = (py as usize) * self.width + (px as usize);
                    self.buffer[idx] = c;
                }
            }
        }
    }

    pub fn draw_resize_corner_cursor(&mut self, mx: i32, my: i32) {
        for d in -6..=6 {
            let px = mx + d;
            let py = my + d;
            if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                self.buffer[(py as usize) * self.width + (px as usize)] = 0xFFFFFFFF;
                if px + 1 < self.width as i32 { self.buffer[(py as usize) * self.width + (px as usize + 1)] = 0xFF000000; }
            }
        }
        self.draw_line_h(mx - 6, my - 6, 5, 0xFFFFFFFF);
        self.draw_line_v(mx - 6, my - 6, 5, 0xFFFFFFFF);
        self.draw_line_h(mx + 2, my + 6, 5, 0xFFFFFFFF);
        self.draw_line_v(mx + 6, my + 2, 5, 0xFFFFFFFF);
    }
}
