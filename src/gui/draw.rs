use alloc::vec::Vec;
use crate::font::FontWeight;
use crate::gui::metrics::get_metrics;

pub struct FrameBuffer<'a> { pub pixels: &'a mut [u32], pub width: usize, pub height: usize, pub pitch_pixels: usize }

#[derive(Clone, Copy)]
pub struct Rect { pub x: usize, pub y: usize, pub w: usize, pub h: usize }
impl Rect {
    pub fn intersects(&self, other: &Rect) -> bool { self.x < other.x + other.w && self.x + self.w > other.x && self.y < other.y + other.h && self.y + self.h > other.y }
    pub fn union(&self, other: &Rect) -> Rect { let x1 = self.x.min(other.x); let y1 = self.y.min(other.y); let x2 = (self.x + self.w).max(other.x + other.w); let y2 = (self.y + self.h).max(other.y + other.h); Rect { x: x1, y: y1, w: x2 - x1, h: y2 - y1 } }
    pub fn clip(&self, fb_w: usize, fb_h: usize) -> Rect { let ex = (self.x + self.w).min(fb_w); let ey = (self.y + self.h).min(fb_h); Rect { x: self.x, y: self.y, w: ex.saturating_sub(self.x), h: ey.saturating_sub(self.y) } }
}

pub struct DamageTracker { pub rects: Vec<Rect> }
impl DamageTracker {
    pub fn new() -> Self { Self { rects: Vec::with_capacity(32) } }
    pub fn add(&mut self, rect: Rect) {
        if rect.w == 0 || rect.h == 0 { return; }
        for r in &mut self.rects { if r.intersects(&rect) { *r = r.union(&rect); return; } }
        if self.rects.len() < 30 { self.rects.push(rect); } else { self.rects[0] = self.rects[0].union(&rect); }
    }
    pub fn get_rects(&self) -> &[Rect] { &self.rects }
    pub fn clear(&mut self) { self.rects.clear(); }
}

#[inline(always)]
pub fn blend_color(bg: u32, fg: u32, alpha: u32) -> u32 {
    if alpha >= 255 { return fg | 0xFF000000; }
    if alpha == 0 { return bg | 0xFF000000; }
    let inv = 255 - alpha;
    let rb = (((fg & 0x00FF00FF) * alpha + (bg & 0x00FF00FF) * inv) >> 8) & 0x00FF00FF;
    let g = (((fg & 0x0000FF00) * alpha + (bg & 0x0000FF00) * inv) >> 8) & 0x0000FF00;
    0xFF000000 | rb | g
}

pub fn draw_rect(fb: &mut FrameBuffer, x: usize, y: usize, w: usize, h: usize, color: u32) {
    let ex = (x + w).min(fb.width); let ey = (y + h).min(fb.height);
    if x >= ex || y >= ey { return; }
    for cy in y..ey { fb.pixels[cy * fb.pitch_pixels + x..cy * fb.pitch_pixels + ex].fill(color | 0xFF000000); }
}

pub fn draw_rect_alpha(fb: &mut FrameBuffer, x: usize, y: usize, w: usize, h: usize, color: u32, alpha: u32) {
    let ex = (x + w).min(fb.width); let ey = (y + h).min(fb.height);
    if x >= ex || y >= ey { return; }
    for cy in y..ey {
        let start = cy * fb.pitch_pixels + x;
        for px in &mut fb.pixels[start..start + (ex - x)] { *px = blend_color(*px, color, alpha); }
    }
}

pub fn draw_line_h(fb: &mut FrameBuffer, x: usize, y: usize, w: usize, color: u32) { draw_rect(fb, x, y, w, 1, color); }
pub fn draw_line_v(fb: &mut FrameBuffer, x: usize, y: usize, h: usize, color: u32) { draw_rect(fb, x, y, 1, h, color); }

