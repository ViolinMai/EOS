use std::any::Any;
use super::canvas::Canvas;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Rect {
    pub fn contains(&self, px: usize, py: usize) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

pub trait Widget: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn layout(&mut self, origin_x: usize, origin_y: usize, max_w: usize, max_h: usize) -> Rect;
    fn paint(&self, canvas: &mut Canvas);
    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        let _ = (mx, my, pressed);
        false
    }
    fn handle_char(&mut self, _c: char) -> bool { false }
}

pub struct Container {
    pub bounds: Rect,
    pub color: u32,
    pub radius: usize,
    pub child: Option<Box<dyn Widget>>,
}

impl Container {
    pub fn new(color: u32, radius: usize, child: Option<Box<dyn Widget>>) -> Self {
        Self {
            bounds: Rect { x: 0, y: 0, w: 0, h: 0 },
            color,
            radius,
            child,
        }
    }
}

impl Widget for Container {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, origin_x: usize, origin_y: usize, max_w: usize, max_h: usize) -> Rect {
        self.bounds = Rect { x: origin_x, y: origin_y, w: max_w, h: max_h };
        if let Some(ref mut c) = self.child {
            c.layout(origin_x, origin_y, max_w, max_h);
        }
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, self.color, self.radius);
        if let Some(ref c) = self.child {
            c.paint(canvas);
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if let Some(ref mut c) = self.child {
            c.handle_mouse(mx, my, pressed)
        } else {
            false
        }
    }

    fn handle_char(&mut self, c: char) -> bool {
        if let Some(ref mut child) = self.child {
            child.handle_char(c)
        } else {
            false
        }
    }
}

pub struct Text {
    pub bounds: Rect,
    pub content: String,
    pub color: u32,
    pub size: usize,
}

impl Text {
    pub fn new(content: impl Into<String>, color: u32, size: usize) -> Self {
        Self {
            bounds: Rect { x: 0, y: 0, w: 0, h: 0 },
            content: content.into(),
            color,
            size,
        }
    }
}

impl Widget for Text {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, origin_x: usize, origin_y: usize, max_w: usize, max_h: usize) -> Rect {
        let approx_w = (self.content.len() * (self.size / 2)).min(max_w);
        self.bounds = Rect { x: origin_x, y: origin_y, w: approx_w, h: self.size.min(max_h) };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        canvas.draw_text(self.bounds.x, self.bounds.y, &self.content, self.color, self.size);
    }
}

pub struct Button {
    pub bounds: Rect,
    pub label: String,
    pub color_normal: u32,
    pub color_hover: u32,
    pub is_hovered: bool,
    pub is_pressed: bool,
    pub on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    pub fn new(label: impl Into<String>, on_click: impl FnMut() + 'static) -> Self {
        Self {
            bounds: Rect { x: 0, y: 0, w: 120, h: 36 },
            label: label.into(),
            color_normal: 0xFF0284C7,
            color_hover: 0xFF38BDF8,
            is_hovered: false,
            is_pressed: false,
            on_click: Some(Box::new(on_click)),
        }
    }
}

impl Widget for Button {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, origin_x: usize, origin_y: usize, max_w: usize, max_h: usize) -> Rect {
        self.bounds = Rect {
            x: origin_x,
            y: origin_y,
            w: 140.min(max_w),
            h: 36.min(max_h),
        };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let col = if self.is_hovered { self.color_hover } else { self.color_normal };
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, col, 6);
        let text_x = self.bounds.x + 18;
        let text_y = self.bounds.y + 8;
        canvas.draw_text(text_x, text_y, &self.label, 0xFFFFFFFF, 15);
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        self.is_hovered = self.bounds.contains(mx, my);
        if self.is_hovered && pressed && !self.is_pressed {
            self.is_pressed = true;
            if let Some(ref mut cb) = self.on_click {
                cb();
            }
            return true;
        }
        if !pressed {
            self.is_pressed = false;
        }
        false
    }
}

pub struct Column {
    pub bounds: Rect,
    pub children: Vec<Box<dyn Widget>>,
    pub spacing: usize,
}

impl Column {
    pub fn new(children: Vec<Box<dyn Widget>>, spacing: usize) -> Self {
        Self {
            bounds: Rect { x: 0, y: 0, w: 0, h: 0 },
            children,
            spacing,
        }
    }
}

