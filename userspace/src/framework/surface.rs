use crate::framework::canvas::Canvas;
use crate::framework::widget::Rect;
use rusttype::Font;
use std::collections::BTreeMap;

pub struct WindowSurface {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
    pub is_dirty: bool,
}

impl WindowSurface {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height],
            is_dirty: true,
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.pixels.resize(width * height, 0);
            self.is_dirty = true;
        }
    }

    pub fn blit_to(
        &self,
        dest: &mut [u32],
        dest_pitch: usize,
        dest_h: usize,
        dest_x: i32,
        dest_y: i32,
        clip: &Rect,
    ) {
        let sx1 = 0.max(clip.x - dest_x);
        let sy1 = 0.max(clip.y - dest_y);
        let sx2 = (self.width as i32).min((clip.x + clip.w) - dest_x);
        let sy2 = (self.height as i32).min((clip.y + clip.h) - dest_y);

        if sx2 <= sx1 || sy2 <= sy1 { return; }

        let copy_w = (sx2 - sx1) as usize;
        for row in sy1..sy2 {
            let dy = dest_y + row;
            if dy < 0 || dy >= dest_h as i32 { continue; }
            let dx = dest_x + sx1;
            if dx < 0 || dx >= dest_pitch as i32 { continue; }

            let src_idx = (row as usize) * self.width + (sx1 as usize);
            let dst_idx = (dy as usize) * dest_pitch + (dx as usize);

            let src_slice = &self.pixels[src_idx..src_idx + copy_w];
            let dst_slice = &mut dest[dst_idx..dst_idx + copy_w];

            for (sp, dp) in src_slice.iter().zip(dst_slice.iter_mut()) {
                let s = *sp;
                let a = (s >> 24) & 0xFF;
                if a == 255 {
                    *dp = s;
                } else if a > 0 {
                    let d = *dp;
                    let inv = 255 - a;
                    let rb = ((((s & 0x00FF00FF) * a) + ((d & 0x00FF00FF) * inv) + 0x00800080) >> 8) & 0x00FF00FF;
                    let g  = ((((s & 0x0000FF00) * a) + ((d & 0x0000FF00) * inv) + 0x00008000) >> 8) & 0x0000FF00;
                    *dp = 0xFF000000 | rb | g;
                }
            }
        }
    }
}