pub fn draw_rect_rounded(fb: &mut FrameBuffer, x: usize, y: usize, w: usize, h: usize, r: usize, color: u32, alpha: u32) {
    let ex = (x + w).min(fb.width); let ey = (y + h).min(fb.height);
    if x >= ex || y >= ey { return; }
    let r = r.min(w / 2).min(h / 2);
    if r == 0 { if alpha >= 255 { draw_rect(fb, x, y, w, h, color); } else { draw_rect_alpha(fb, x, y, w, h, color, alpha); } return; }
    let r_sq = r * r; let len = ex - x;
    
    for cy in y..ey {
        let is_top = cy < y + r; let is_bottom = cy >= ey - r;
        let start = cy * fb.pitch_pixels + x;
        
        if !is_top && !is_bottom {
            if alpha >= 255 { fb.pixels[start..start + len].fill(color | 0xFF000000); }
            else { for px in &mut fb.pixels[start..start + len] { *px = blend_color(*px, color, alpha); } }
        } else {
            let dy = if is_top { (y + r - 1).saturating_sub(cy) } else { cy.saturating_sub(ey - r) };
            for cx in x..ex {
                let is_left = cx < x + r; let is_right = cx >= ex - r; let mut p_alpha = alpha;
                if is_left || is_right {
                    let dx = if is_left { (x + r - 1).saturating_sub(cx) } else { cx.saturating_sub(ex - r) };
                    let dist_sq = dx * dx + dy * dy;
                    if dist_sq >= r_sq { continue; }
                    if dist_sq > r_sq.saturating_sub(r * 2) { p_alpha = (alpha * (r_sq - dist_sq) as u32) / (r * 2) as u32; }
                }
                let idx = cy * fb.pitch_pixels + cx;
                fb.pixels[idx] = blend_color(fb.pixels[idx], color, p_alpha);
            }
        }
    }
}

pub fn draw_rect_outline(fb: &mut FrameBuffer, x: usize, y: usize, w: usize, h: usize, color: u32, r: usize) {
    if r == 0 {
        draw_line_h(fb, x, y, w, color); draw_line_h(fb, x, y + h - 1, w, color);
        draw_line_v(fb, x, y, h, color); draw_line_v(fb, x + w - 1, y, h, color);
    } else { draw_rect_rounded(fb, x, y, w, h, r, color, 120); }
}

pub fn draw_text(fb: &mut FrameBuffer, x: usize, y: usize, text: &str, color: u32, size_idx: usize) {
    let metrics = get_metrics();
    let size_px = match size_idx { 0 => metrics.font_caption, 1 => metrics.font_body, 2 => metrics.font_title, _ => metrics.font_heading };
    let weight = match size_idx { 0 | 1 => FontWeight::Regular, 2 => FontWeight::Medium, _ => FontWeight::Bold };
    let font_mgr = unsafe { crate::font::get_font_manager() };
    font_mgr.draw_text_aa(fb, x, y, text, color, size_px, weight);
}

pub fn draw_text_weight(fb: &mut FrameBuffer, x: usize, y: usize, text: &str, color: u32, size_px: usize, weight: FontWeight) {
    let font_mgr = unsafe { crate::font::get_font_manager() };
    font_mgr.draw_text_aa(fb, x, y, text, color, size_px, weight);
}

pub fn draw_text_clipped(fb: &mut FrameBuffer, x: usize, y: usize, max_w: usize, text: &str, color: u32, size_idx: usize) {
    let metrics = get_metrics();
    let size_px = match size_idx { 0 => metrics.font_caption, 1 => metrics.font_body, 2 => metrics.font_title, _ => metrics.font_heading };
    let weight = match size_idx { 0 | 1 => FontWeight::Regular, 2 => FontWeight::Medium, _ => FontWeight::Bold };
    let font_mgr = unsafe { crate::font::get_font_manager() };
    font_mgr.draw_text_clipped(fb, x, y, max_w, text, color, size_px, weight);
}

pub fn draw_cursor(fb: &mut FrameBuffer, mx: usize, my: usize) {
    for cy in 0..22 {
        let py = my + cy; if py >= fb.height { break; }
        for cx in 0..15 {
            let px = mx + cx; if px >= fb.width { break; }
            if cx <= (cy * 2) / 3 && (cx + cy) < 24 {
                let is_border = cx == 0 || cx == (cy * 2) / 3 || (cx + cy) >= 22;
                fb.pixels[py * fb.pitch_pixels + px] = if is_border { 0xFF000000 } else { 0xFFFFFFFF };
            }
        }
    }
}
