use std::collections::BTreeMap;
use rusttype::{Font, Scale, point};

pub struct FontEngine<'a> {
    pub font: Option<Font<'a>>,
    pub cache: BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>,
}

impl<'a> FontEngine<'a> {
    pub fn new(font_data: &'a [u8]) -> Self {
        Self { font: Font::try_from_bytes(font_data), cache: BTreeMap::new() }
    }
    
    pub fn draw_text(&mut self, buffer: &mut [u32], pitch: usize, max_h: usize, x: usize, y: usize, text: &str, color: u32, size: usize) {
        let font = match &self.font { Some(f) => f, None => return };
        let scale = Scale::uniform(size as f32);
        let v_metrics = font.v_metrics(scale);
        let mut cur_x = x;

        for c in text.chars() {
            if c == '\n' { cur_x = x; continue; }
            let key = (size, c);
            if !self.cache.contains_key(&key) {
                let glyph = font.glyph(c).scaled(scale).positioned(point(0.0, v_metrics.ascent));
                let adv = glyph.unpositioned().h_metrics().advance_width.round() as usize;
                if let Some(bb) = glyph.pixel_bounding_box() {
                    let mut cov = vec![0u8; bb.width() as usize * bb.height() as usize];
                    glyph.draw(|gx, gy, v| cov[gy as usize * bb.width() as usize + gx as usize] = (v * 255.0) as u8);
                    self.cache.insert(key, (bb.width() as usize, bb.height() as usize, bb.min.x as isize, bb.min.y as isize, adv.max(1), cov));
                } else {
                    self.cache.insert(key, (0, 0, 0, 0, adv.max(size / 3), Vec::new()));
                }
            }
            let (gw, gh, bx, by, adv, cov) = self.cache.get(&key).unwrap();
            let gx = (cur_x as isize + bx).max(0) as usize;
            let gy = (y as isize + by).max(0) as usize;

            for row in 0..*gh {
                let py = gy + row; if py >= max_h { break; }
                for col in 0..*gw {
                    let px = gx + col; if px >= pitch { break; }
                    let alpha = cov[row * gw + col] as u32;
                    if alpha > 0 {
                        let idx = py * pitch + px;
                        buffer[idx] = blend(buffer[idx], color, alpha);
                    }
                }
            }
            cur_x += adv;
        }
    }
}

pub fn blend(bg: u32, fg: u32, alpha: u32) -> u32 {
    if alpha >= 255 { return fg | 0xFF000000; }
    if alpha == 0 { return bg | 0xFF000000; }
    let inv = 255 - alpha;
    let rb = (((fg & 0x00FF00FF) * alpha + (bg & 0x00FF00FF) * inv) >> 8) & 0x00FF00FF;
    let g = (((fg & 0x0000FF00) * alpha + (bg & 0x0000FF00) * inv) >> 8) & 0x0000FF00;
    0xFF000000 | rb | g
}

pub fn draw_rect(buf: &mut [u32], pitch: usize, max_h: usize, x: usize, y: usize, w: usize, h: usize, color: u32, radius: usize) {
    let ex = (x + w).min(pitch); let ey = (y + h).min(max_h);
    if x >= ex || y >= ey { return; }
    if radius == 0 {
        for cy in y..ey {
            let row = cy * pitch;
            for cx in x..ex { buf[row + cx] = color; }
        }
    } else {
        let r_sq = radius * radius;
        for cy in y..ey {
            let row = cy * pitch;
            let is_top = cy < y + radius; let is_bottom = cy >= ey - radius;
            let dy = if is_top { (y + radius - 1).saturating_sub(cy) } else if is_bottom { cy.saturating_sub(ey - radius) } else { 0 };
            for cx in x..ex {
                let is_left = cx < x + radius; let is_right = cx >= ex - radius;
                if (is_top || is_bottom) && (is_left || is_right) {
                    let dx = if is_left { (x + radius - 1).saturating_sub(cx) } else { cx.saturating_sub(ex - radius) };
                    if dx * dx + dy * dy >= r_sq { continue; }
                }
                buf[row + cx] = color;
            }
        }
    }
}
