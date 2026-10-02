mod syscall;
mod sdk;

use std::collections::BTreeMap;
use std::fs;
use std::time::{Duration, Instant};
use std::thread;
use rusttype::{Font, Scale, point};

pub const W: usize = 960;
pub const H: usize = 640;

pub struct UserspaceFontEngine<'a> {
    font: Option<Font<'a>>,
    cache: BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>,
}

impl<'a> UserspaceFontEngine<'a> {
    pub fn new(font_data: &'a [u8]) -> Self {
        let font = Font::try_from_bytes(font_data);
        Self {
            font,
            cache: BTreeMap::new(),
        }
    }

    pub fn draw_text(&mut self, buffer: &mut [u32], pitch: usize, x: usize, y: usize, text: &str, color: u32, size: usize) {
        let font = match &self.font {
            Some(f) => f,
            None => return,
        };

        let scale = Scale::uniform(size as f32);
        let v_metrics = font.v_metrics(scale);
        let mut cur_x = x;
        let line_height = (size as f32 * 1.25) as usize;

        for c in text.chars() {
            if c == '\n' {
                cur_x = x;
                continue;
            }
            if c == '\r' { continue; }

            let key = (size, c);
            if !self.cache.contains_key(&key) {
                let glyph = font.glyph(c).scaled(scale).positioned(point(0.0, v_metrics.ascent));
                let h_metrics = glyph.unpositioned().h_metrics();
                let advance = h_metrics.advance_width.round() as usize;

                if let Some(bb) = glyph.pixel_bounding_box() {
                    let gw = bb.width() as usize;
                    let gh = bb.height() as usize;
                    let mut coverage = vec![0u8; gw * gh];
                    glyph.draw(|gx, gy, v| {
                        if (gx as usize) < gw && (gy as usize) < gh {
                            coverage[gy as usize * gw + gx as usize] = (v * 255.0) as u8;
                        }
                    });
                    self.cache.insert(key, (gw, gh, bb.min.x as isize, bb.min.y as isize, advance.max(1), coverage));
                } else {
                    self.cache.insert(key, (0, 0, 0, 0, advance.max(size / 3), Vec::new()));
                }
            }

            let (gw, gh, bx, by, adv, cov) = self.cache.get(&key).unwrap();
            let gx = (cur_x as isize + bx).max(0) as usize;
            let gy = (y as isize + by).max(0) as usize;

            for row in 0..*gh {
                let py = gy + row;
                if py >= H { break; }
                for col in 0..*gw {
                    let px = gx + col;
                    if px >= W { break; }
                    let alpha = cov[row * gw + col] as u32;
                    if alpha > 0 {
                        let idx = py * pitch + px;
                        buffer[idx] = sdk::gfx::blend(buffer[idx], color, alpha);
                    }
                }
            }
            cur_x += adv;
        }
    }
}

fn draw_rounded_box(buf: &mut [u32], x: usize, y: usize, w: usize, h: usize, color: u32) {
    let ex = (x + w).min(W);
    let ey = (y + h).min(H);
    for cy in y..ey {
        let row = cy * W;
        for cx in x..ex {
            buf[row + cx] = color;
        }
    }
}

fn main() {
    println!("🚀 Launching EOS Userspace Vector Desktop Interface (PID 2)...");
    
    // قراءة خط SF Pro من خلال نظام ملفات النواة VFS
    let font_bytes = fs::read("fonts/SFPRODISPLAYREGULAR.OTF")
        .or_else(|_| fs::read("fonts/Roboto-Regular.ttf"))
        .unwrap_or_else(|_| Vec::new());

    let mut engine = UserspaceFontEngine::new(&font_bytes);
    let mut pixels = vec![0xFF1E293B; W * H];
    let start_time = Instant::now();

    let mut mouse_x = W / 2;
    let mut mouse_y = H / 2;
    let mut click_state = false;

    loop {
        while let Some(ev) = sdk::window::poll_event() {
            let (etype, data) = (ev[0], ev[1]);
            if etype == 1 {
                mouse_x = ((data & 0xFFFFFFFF) as usize).min(W - 1);
                mouse_y = ((data >> 32) as usize).min(H - 1);
            } else if etype == 2 {
                click_state = (data >> 8) == 1;
            }
        }

        // مسح الخلفية بلون السطح الداكن
        pixels.fill(0xFF0F172A);

        // رسم كرت نافذة تفاعلي بحواف واضحة
        draw_rounded_box(&mut pixels, 40, 40, W - 80, H - 80, 0xFF1E293B);
        draw_rounded_box(&mut pixels, 40, 40, W - 80, 50, 0xFF334155);

        // تصيير النصوص المتجهية الاحترافية باستخدام RustType داخل Userspace
        engine.draw_text(&mut pixels, W, 60, 55, " EOS High-DPI Desktop Environment", 0xFFFFFFFF, 20);
        engine.draw_text(&mut pixels, W, 60, 120, "Vector Typography Rendered Safely in Userspace (Ring 3)", 0xFF38BDF8, 24);
        engine.draw_text(&mut pixels, W, 60, 160, "• Fully anti-aliased TrueType/OpenType font pipeline", 0xFFCBD5E1, 16);
        engine.draw_text(&mut pixels, W, 60, 190, "• Running on musl libc with full Rust std capabilities", 0xFFCBD5E1, 16);
        engine.draw_text(&mut pixels, W, 60, 220, "• Isolated memory safety with zero kernel floating-point overhead", 0xFFCBD5E1, 16);

        let elapsed = start_time.elapsed().as_secs();
        let status = format!("Uptime: {}s | Mouse: ({}, {}) | Click: {}", elapsed, mouse_x, mouse_y, click_state);
        engine.draw_text(&mut pixels, W, 60, H - 70, &status, 0xFF94A3B8, 15);

        // رسم مؤشر تفاعلي داخل واجهة المستخدم
        let cursor_color = if click_state { 0xFF22C55E } else { 0xFFE2E8F0 };
        draw_rounded_box(&mut pixels, mouse_x.saturating_sub(4), mouse_y.saturating_sub(4), 8, 8, cursor_color);

        // إرسال السطح المكتمل إلى النواة عبر استدعاء النظام sys_present
        sdk::window::present(pixels.as_ptr(), W, H);
        thread::sleep(Duration::from_millis(16));
    }
}
