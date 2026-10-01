#![allow(dead_code)]
use core::fmt;

pub const FONT_WIDTH: usize = 8;
pub const FONT_HEIGHT: usize = 16;

// توسيع المخزن المؤقت ليتسع لدقة 2K (2560x1440) و 4K بأمان
const SAVED_SCREEN_SIZE: usize = 3840 * 2160;
static mut SCREEN_SAVE_BUFFER: [u32; SAVED_SCREEN_SIZE] = [0; SAVED_SCREEN_SIZE];
static mut SCREEN_SAVED: bool = false;

pub struct FramebufferWriter {
    pub buffer: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub scale: usize,
    pub mouse_prev_x: usize,
    pub mouse_prev_y: usize,
    pub mouse_saved_pixels: [u32; 32 * 32],
    pub mouse_drawn: bool,
    pub select_start: Option<(usize, usize)>,
    pub select_end: Option<(usize, usize)>,
}

pub type FrameWriter = FramebufferWriter;
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
            mouse_prev_x: 0,
            mouse_prev_y: 0,
            mouse_saved_pixels: [0; 32 * 32],
            mouse_drawn: false,
            select_start: None,
            select_end: None,
        }
    }

    pub fn save_screen(&mut self) {
        unsafe {
            self.erase_mouse_cursor();
            let total = core::cmp::min(self.width * self.height, SAVED_SCREEN_SIZE);
            let fb_ptr = self.buffer as *const u32;
            for i in 0..total {
                SCREEN_SAVE_BUFFER[i] = *fb_ptr.add(i);
            }
            SCREEN_SAVED = true;
        }
    }

    pub fn restore_screen(&mut self) {
        unsafe {
            if SCREEN_SAVED {
                let total = core::cmp::min(self.width * self.height, SAVED_SCREEN_SIZE);
                let fb_ptr = self.buffer as *mut u32;
                for i in 0..total {
                    *fb_ptr.add(i) = SCREEN_SAVE_BUFFER[i];
                }
                SCREEN_SAVED = false;
                self.mouse_drawn = false;
            }
        }
    }

    #[inline]
    pub fn put_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height { return; }
        unsafe {
            let offset = (y * self.pitch) + (x * 4);
            let pixel_ptr = self.buffer.add(offset) as *mut u32;
            *pixel_ptr = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }
    }

    #[inline]
    pub fn get_pixel(&self, x: usize, y: usize) -> u32 {
        if x >= self.width || y >= self.height { return 0; }
        unsafe {
            let offset = (y * self.pitch) + (x * 4);
            let pixel_ptr = self.buffer.add(offset) as *const u32;
            *pixel_ptr
        }
    }

    pub fn erase_mouse_cursor(&mut self) {
        if self.mouse_drawn {
            for y in 0..24 {
                let sy = self.mouse_prev_y + y;
                if sy >= self.height { break; }
                for x in 0..24 {
                    let sx = self.mouse_prev_x + x;
                    if sx >= self.width { break; }
                    let old_color = self.mouse_saved_pixels[y * 24 + x];
                    let r = ((old_color >> 16) & 0xFF) as u8;
                    let g = ((old_color >> 8) & 0xFF) as u8;
                    let b = (old_color & 0xFF) as u8;
                    self.put_pixel(sx, sy, r, g, b);
                }
            }
            self.mouse_drawn = false;
        }
    }

    pub fn scroll_up(&mut self) {
        self.erase_mouse_cursor();
        let line_height = FONT_HEIGHT * self.scale;
        let bytes_per_line = self.pitch * line_height;
        let total_bytes = self.height * self.pitch;

        if total_bytes <= bytes_per_line { return; }

        unsafe {
            let src = self.buffer.add(bytes_per_line);
            let dst = self.buffer;
            let copy_bytes = total_bytes - bytes_per_line;
            core::ptr::copy(src, dst, copy_bytes);

            let bottom_ptr = self.buffer.add(copy_bytes);
            let pixel_color = ((15u32) << 16) | ((23u32) << 8) | 42u32;
            let bottom_pixels = (bytes_per_line / 4) as usize;
            let bottom_slice = core::slice::from_raw_parts_mut(bottom_ptr as *mut u32, bottom_pixels);
            for p in bottom_slice.iter_mut() {
                *p = pixel_color;
            }
        }

        if self.cursor_y >= line_height {
            self.cursor_y -= line_height;
        }
    }

    pub fn check_scroll(&mut self) {
        let line_height = FONT_HEIGHT * self.scale;
        while self.cursor_y + line_height >= self.height - 12 {
            self.scroll_up();
        }
    }

    pub fn write_char(&mut self, c: char, r: u8, g: u8, b: u8) {
        self.erase_mouse_cursor();
        let char_w = FONT_WIDTH * self.scale;
        let char_h = FONT_HEIGHT * self.scale;

        if c == '\n' {
            self.cursor_x = 24;
            self.cursor_y += char_h;
            self.check_scroll();
            return;
        }
        if c == '\r' {
            self.cursor_x = 24;
            return;
        }

        if self.cursor_x + char_w >= self.width - 24 {
            self.cursor_x = 24;
            self.cursor_y += char_h;
            self.check_scroll();
        }

        let glyph = crate::font::get_glyph(c);
        for (gy, byte) in glyph.iter().enumerate() {
            for gx in 0..8 {
                if (byte & (1 << (7 - gx))) != 0 {
                    for dy in 0..self.scale {
                        for dx in 0..self.scale {
                            let px = self.cursor_x + (gx * self.scale) + dx;
                            let py = self.cursor_y + (gy * self.scale) + dy;
                            self.put_pixel(px, py, r, g, b);
                        }
                    }
                }
            }
        }
        self.cursor_x += char_w;
    }

    pub fn write_str(&mut self, s: &str, r: u8, g: u8, b: u8) {
        for c in s.chars() {
            self.write_char(c, r, g, b);
        }
    }
}
