use std::collections::BTreeMap;
use rusttype::{Font, Scale, point};
use crate::framework::widget::Rect;
use crate::framework::vector::draw_vector_icon;

pub struct Canvas<'a> {
    pub buffer: &'a mut [u32],
    pub width: usize,
    pub height: usize,
    pub font: Option<&'a Font<'a>>,
    pub font_cache: &'a mut BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>,
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
            font_cache,
            clip_stack: Vec::new(),
        }
    }

    pub fn push_clip(&mut self, rect: Rect) {
        let clipped = if let Some(last) = self.clip_stack.last() {
            rect.intersect(last)
        } else {
            rect.intersect(&Rect::new(0, 0, self.width as i32, self.height as i32))
        };
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
        let rect = Rect::new(x, y, w, h).intersect(&clip);
        if rect.w <= 0 || rect.h <= 0 { return; }

        let alpha = (color >> 24) & 0xFF;
        if alpha == 0 { return; }

        let x1 = rect.x as usize;
        let x2 = (rect.x + rect.w) as usize;

        if radius == 0 {
            if alpha == 255 {
                for cy in rect.y..(rect.y + rect.h) {
                    let row = (cy as usize) * self.width;
                    self.buffer[row + x1..row + x2].fill(color);
                }
            } else {
                for cy in rect.y..(rect.y + rect.h) {
                    let row = (cy as usize) * self.width;
                    for px in &mut self.buffer[row + x1..row + x2] {
                        *px = blend(*px, color, alpha);
                    }
                }
            }
            return;
        }

        let r_sq = (radius * radius) as i32;
        for cy in rect.y..(rect.y + rect.h) {
            let row = (cy as usize) * self.width;
            let is_top = cy < y + radius as i32;
            let is_bottom = cy >= y + h - radius as i32;

            if !is_top && !is_bottom {
                if alpha == 255 {
                    self.buffer[row + x1..row + x2].fill(color);
                } else {
                    for px in &mut self.buffer[row + x1..row + x2] {
                        *px = blend(*px, color, alpha);
                    }
                }
                continue;
            }

            let dy = if is_top { (y + radius as i32 - 1) - cy } else { cy - (y + h - radius as i32) };

            for cx in rect.x..(rect.x + rect.w) {
                let is_left = cx < x + radius as i32;
                let is_right = cx >= x + w - radius as i32;

                if is_left || is_right {
                    let dx = if is_left { (x + radius as i32 - 1) - cx } else { cx - (x + w - radius as i32) };
                    if dx * dx + dy * dy >= r_sq { continue; }
                }

                let idx = row + (cx as usize);
                if alpha < 255 {
                    self.buffer[idx] = blend(self.buffer[idx], color, alpha);
                } else {
                    self.buffer[idx] = color;
                }
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
            self.draw_rect(x, y, w, 1, color, 0);
            self.draw_rect(x, y + h - 1, w, 1, color, 0);
            self.draw_rect(x, y, 1, h, color, 0);
            self.draw_rect(x + w - 1, y, 1, h, color, 0);
        }
    }

    pub fn draw_line_h(&mut self, x: i32, y: i32, w: i32, color: u32) {
        self.draw_rect(x, y, w, 1, color, 0);
    }

    pub fn draw_line_v(&mut self, x: i32, y: i32, h: i32, color: u32) {
        self.draw_rect(x, y, 1, h, color, 0);
    }

    pub fn draw_frosted_glass_rect(&mut self, x: i32, y: i32, w: i32, h: i32, tint: u32, radius: usize) {
        self.draw_rect(x, y, w, h, tint, radius);
    }

    pub fn measure_text(&mut self, text: &str, size: usize) -> (usize, usize) {
        let font = match self.font {
            Some(f) => f,
            None => {
                let scale = (size / 8).max(1);
                let char_w = 8 * scale;
                return (text.chars().count() * char_w, char_w);
            }
        };
        let scale = Scale::uniform(size as f32);
        let v_metrics = font.v_metrics(scale);
        let mut width = 0;

        for c in text.chars() {
            if c == '\n' { continue; }
            let key = (size, c);
            if !self.font_cache.contains_key(&key) {
                let glyph = font.glyph(c).scaled(scale).positioned(point(0.0, v_metrics.ascent));
                let adv = glyph.unpositioned().h_metrics().advance_width.round() as usize;
                if let Some(bb) = glyph.pixel_bounding_box() {
                    let mut cov = vec![0u8; bb.width() as usize * bb.height() as usize];
                    glyph.draw(|gx, gy, v| cov[gy as usize * bb.width() as usize + gx as usize] = (v * 255.0) as u8);
                    self.font_cache.insert(key, (bb.width() as usize, bb.height() as usize, bb.min.x as isize, bb.min.y as isize, adv.max(1), cov));
                } else {
                    self.font_cache.insert(key, (0, 0, 0, 0, adv.max(size / 3), Vec::new()));
                }
            }
            if let Some((_, _, _, _, adv, _)) = self.font_cache.get(&key) {
                width += *adv;
            }
        }
        (width, size)
    }

    pub fn draw_text(&mut self, x: i32, y: i32, text: &str, color: u32, size: usize) {
        let clip = self.current_clip();
        let font = match self.font {
            Some(f) => f,
            None => {
                let scale = (size / 8).max(1) as i32;
                let adv = 8 * scale;
                let mut cur_x = x;
                let alpha = (color >> 24) & 0xFF;
                for c in text.chars() {
                    if c == '\n' {
                        cur_x = x;
                        continue;
                    }
                    let glyph = fallback_glyph_8x8(c);
                    for (row, &bits) in glyph.iter().enumerate() {
                        if bits == 0 { continue; }
                        for col in 0..8 {
                            if (bits & (0x80 >> col)) != 0 {
                                for sy in 0..scale {
                                    let py = y + (row as i32 * scale) + sy;
                                    if py < clip.y || py >= clip.y + clip.h || py < 0 || py >= self.height as i32 { continue; }
                                    let row_idx = (py as usize) * self.width;
                                    for sx in 0..scale {
                                        let px = cur_x + (col as i32 * scale) + sx;
                                        if px < clip.x || px >= clip.x + clip.w || px < 0 || px >= self.width as i32 { continue; }
                                        let idx = row_idx + (px as usize);
                                        if alpha < 255 {
                                            self.buffer[idx] = blend(self.buffer[idx], color, alpha);
                                        } else {
                                            self.buffer[idx] = color;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    cur_x += adv;
                }
                return;
            }
        };
        let scale = Scale::uniform(size as f32);
        let v_metrics = font.v_metrics(scale);
        let mut cur_x = x;

        for c in text.chars() {
            if c == '\n' { cur_x = x; continue; }
            let key = (size, c);
            if !self.font_cache.contains_key(&key) {
                let glyph = font.glyph(c).scaled(scale).positioned(point(0.0, v_metrics.ascent));
                let adv = glyph.unpositioned().h_metrics().advance_width.round() as usize;
                if let Some(bb) = glyph.pixel_bounding_box() {
                    let mut cov = vec![0u8; bb.width() as usize * bb.height() as usize];
                    glyph.draw(|gx, gy, v| cov[gy as usize * bb.width() as usize + gx as usize] = (v * 255.0) as u8);
                    self.font_cache.insert(key, (bb.width() as usize, bb.height() as usize, bb.min.x as isize, bb.min.y as isize, adv.max(1), cov));
                } else {
                    self.font_cache.insert(key, (0, 0, 0, 0, adv.max(size / 3), Vec::new()));
                }
            }

            let (gw, gh, bx, by, adv, cov) = self.font_cache.get(&key).unwrap();
            let gx = cur_x + *bx as i32;
            let gy = y + *by as i32; // Exact pixel-perfect baseline

            for row in 0..*gh {
                let py = gy + row as i32;
                if py < clip.y || py >= clip.y + clip.h || py < 0 || py >= self.height as i32 { continue; }
                let row_idx = (py as usize) * self.width;

                for col in 0..*gw {
                    let px = gx + col as i32;
                    if px < clip.x || px >= clip.x + clip.w || px < 0 || px >= self.width as i32 { continue; }
                    let alpha = cov[row * gw + col] as u32;
                    if alpha > 0 {
                        let idx = row_idx + (px as usize);
                        self.buffer[idx] = blend(self.buffer[idx], color, alpha);
                    }
                }
            }
            cur_x += *adv as i32;
        }
    }

    pub fn draw_text_clipped(&mut self, x: i32, y: i32, max_w: i32, text: &str, color: u32, size: usize) {
        if max_w <= 0 { return; }
        self.push_clip(Rect::new(x, y - 4, max_w, size as i32 + 10));
        self.draw_text(x, y, text, color, size);
        self.pop_clip();
    }

    pub fn draw_text_with_icons(&mut self, x: i32, y: i32, text: &str, color: u32, size: usize) {
        let parts: Vec<&str> = text.split(' ').collect();
        let mut cur_x = x;
        for p in parts {
            if p.starts_with(':') && p.ends_with(':') {
                let icon_name = p.trim_matches(':');
                draw_vector_icon(self, icon_name, cur_x, y, size as i32, size as i32, Some(color));
                cur_x += size as i32 + 4;
            } else {
                self.draw_text(cur_x, y, p, color, size);
                let (w, _) = self.measure_text(p, size);
                cur_x += w as i32 + 6;
            }
        }
    }

    pub fn draw_linux_cursor(&mut self, mx: i32, my: i32) {
        #[rustfmt::skip]
        let shape: [&[u8; 13]; 19] = [
            b"B............",
            b"BB...........",
            b"BWB..........",
            b"BWWBD........",
            b"BWWWBD.......",
            b"BWWWWBD......",
            b"BWWWWWBD.....",
            b"BWWWWWWBD....",
            b"BWWWWWWWBD...",
            b"BWWWWWWWWBD..",
            b"BWWWWBBBBBBD.",
            b"BWWWB........",
            b"BWWB.........",
            b"BWB.BB.......",
            b"BB..BWB......",
            b"B....BWB.....",
            b".....BWB.....",
            b"......BB.....",
            b".............",
        ];

        for (ry, row) in shape.iter().enumerate() {
            let py = my + ry as i32;
            if py < 0 || py >= self.height as i32 { continue; }
            for (rx, &pixel) in row.iter().enumerate() {
                let px = mx + rx as i32;
                if px < 0 || px >= self.width as i32 { continue; }
                let col = match pixel {
                    b'W' => Some(0xFF0F172A),
                    b'B' => Some(0xFFFFFFFF),
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

    pub fn draw_pointer_hand_cursor(&mut self, mx: i32, my: i32) {
        #[rustfmt::skip]
        let shape: [&[u8; 15]; 18] = [
            b"...BBB.........",
            b"..BWWWB........",
            b"..BWWWB........",
            b"..BWWWB........",
            b"..BWWWB.BBBB...",
            b"..BWWWB.BWWWB..",
            b"..BWWWB.BWWWB..",
            b".BBWWWWBBWWWB..",
            b"BWWWWWWWWWWWB..",
            b"BWWWWWWWWWWWB..",
            b".BWWWWWWWWWWBD.",
            b"..BWWWWWWWWBD..",
            b"..BWWWWWWWWBD..",
            b"..BWWWWWWWWBD..",
            b"...BWWWWWWBD...",
            b"...BBBBBBBBD...",
            b"....DDDDDDD....",
            b"...............",
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
        for dy in -5..=5 {
            let py = my + dy;
            if py < 0 || py >= self.height as i32 { continue; }
            for dx in -5..=5 {
                let px = mx + dx;
                if px < 0 || px >= self.width as i32 { continue; }
                if (dx == dy) || (dx == dy + 1) || (dx == dy - 1) {
                    let idx = (py as usize) * self.width + (px as usize);
                    self.buffer[idx] = 0xFFFFFFFF;
                }
            }
        }
    }
}

fn fallback_glyph_8x8(c: char) -> [u8; 8] {
    match c {
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '!' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00],
        '"' => [0x66, 0x66, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00],
        '#' => [0x24, 0x7E, 0x24, 0x24, 0x7E, 0x24, 0x00, 0x00],
        '$' => [0x18, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x18, 0x00],
        '%' => [0x62, 0x64, 0x08, 0x10, 0x26, 0x46, 0x00, 0x00],
        '&' => [0x38, 0x44, 0x38, 0x4A, 0x44, 0x3A, 0x00, 0x00],
        '\'' => [0x18, 0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00],
        '(' => [0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
        ')' => [0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
        '*' => [0x00, 0x24, 0x18, 0x7E, 0x18, 0x24, 0x00, 0x00],
        '+' => [0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        '/' => [0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x00, 0x00],
        '0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        '1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        '2' => [0x3C, 0x66, 0x06, 0x1C, 0x30, 0x60, 0x7E, 0x00],
        '3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        '4' => [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
        '5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        '6' => [0x3C, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        '7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        '8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        '9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
        ';' => [0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x30, 0x00],
        '<' => [0x0C, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0C, 0x00],
        '=' => [0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00],
        '>' => [0x30, 0x18, 0x0C, 0x06, 0x0C, 0x18, 0x30, 0x00],
        '?' => [0x3C, 0x66, 0x06, 0x1C, 0x18, 0x00, 0x18, 0x00],
        '@' => [0x3C, 0x66, 0x6E, 0x6A, 0x6E, 0x60, 0x3C, 0x00],
        'A' => [0x18, 0x3C, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x00],
        'B' => [0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
        'C' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        'D' => [0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
        'E' => [0x7E, 0x60, 0x60, 0x78, 0x60, 0x60, 0x7E, 0x00],
        'F' => [0x7E, 0x60, 0x60, 0x78, 0x60, 0x60, 0x60, 0x00],
        'G' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3E, 0x00],
        'H' => [0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'I' => [0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'J' => [0x1E, 0x0C, 0x0C, 0x0C, 0x6C, 0x6C, 0x38, 0x00],
        'K' => [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
        'L' => [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
        'M' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        'N' => [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
        'O' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'P' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'Q' => [0x3C, 0x66, 0x66, 0x66, 0x6A, 0x6C, 0x36, 0x00],
        'R' => [0x7C, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x66, 0x00],
        'S' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
        'T' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'U' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'V' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'W' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        'X' => [0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
        'Y' => [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
        'Z' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00],
        '[' => [0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00],
        '\\' => [0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x00, 0x00],
        ']' => [0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00],
        '^' => [0x18, 0x3C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7E, 0x00],
        '`' => [0x30, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00],
        'a' => [0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00],
        'b' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'c' => [0x00, 0x00, 0x3C, 0x60, 0x60, 0x60, 0x3C, 0x00],
        'd' => [0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'e' => [0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00],
        'f' => [0x1C, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x30, 0x00],
        'g' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        'h' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'i' => [0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'j' => [0x0C, 0x00, 0x1C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38],
        'k' => [0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00],
        'l' => [0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'm' => [0x00, 0x00, 0x66, 0x7F, 0x6B, 0x63, 0x63, 0x00],
        'n' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'o' => [0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'p' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60],
        'q' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x06],
        'r' => [0x00, 0x00, 0x7C, 0x66, 0x60, 0x60, 0x60, 0x00],
        's' => [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
        't' => [0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x1C, 0x00],
        'u' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'v' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'w' => [0x00, 0x00, 0x63, 0x63, 0x6B, 0x7F, 0x36, 0x00],
        'x' => [0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
        'y' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        'z' => [0x00, 0x00, 0x7E, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        '{' => [0x0E, 0x18, 0x18, 0x70, 0x18, 0x18, 0x0E, 0x00],
        '|' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        '}' => [0x70, 0x18, 0x18, 0x0E, 0x18, 0x18, 0x70, 0x00],
        '~' => [0x32, 0x4C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        _ => [0x7E, 0x42, 0x5A, 0x5A, 0x5A, 0x42, 0x7E, 0x00],
    }
}
