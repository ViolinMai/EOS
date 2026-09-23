use core::fmt;

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
    pub select_start_x: Option<usize>,
    pub select_end_x: Option<usize>,
    pub mouse_prev_x: usize,
    pub mouse_prev_y: usize,
    pub mouse_saved_pixels: [u32; 16 * 16],
    pub mouse_drawn: bool,
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
            select_start_x: None,
            select_end_x: None,
            mouse_prev_x: 0,
            mouse_prev_y: 0,
            mouse_saved_pixels: [0; 16 * 16],
            mouse_drawn: false,
        }
    }

    pub fn put_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        unsafe {
            let offset = (y * self.pitch) + (x * 4);
            let pixel_ptr = self.buffer.add(offset) as *mut u32;
            *pixel_ptr = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }
    }

    pub fn get_pixel(&self, x: usize, y: usize) -> u32 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        unsafe {
            let offset = (y * self.pitch) + (x * 4);
            let pixel_ptr = self.buffer.add(offset) as *const u32;
            *pixel_ptr
        }
    }

    pub fn scroll_up(&mut self) {
        let line_height = FONT_HEIGHT * self.scale;
        let bytes_per_line = self.pitch * line_height;
        let total_bytes = self.height * self.pitch;

        if total_bytes <= bytes_per_line {
            return;
        }

        unsafe {
            self.mouse_drawn = false;
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

    // محرك المزج اللوني الحقيقي (True Alpha Blending)
    pub fn blit_buffer_alpha(&mut self, buf: &[u32], dest_x: usize, dest_y: usize, w: usize, h: usize) {
        for y in 0..h {
            let screen_y = dest_y + y;
            if screen_y >= self.height {
                break;
            }
            for x in 0..w {
                let screen_x = dest_x + x;
                if screen_x >= self.width {
                    break;
                }

                let pixel = buf[y * w + x];
                let alpha = ((pixel >> 24) & 0xFF) as u32;

                if alpha == 0 {
                    continue;
                }

                let src_r = ((pixel >> 16) & 0xFF) as u32;
                let src_g = ((pixel >> 8) & 0xFF) as u32;
                let src_b = (pixel & 0xFF) as u32;

                if alpha >= 255 {
                    self.put_pixel(screen_x, screen_y, src_r as u8, src_g as u8, src_b as u8);
                } else {
                    let dst_pixel = self.get_pixel(screen_x, screen_y);
                    let dst_r = (dst_pixel >> 16) & 0xFF;
                    let dst_g = (dst_pixel >> 8) & 0xFF;
                    let dst_b = dst_pixel & 0xFF;

                    let inv_a = 255 - alpha;
                    let out_r = ((src_r * alpha + dst_r * inv_a) / 255) as u8;
                    let out_g = ((src_g * alpha + dst_g * inv_a) / 255) as u8;
                    let out_b = ((src_b * alpha + dst_b * inv_a) / 255) as u8;

                    self.put_pixel(screen_x, screen_y, out_r, out_g, out_b);
                }
            }
        }
    }

    pub fn update_mouse_cursor(
        &mut self,
        new_x: usize,
        new_y: usize,
        _left_click: bool,
        _current_input: &str,
        _cursor_idx: usize,
    ) {
        if self.mouse_drawn {
            for y in 0..16 {
                let sy = self.mouse_prev_y + y;
                if sy >= self.height {
                    break;
                }
                for x in 0..16 {
                    let sx = self.mouse_prev_x + x;
                    if sx >= self.width {
                        break;
                    }
                    let old_color = self.mouse_saved_pixels[y * 16 + x];
                    let r = ((old_color >> 16) & 0xFF) as u8;
                    let g = ((old_color >> 8) & 0xFF) as u8;
                    let b = (old_color & 0xFF) as u8;
                    self.put_pixel(sx, sy, r, g, b);
                }
            }
        }

        self.mouse_prev_x = new_x;
        self.mouse_prev_y = new_y;

        for y in 0..16 {
            let sy = new_y + y;
            if sy >= self.height {
                break;
            }
            for x in 0..16 {
                let sx = new_x + x;
                if sx >= self.width {
                    break;
                }
                self.mouse_saved_pixels[y * 16 + x] = self.get_pixel(sx, sy);

                if x <= y && (x + y) < 18 {
                    if x == 0 || x == y || (x + y) >= 16 {
                        self.put_pixel(sx, sy, 0, 0, 0);
                    } else {
                        self.put_pixel(sx, sy, 255, 255, 255);
                    }
                }
            }
        }
        self.mouse_drawn = true;
    }

    pub fn write_char(&mut self, c: char, r: u8, g: u8, b: u8) {
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

    pub fn write_fmt(&mut self, args: fmt::Arguments, r: u8, g: u8, b: u8) {
        struct WriterColorWrapper<'a> {
            w: &'a mut FramebufferWriter,
            r: u8,
            g: u8,
            b: u8,
        }
        impl<'a> fmt::Write for WriterColorWrapper<'a> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.w.write_str(s, self.r, self.g, self.b);
                Ok(())
            }
        }
        let mut wrapper = WriterColorWrapper { w: self, r, g, b };
        let _ = fmt::write(&mut wrapper, args);
    }

    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.put_pixel(x, y, r, g, b);
            }
        }
        self.cursor_x = 24;
        self.cursor_y = 24;
        self.mouse_drawn = false;
    }

    pub fn redraw_line_text(&mut self, text: &str, cursor_idx: usize) {
        self.clear_current_line(5);
        let char_w = FONT_WIDTH * self.scale;
        self.cursor_x = 24 + (5 * char_w);
        self.write_str(text, 248, 250, 252);
        self.cursor_x = 24 + (5 * char_w) + (cursor_idx * char_w);
        self.draw_cursor(true);
    }

    pub fn clear_current_line(&mut self, skip_chars: usize) {
        let char_w = FONT_WIDTH * self.scale;
        let char_h = FONT_HEIGHT * self.scale;
        let start_x = 24 + (skip_chars * char_w);
        for y in self.cursor_y..(self.cursor_y + char_h) {
            for x in start_x..self.width {
                self.put_pixel(x, y, 15, 23, 42);
            }
        }
    }

    pub fn draw_cursor(&mut self, visible: bool) {
        let char_w = FONT_WIDTH * self.scale;
        let char_h = FONT_HEIGHT * self.scale;
        let (r, g, b) = if visible { (56, 189, 248) } else { (15, 23, 42) };
        for y in (self.cursor_y + char_h - 2)..(self.cursor_y + char_h) {
            for x in self.cursor_x..(self.cursor_x + char_w) {
                self.put_pixel(x, y, r, g, b);
            }
        }
    }
}
