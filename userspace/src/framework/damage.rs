use crate::framework::widget::Rect;

pub const MAX_DAMAGE_RECTS: usize = 32;

#[derive(Clone, Debug)]
pub struct DamageRegion {
    pub rects: Vec<Rect>,
}

impl DamageRegion {
    pub fn new() -> Self {
        Self { rects: Vec::with_capacity(MAX_DAMAGE_RECTS) }
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn clear(&mut self) {
        self.rects.clear();
    }

    pub fn add(&mut self, mut rect: Rect) {
        if rect.w <= 0 || rect.h <= 0 { return; }

        for r in &mut self.rects {
            if r.contains_rect(&rect) {
                return;
            }
            if rect.contains_rect(r) {
                *r = rect;
                self.coalesce();
                return;
            }
            if r.intersects(&rect) {
                let u = r.union(&rect);
                let sum_area = (r.w as i64 * r.h as i64) + (rect.w as i64 * rect.h as i64);
                let u_area = u.w as i64 * u.h as i64;
                if u_area <= (sum_area * 13) / 10 {
                    *r = u;
                    self.coalesce();
                    return;
                }
            }
        }

        if self.rects.len() < MAX_DAMAGE_RECTS {
            self.rects.push(rect);
        } else {
            let mut best_i = 0;
            let mut best_j = 1;
            let mut best_waste = i64::MAX;
            for i in 0..self.rects.len() {
                for j in (i + 1)..self.rects.len() {
                    let u = self.rects[i].union(&self.rects[j]);
                    let waste = (u.w as i64 * u.h as i64) - (self.rects[i].w as i64 * self.rects[i].h as i64) - (self.rects[j].w as i64 * self.rects[j].h as i64);
                    if waste < best_waste {
                        best_waste = waste;
                        best_i = i;
                        best_j = j;
                    }
                }
            }
            let merged = self.rects[best_i].union(&self.rects[best_j]);
            self.rects[best_i] = merged;
            self.rects.swap_remove(best_j);
            self.rects.push(rect);
        }
    }

    pub fn add_outset(&mut self, rect: Rect, outset: i32) {
        self.add(Rect::new(
            rect.x - outset,
            rect.y - outset,
            rect.w + (outset * 2),
            rect.h + (outset * 2),
        ));
    }

    pub fn clamp_to_screen(&mut self, screen_w: usize, screen_h: usize) {
        let sw = screen_w as i32;
        let sh = screen_h as i32;
        let mut i = 0;
        while i < self.rects.len() {
            let r = &mut self.rects[i];
            let x1 = r.x.clamp(0, sw);
            let y1 = r.y.clamp(0, sh);
            let x2 = (r.x + r.w).clamp(0, sw);
            let y2 = (r.y + r.h).clamp(0, sh);
            if x2 > x1 && y2 > y1 {
                r.x = x1;
                r.y = y1;
                r.w = x2 - x1;
                r.h = y2 - y1;
                i += 1;
            } else {
                self.rects.swap_remove(i);
            }
        }
    }

    fn coalesce(&mut self) {
        let mut i = 0;
        while i < self.rects.len() {
            let mut j = i + 1;
            while j < self.rects.len() {
                if self.rects[i].contains_rect(&self.rects[j]) {
                    self.rects.swap_remove(j);
                } else if self.rects[j].contains_rect(&self.rects[i]) {
                    self.rects[i] = self.rects[j];
                    self.rects.swap_remove(j);
                } else if self.rects[i].intersects(&self.rects[j]) {
                    let u = self.rects[i].union(&self.rects[j]);
                    let sum = (self.rects[i].w as i64 * self.rects[i].h as i64) + (self.rects[j].w as i64 * self.rects[j].h as i64);
                    let u_a = u.w as i64 * u.h as i64;
                    if u_a <= (sum * 12) / 10 {
                        self.rects[i] = u;
                        self.rects.swap_remove(j);
                    } else {
                        j += 1;
                    }
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
}
