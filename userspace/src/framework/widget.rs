use crate::framework::canvas::Canvas;
use std::any::Any;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }
}

pub trait Widget: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect;
    fn paint(&self, canvas: &mut Canvas);
    fn handle_mouse(&mut self, _mx: i32, _my: i32, _pressed: bool) -> bool { false }
    fn handle_scroll(&mut self, _mx: i32, _my: i32, _dy: i32) -> bool { false }
    fn handle_key(&mut self, _keycode: u8, _mods: u8) -> bool { false }
    fn handle_char(&mut self, _c: char) -> bool { false }
    fn is_focusable(&self) -> bool { false }
}

pub struct WindowFrame {
    pub title: String,
    pub bounds: Rect,
    pub content: Box<dyn Widget>,
    pub is_closed: bool,
    pub is_active: bool,
    pub is_minimized: bool,
    pub is_maximized: bool,
    pub saved_bounds: Rect,
    pub is_dragging: bool,
    pub drag_offset_x: i32,
    pub drag_offset_y: i32,
    pub last_title_click_tick: u64,
    pub hover_close: bool,
    pub hover_min: bool,
    pub hover_max: bool,
}

impl WindowFrame {
    pub fn new(
        title: impl Into<String>,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        mut content: Box<dyn Widget>,
    ) -> Self {
        let bounds = Rect::new(x, y, w, h);
        let theme = crate::framework::get_theme();
        let tb_h = theme.pt(32.0);
        let content_y = y + tb_h + 1;
        let content_h = (h - tb_h - 1).max(0);
        content.layout(x, content_y, w, content_h);

        Self {
            title: title.into(),
            bounds,
            content,
            is_closed: false,
            is_active: true,
            is_minimized: false,
            is_maximized: false,
            saved_bounds: bounds,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            last_title_click_tick: 0,
            hover_close: false,
            hover_min: false,
            hover_max: false,
        }
    }

    pub fn toggle_maximize(&mut self) {
        let theme = crate::framework::get_theme();
        if self.is_maximized {
            self.layout(self.saved_bounds.x, self.saved_bounds.y, self.saved_bounds.w, self.saved_bounds.h);
            self.is_maximized = false;
        } else {
            self.saved_bounds = self.bounds;
            self.layout(0, theme.pt(32.0), 1920, 1080 - theme.pt(32.0) - theme.pt(70.0));
            self.is_maximized = true;
        }
    }
}