impl Widget for Column {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, origin_x: usize, origin_y: usize, max_w: usize, max_h: usize) -> Rect {
        let mut cur_y = origin_y;
        for c in &mut self.children {
            let r = c.layout(origin_x, cur_y, max_w, max_h.saturating_sub(cur_y - origin_y));
            cur_y += r.h + self.spacing;
        }
        self.bounds = Rect { x: origin_x, y: origin_y, w: max_w, h: cur_y - origin_y };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        for c in &self.children {
            c.paint(canvas);
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        let mut handled = false;
        for c in &mut self.children {
            if c.handle_mouse(mx, my, pressed) {
                handled = true;
            }
        }
        handled
    }

    fn handle_char(&mut self, c: char) -> bool {
        let mut handled = false;
        for child in &mut self.children {
            if child.handle_char(c) {
                handled = true;
            }
        }
        handled
    }
}

pub struct WindowFrame {
    pub bounds: Rect,
    pub title: String,
    pub is_dragging: bool,
    pub drag_offset_x: usize,
    pub drag_offset_y: usize,
    pub is_closed: bool,
    pub content: Box<dyn Widget>,
}

impl WindowFrame {
    pub fn new(title: impl Into<String>, x: usize, y: usize, w: usize, h: usize, content: Box<dyn Widget>) -> Self {
        Self {
            bounds: Rect { x, y, w, h },
            title: title.into(),
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            is_closed: false,
            content,
        }
    }
}

impl Widget for WindowFrame {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, _origin_x: usize, _origin_y: usize, max_w: usize, max_h: usize) -> Rect {
        self.bounds.w = self.bounds.w.min(max_w);
        self.bounds.h = self.bounds.h.min(max_h);
        let content_y = self.bounds.y + 40;
        let content_h = self.bounds.h.saturating_sub(44);
        self.content.layout(self.bounds.x + 2, content_y, self.bounds.w.saturating_sub(4), content_h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        if self.is_closed { return; }
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF1E293B, 12);
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, 40, 0xFF334155, 12);
        canvas.draw_rect(self.bounds.x, self.bounds.y + 30, self.bounds.w, 10, 0xFF334155, 0);

        canvas.draw_rect(self.bounds.x + 14, self.bounds.y + 14, 12, 12, 0xFFFF5F56, 6);
        canvas.draw_rect(self.bounds.x + 32, self.bounds.y + 14, 12, 12, 0xFFFFBD2E, 6);
        canvas.draw_rect(self.bounds.x + 50, self.bounds.y + 14, 12, 12, 0xFF27C93F, 6);

        let title_x = self.bounds.x + 75;
        canvas.draw_text(title_x, self.bounds.y + 11, &self.title, 0xFFFFFFFF, 15);

        canvas.set_clip(self.bounds.x + 2, self.bounds.y + 40, self.bounds.w.saturating_sub(4), self.bounds.h.saturating_sub(44));
        self.content.paint(canvas);
        canvas.reset_clip();
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if self.is_closed { return false; }

        let close_rect = Rect { x: self.bounds.x + 14, y: self.bounds.y + 14, w: 12, h: 12 };
        if close_rect.contains(mx, my) && pressed {
            self.is_closed = true;
            return true;
        }

        let title_rect = Rect { x: self.bounds.x, y: self.bounds.y, w: self.bounds.w, h: 40 };
        if title_rect.contains(mx, my) {
            if pressed && !self.is_dragging {
                self.is_dragging = true;
                self.drag_offset_x = mx - self.bounds.x;
                self.drag_offset_y = my - self.bounds.y;
                return true;
            }
        }

        if !pressed {
            self.is_dragging = false;
        }

        if self.is_dragging {
            self.bounds.x = mx.saturating_sub(self.drag_offset_x).min(1920 - 100);
            self.bounds.y = my.saturating_sub(self.drag_offset_y).max(36).min(1080 - 60);
            return true;
        }

        self.content.handle_mouse(mx, my, pressed)
    }

    fn handle_char(&mut self, c: char) -> bool {
        if self.is_closed { return false; }
        self.content.handle_char(c)
    }
}
