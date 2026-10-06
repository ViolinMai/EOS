use crate::framework::widget::Rect;

pub struct DamageRegion {
    pub rects: Vec<Rect>,
}

impl DamageRegion {
    pub fn new() -> Self {
        Self {
            rects: Vec::with_capacity(32),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn clear(&mut self) {
        self.rects.clear();
    }

    pub fn add(&mut self, rect: Rect) {
        if rect.w <= 0 || rect.h <= 0 { return; }

        for r in &self.rects {
            if r.contains_rect(&rect) {
                return;
            }
        }

        self.rects.retain(|r| !rect.contains_rect(r));

        for r in &mut self.rects {
            if r.intersects(&rect) {
                *r = r.union(&rect);
                self.coalesce();
                return;
            }
        }

        if self.rects.len() < 16 {
            self.rects.push(rect);
        } else {
            let mut best_i = 0;
            let mut best_j = 1;
            let mut min_area = i32::MAX;

            for i in 0..self.rects.len() {
                for j in (i + 1)..self.rects.len() {
                    let u = self.rects[i].union(&self.rects[j]);
                    let area = u.w * u.h;
                    if area < min_area {
                        min_area = area;
                        best_i = i;
                        best_j = j;
                    }
                }
            }

            let merged = self.rects[best_i].union(&self.rects[best_j]);
            self.rects[best_i] = merged;
            self.rects.remove(best_j);
            self.rects.push(rect);
        }
    }

    pub fn clamp_to_screen(&mut self, screen_w: usize, screen_h: usize) {
        let sw = screen_w as i32;
        let sh = screen_h as i32;
        for r in &mut self.rects {
            let x1 = r.x.clamp(0, sw);
            let y1 = r.y.clamp(0, sh);
            let x2 = (r.x + r.w).clamp(0, sw);
            let y2 = (r.y + r.h).clamp(0, sh);
            r.x = x1;
            r.y = y1;
            r.w = (x2 - x1).max(0);
            r.h = (y2 - y1).max(0);
        }
        self.rects.retain(|r| r.w > 0 && r.h > 0);
    }

    pub fn to_syscall_rects(&self) -> Vec<[u32; 4]> {
        self.rects
            .iter()
            .map(|r| [r.x as u32, r.y as u32, r.w as u32, r.h as u32])
            .collect()
    }

    fn coalesce(&mut self) {
        let mut i = 0;
        while i < self.rects.len() {
            let mut j = i + 1;
            while j < self.rects.len() {
                if self.rects[i].contains_rect(&self.rects[j]) {
                    self.rects.remove(j);
                } else if self.rects[j].contains_rect(&self.rects[i]) {
                    self.rects.swap_remove(i);
                    i = i.saturating_sub(1);
                    break;
                } else if self.rects[i].intersects(&self.rects[j]) {
                    let u = self.rects[i].union(&self.rects[j]);
                    self.rects[i] = u;
                    self.rects.remove(j);
                    j = i + 1;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
}