impl Widget for WindowFrame {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        let theme = crate::framework::get_theme();
        let tb_h = theme.pt(32.0);
        let content_y = self.bounds.y + tb_h + 1;
        let content_h = (self.bounds.h - tb_h - 1).max(0);
        self.content.layout(self.bounds.x, content_y, self.bounds.w, content_h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        if self.is_closed || self.is_minimized { return; }

        let theme = crate::framework::get_theme();
        let tb_h = theme.pt(32.0);
        let cr = if self.is_maximized { 0 } else { theme.pt(10.0) };

        // جسم النافذة وشريط العنوان الشفاف الزجاجي
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_window, cr as usize);
        canvas.draw_frosted_glass_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, 0xDD1E293B, cr as usize);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);

        let border_col = if self.is_active { theme.accent } else { theme.border_window };
        if !self.is_maximized {
            canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, border_col, cr as usize);
        }

        let btn_sz = theme.pt(14.0);
        let by = self.bounds.y + (tb_h - btn_sz) / 2;

        // 1. زر الإغلاق: دائري بلون رمادي/داكن، وعند الـ Hover يتحول للأحمر
        let close_c = if self.hover_close { 0xFFEF4444 } else { 0x6694A3B8 };
        canvas.draw_rect(self.bounds.x + theme.pt(14.0), by, btn_sz, btn_sz, close_c, (btn_sz / 2) as usize);

        // 2. زر التصغير: علامة '-'
        let min_c = if self.hover_min { 0xFFE2E8F0 } else { 0xFF94A3B8 };
        let min_bx = self.bounds.x + theme.pt(36.0);
        canvas.draw_rect(min_bx, by, btn_sz, btn_sz, 0x44334155, theme.pt(3.0) as usize);
        let my_mid = by + (btn_sz / 2);
        canvas.draw_line_h(min_bx + theme.pt(3.0), my_mid, btn_sz - theme.pt(6.0), min_c);

        // 3. زر التكبير/الشاشة الكاملة: علامة '+'
        let max_c = if self.hover_max { 0xFFE2E8F0 } else { 0xFF94A3B8 };
        let max_bx = self.bounds.x + theme.pt(58.0);
        canvas.draw_rect(max_bx, by, btn_sz, btn_sz, 0x44334155, theme.pt(3.0) as usize);
        let mx_mid = max_bx + (btn_sz / 2);
        canvas.draw_line_h(max_bx + theme.pt(3.0), my_mid, btn_sz - theme.pt(6.0), max_c);
        canvas.draw_line_v(mx_mid, by + theme.pt(3.0), btn_sz - theme.pt(6.0), max_c);

        let tx = self.bounds.x + theme.pt(84.0);
        let (_, th) = canvas.measure_text(&self.title, theme.font_title());
        let ty = self.bounds.y + (tb_h - (th as i32)) / 2;
        canvas.draw_text_clipped(tx, ty, (self.bounds.w - theme.pt(170.0)).max(0), &self.title, theme.text_primary, theme.font_title());

        let content_rect = Rect {
            x: self.bounds.x,
            y: self.bounds.y + tb_h + 1,
            w: self.bounds.w,
            h: (self.bounds.h - tb_h - 1).max(0),
        };

        canvas.push_clip(content_rect);
        self.content.paint(canvas);
        canvas.pop_clip();
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if self.is_closed || self.is_minimized { return false; }
        let theme = crate::framework::get_theme();
        let tb_h = theme.pt(32.0);
        let btn_sz = theme.pt(14.0);
        let by = self.bounds.y + (tb_h - btn_sz) / 2;

        let in_close = mx >= self.bounds.x + theme.pt(14.0) && mx <= self.bounds.x + theme.pt(14.0) + btn_sz && my >= by && my <= by + btn_sz;
        let in_min = mx >= self.bounds.x + theme.pt(36.0) && mx <= self.bounds.x + theme.pt(36.0) + btn_sz && my >= by && my <= by + btn_sz;
        let in_max = mx >= self.bounds.x + theme.pt(58.0) && mx <= self.bounds.x + theme.pt(58.0) + btn_sz && my >= by && my <= by + btn_sz;

        self.hover_close = in_close;
        self.hover_min = in_min;
        self.hover_max = in_max;

        if pressed {
            if in_close {
                self.is_closed = true;
                return true;
            }
            if in_min {
                self.is_minimized = true;
                return true;
            }
            if in_max {
                self.toggle_maximize();
                return true;
            }

            if mx >= self.bounds.x && mx <= self.bounds.x + self.bounds.w && my >= self.bounds.y && my < self.bounds.y + tb_h {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
                if now.saturating_sub(self.last_title_click_tick) < 350 {
                    self.toggle_maximize();
                    self.last_title_click_tick = 0;
                    self.is_dragging = false;
                    return true;
                }
                self.last_title_click_tick = now;

                if !self.is_maximized {
                    self.is_dragging = true;
                    self.drag_offset_x = mx - self.bounds.x;
                    self.drag_offset_y = my - self.bounds.y;
                }
                return true;
            }
        }

        if self.bounds.contains(mx, my) {
            let _ = self.content.handle_mouse(mx, my, pressed);
            return true;
        }

        false
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if self.is_closed || self.is_minimized { return false; }
        if !self.bounds.contains(mx, my) { return false; }
        self.content.handle_scroll(mx, my, dy);
        true
    }

    fn handle_key(&mut self, keycode: u8, mods: u8) -> bool {
        if self.is_closed || self.is_minimized { return false; }
        self.content.handle_key(keycode, mods)
    }

    fn handle_char(&mut self, c: char) -> bool {
        if self.is_closed || self.is_minimized { return false; }
        self.content.handle_char(c)
    }
}
