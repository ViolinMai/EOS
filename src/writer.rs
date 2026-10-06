#![allow(dead_code)]

use crate::splash_data::{SPLASH_BITMAP, SPLASH_BITMAP_W, SPLASH_BITMAP_H};

pub const FONT_WIDTH: usize = 8;
pub const FONT_HEIGHT: usize = 16;

pub struct FramebufferWriter {
    pub buffer: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub scale: usize,
}

pub static mut WRITER: Option<FramebufferWriter> = None;

impl FramebufferWriter {
    pub fn new(buffer: *mut u8, width: usize, height: usize, pitch: usize) -> Self {
        Self {
            buffer,
            width,
            height,
            pitch,
            cursor_x: 24,
            cursor_y: 24,
            scale: 2,
        }
    }

    #[inline(always)]
    pub fn put_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height { return; }
        unsafe {
            let offset = (y * self.pitch) + (x * 4);
            let pixel_ptr = self.buffer.add(offset) as *mut u32;
            *pixel_ptr = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }
    }

    pub fn draw_filled_circle(&mut self, cx: isize, cy: isize, radius: isize, r: u8, g: u8, b: u8) {
        let r_sq = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= r_sq {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && px < self.width as isize && py >= 0 && py < self.height as isize {
                        self.put_pixel(px as usize, py as usize, r, g, b);
                    }
                }
            }
        }
    }

    pub fn render_boot_splash(&mut self, step: usize) {
        unsafe {
            let slice = core::slice::from_raw_parts_mut(self.buffer as *mut u32, (self.pitch / 4) * self.height);
            slice.fill(0xFF000000);
        }

        // تمركز مرن لأي أبعاد للصورة (العرض والارتفاع مستقلان تماماً)
        let bmp_w = SPLASH_BITMAP_W;
        let bmp_h = SPLASH_BITMAP_H;

        let start_x = ((self.width.saturating_sub(bmp_w)) / 2) as isize;
        let start_y = (((self.height.saturating_sub(bmp_h)) / 2).saturating_sub(40)) as isize;

        let bytes_per_row = (bmp_w + 7) / 8;
        for y in 0..bmp_h {
            let py = start_y + y as isize;
            for bx in 0..bytes_per_row {
                let byte_idx = y * bytes_per_row + bx;
                if byte_idx >= SPLASH_BITMAP.len() { break; }
                let byte_val = SPLASH_BITMAP[byte_idx];
                if byte_val == 0 { continue; }
                for bit in 0..8 {
                    if (byte_val & (1 << (7 - bit))) != 0 {
                        let px = start_x + (bx * 8 + bit) as isize;
                        if px >= 0 && px < self.width as isize && py >= 0 && py < self.height as isize {
                            self.put_pixel(px as usize, py as usize, 255, 255, 255);
                        }
                    }
                }
            }
        }

        // نقاط التحميل الثلاث أسفل الصورة بنمط متناسق
        let dot_y = start_y + bmp_h as isize + 36;
        let mid_x = (self.width / 2) as isize;
        let spacing = 28;
        for i in 0..3 {
            let dx = mid_x + ((i as isize - 1) * spacing);
            if i == (step % 3) {
                self.draw_filled_circle(dx, dot_y, 6, 255, 255, 255);
            } else {
                self.draw_filled_circle(dx, dot_y, 4, 80, 80, 95);
            }
        }
    }
}
