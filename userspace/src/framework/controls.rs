use crate::framework::{get_theme, Canvas, Rect, Widget};
use std::any::Any;

pub struct LabelWidget {
    pub bounds: Rect,
    pub text: String,
    pub color: Option<u32>,
    pub font_size: usize,
    pub selectable: bool,
}

impl LabelWidget {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            bounds: Rect::default(),
            text: text.into(),
            color: None,
            font_size: 14,
            selectable: false,
        }
    }

    pub fn with_selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    pub fn with_size(mut self, size: usize) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }
}

impl Widget for LabelWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let color = self.color.unwrap_or(theme.text_primary);
        let (_, th) = canvas.measure_text(&self.text, self.font_size);
        let text_y = self.bounds.y + ((self.bounds.h - th as i32) / 2);
        canvas.draw_text(self.bounds.x, text_y, &self.text, color, self.font_size);
    }
}

pub struct ButtonWidget {
    pub bounds: Rect,
    pub text: String,
    pub on_click: Option<Box<dyn FnMut()>>,
    pub is_hovered: bool,
    pub is_pressed: bool,
}

impl ButtonWidget {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            bounds: Rect::default(),
            text: text.into(),
            on_click: None,
            is_hovered: false,
            is_pressed: false,
        }
    }

    pub fn on_click<F: FnMut() + 'static>(mut self, cb: F) -> Self {
        self.on_click = Some(Box::new(cb));
        self
    }
}

impl Widget for ButtonWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let bg_color = if self.is_pressed {
            theme.accent_hover
        } else if self.is_hovered {
            theme.accent_hover
        } else {
            theme.accent
        };

        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, bg_color, theme.pt(6.0) as usize);
        let (tw, th) = canvas.measure_text(&self.text, theme.font_body());
        let text_x = self.bounds.x + ((self.bounds.w - tw as i32) / 2);
        let text_y = self.bounds.y + ((self.bounds.h - th as i32) / 2);
        canvas.draw_text(text_x, text_y, &self.text, 0xFFFFFFFF, theme.font_body());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        let contains = self.bounds.contains(mx, my);
        self.is_hovered = contains;

        if contains {
            if pressed {
                self.is_pressed = true;
            } else if self.is_pressed {
                self.is_pressed = false;
                if let Some(cb) = &mut self.on_click {
                    cb();
                }
            }
            return true;
        } else {
            self.is_pressed = false;
            return false;
        }
    }
}

pub struct TextInputWidget {
    pub bounds: Rect,
    pub text: String,
    pub placeholder: String,
    pub is_focused: bool,
    pub selectable: bool,
    pub sel_all: bool,
    pub cursor_pos: usize,
    pub on_submit: Option<Box<dyn FnMut(&str)>>,
}

impl TextInputWidget {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            bounds: Rect::default(),
            text: String::new(),
            placeholder: placeholder.into(),
            is_focused: false,
            selectable: true,
            sel_all: false,
            cursor_pos: 0,
            on_submit: None,
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self.cursor_pos = self.text.len();
        self
    }

    pub fn on_submit<F: FnMut(&str) + 'static>(mut self, cb: F) -> Self {
        self.on_submit = Some(Box::new(cb));
        self
    }
}

impl Widget for TextInputWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn is_focusable(&self) -> bool { true }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_titlebar, theme.pt(6.0) as usize);

        let border_color = if self.is_focused { theme.accent } else { theme.border_window };
        canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, border_color, theme.pt(6.0) as usize);

        let pad_x = self.bounds.x + theme.pt(8.0);
        let (_, th) = canvas.measure_text("A", theme.font_body());
        let text_y = self.bounds.y + ((self.bounds.h - th as i32) / 2);

        if self.text.is_empty() {
            canvas.draw_text(pad_x, text_y, &self.placeholder, theme.text_muted, theme.font_body());
        } else {
            canvas.draw_text(pad_x, text_y, &self.text, theme.text_primary, theme.font_body());
        }

        if self.is_focused {
            let cursor_str = &self.text[..self.cursor_pos.min(self.text.len())];
            let (cw, _) = canvas.measure_text(cursor_str, theme.font_body());
            let caret_x = pad_x + cw as i32;
            canvas.draw_line_v(caret_x, text_y, th as i32, theme.accent);
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if self.bounds.contains(mx, my) {
            if pressed {
                self.is_focused = true;
                self.cursor_pos = self.text.len();
            }
            true
        } else {
            if pressed {
                self.is_focused = false;
            }
            false
        }
    }

    fn handle_key(&mut self, keycode: u8, _mods: u8) -> bool {
        if !self.is_focused { return false; }
        match keycode {
            0x1C => {
                if let Some(cb) = &mut self.on_submit {
                    let val = self.text.clone();
                    cb(&val);
                }
                true
            }
            0x4B => {
                if self.cursor_pos > 0 { self.cursor_pos -= 1; }
                true
            }
            0x4D => {
                if self.cursor_pos < self.text.len() { self.cursor_pos += 1; }
                true
            }
            _ => false
        }
    }

    fn handle_char(&mut self, c: char) -> bool {
        if !self.is_focused { return false; }
        if c == '\x08' {
            if self.cursor_pos > 0 && !self.text.is_empty() {
                self.cursor_pos -= 1;
                self.text.remove(self.cursor_pos);
            }
            return true;
        } else if c >= ' ' && c <= '~' {
            self.text.insert(self.cursor_pos, c);
            self.cursor_pos += 1;
            return true;
        }
        false
    }
}

pub struct CheckBoxWidget {
    pub bounds: Rect,
    pub label: String,
    pub checked: bool,
    pub on_toggle: Option<Box<dyn FnMut(bool)>>,
}

impl CheckBoxWidget {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            bounds: Rect::default(),
            label: label.into(),
            checked,
            on_toggle: None,
        }
    }

    pub fn on_toggle<F: FnMut(bool) + 'static>(mut self, cb: F) -> Self {
        self.on_toggle = Some(Box::new(cb));
        self
    }
}

impl Widget for CheckBoxWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let box_sz = theme.pt(18.0);
        let box_y = self.bounds.y + ((self.bounds.h - box_sz) / 2);

        if self.checked {
            canvas.draw_rect(self.bounds.x, box_y, box_sz, box_sz, theme.accent, theme.pt(4.0) as usize);
            canvas.draw_rect(self.bounds.x + theme.pt(4.0), box_y + theme.pt(4.0), (box_sz - theme.pt(8.0)).max(0), (box_sz - theme.pt(8.0)).max(0), 0xFFFFFFFF, theme.pt(2.0) as usize);
        } else {
            canvas.draw_rect_outline(self.bounds.x, box_y, box_sz, box_sz, theme.border_window, theme.pt(4.0) as usize);
        }

        let (_, th) = canvas.measure_text(&self.label, theme.font_body());
        let text_y = self.bounds.y + ((self.bounds.h - th as i32) / 2);
        canvas.draw_text(self.bounds.x + box_sz + theme.pt(8.0), text_y, &self.label, theme.text_primary, theme.font_body());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if self.bounds.contains(mx, my) {
            if pressed {
                self.checked = !self.checked;
                if let Some(cb) = &mut self.on_toggle {
                    cb(self.checked);
                }
            }
            return true;
        }
        false
    }
}

pub struct ProgressBarWidget {
    pub bounds: Rect,
    pub progress: f32,
}

impl ProgressBarWidget {
    pub fn new(progress: f32) -> Self {
        Self {
            bounds: Rect::default(),
            progress: progress.clamp(0.0, 1.0),
        }
    }

    pub fn set_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }
}

impl Widget for ProgressBarWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF141E2E, theme.pt(4.0) as usize);
        let fill_w = ((self.bounds.w as f32) * self.progress).round() as i32;
        if fill_w > 0 {
            canvas.draw_rect(self.bounds.x, self.bounds.y, fill_w, self.bounds.h, theme.accent, theme.pt(4.0) as usize);
        }
    }
}

pub struct ScrollViewWidget {
    pub bounds: Rect,
    pub child: Box<dyn Widget>,
    pub content_height: i32,
    pub scroll_y: i32,
}

impl ScrollViewWidget {
    pub fn new(child: Box<dyn Widget>, content_height: i32) -> Self {
        Self {
            bounds: Rect::default(),
            child,
            content_height,
            scroll_y: 0,
        }
    }
}

impl Widget for ScrollViewWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect { x, y, w, h };
        let child_y = y - self.scroll_y;
        self.child.layout(x, child_y, w, self.content_height.max(h));
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        canvas.push_clip(self.bounds);
        self.child.paint(canvas);

        if self.content_height > self.bounds.h && self.bounds.h > 0 {
            let track_h = self.bounds.h as f32;
            let thumb_h = ((track_h / self.content_height as f32) * track_h).max(20.0);
            let max_scroll = (self.content_height - self.bounds.h).max(1) as f32;
            let thumb_y = self.bounds.y as f32 + ((self.scroll_y as f32 / max_scroll) * (track_h - thumb_h));

            let bar_w = 4;
            let bar_x = self.bounds.x + self.bounds.w - bar_w - 3;
            canvas.draw_rect(bar_x, thumb_y as i32, bar_w, thumb_h as i32, 0x8894A3B8, 2);
        }

        canvas.pop_clip();
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        self.child.handle_mouse(mx, my, pressed)
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let max_scroll = (self.content_height - self.bounds.h).max(0);
        if dy > 0 {
            self.scroll_y = (self.scroll_y + 32).min(max_scroll);
        } else {
            self.scroll_y = (self.scroll_y - 32).max(0);
        }
        self.layout(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h);
        true
    }

    fn handle_key(&mut self, keycode: u8, mods: u8) -> bool {
        self.child.handle_key(keycode, mods)
    }

    fn handle_char(&mut self, c: char) -> bool {
        self.child.handle_char(c)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFitMode {
    /// يحافظ على نسبة العرض للارتفاع كاملة داخل الإطار بدون قص أو مط
    Contain,
    /// يملأ الإطار بالكامل مع قص الأطراف الزائدة دون تشويه الأبعاد
    Cover,
}

/// عنصر عرض صور معياري في الـ Framework مع الحفاظ الصحيح على نسبة الأبعاد (Upstream ImageWidget)
#[derive(Clone)]
pub struct ImageWidget {
    pub bounds: Rect,
    pub pixels: Vec<u32>,
    pub img_w: usize,
    pub img_h: usize,
    pub fit: ImageFitMode,
}

impl ImageWidget {
    pub fn new(pixels: Vec<u32>, img_w: usize, img_h: usize) -> Self {
        Self { bounds: Rect::default(), pixels, img_w, img_h, fit: ImageFitMode::Contain }
    }

    pub fn new_cover(pixels: Vec<u32>, img_w: usize, img_h: usize) -> Self {
        Self { bounds: Rect::default(), pixels, img_w, img_h, fit: ImageFitMode::Cover }
    }
}

impl Widget for ImageWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }
    fn paint(&self, canvas: &mut Canvas) {
        if self.img_w == 0 || self.img_h == 0 || self.pixels.is_empty() || self.bounds.w <= 0 || self.bounds.h <= 0 {
            return;
        }
        let clip = canvas.current_clip();
        let bw = self.bounds.w as f32;
        let bh = self.bounds.h as f32;
        let iw = self.img_w as f32;
        let ih = self.img_h as f32;

        let (draw_x, draw_y, draw_w, draw_h, src_x0, src_y0, src_w, src_h) = match self.fit {
            ImageFitMode::Contain => {
                let scale = (bw / iw).min(bh / ih);
                let dw = (iw * scale).round().max(1.0) as i32;
                let dh = (ih * scale).round().max(1.0) as i32;
                let dx = self.bounds.x + (self.bounds.w - dw) / 2;
                let dy = self.bounds.y + (self.bounds.h - dh) / 2;
                (dx, dy, dw, dh, 0.0f32, 0.0f32, iw, ih)
            }
            ImageFitMode::Cover => {
                let scale = (bw / iw).max(bh / ih);
                let crop_w = (bw / scale).min(iw);
                let crop_h = (bh / scale).min(ih);
                let sx0 = (iw - crop_w) * 0.5;
                let sy0 = (ih - crop_h) * 0.5;
                (self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, sx0, sy0, crop_w, crop_h)
            }
        };

        let target_rect = Rect::new(draw_x, draw_y, draw_w, draw_h).intersect(&clip);
        if target_rect.w <= 0 || target_rect.h <= 0 { return; }

        let x1 = target_rect.x.max(0);
        let y1 = target_rect.y.max(0);
        let x2 = (target_rect.x + target_rect.w).min(canvas.width as i32);
        let y2 = (target_rect.y + target_rect.h).min(canvas.height as i32);

        for py in y1..y2 {
            let v = (py - draw_y) as f32 / (draw_h.max(1) as f32);
            let sy = (src_y0 + v * src_h) as usize;
            let src_row = sy.min(self.img_h - 1) * self.img_w;
            let dst_row = (py as usize) * canvas.width;
            for px in x1..x2 {
                let u = (px - draw_x) as f32 / (draw_w.max(1) as f32);
                let sx = (src_x0 + u * src_w) as usize;
                let fg = self.pixels[src_row + sx.min(self.img_w - 1)];
                let a = (fg >> 24) & 0xFF;
                let idx = dst_row + (px as usize);
                if a == 255 {
                    canvas.buffer[idx] = fg;
                } else if a > 0 {
                    let bg = canvas.buffer[idx];
                    let inv = 255 - a;
                    let rb = (((fg & 0x00FF00FF) * a + (bg & 0x00FF00FF) * inv + 0x00800080) >> 8) & 0x00FF00FF;
                    let g = (((fg & 0x0000FF00) * a + (bg & 0x0000FF00) * inv + 0x00008000) >> 8) & 0x0000FF00;
                    canvas.buffer[idx] = 0xFF000000 | rb | g;
                }
            }
        }
    }
}

/// شريط تمرير ناعم بأنميشن وتوسّع عند التمرير (Upstream SmoothScrollBar)
pub struct SmoothScrollBar {
    pub value: f32,
    pub target: f32,
    pub hovered: bool,
}

impl SmoothScrollBar {
    pub fn new() -> Self { Self { value: 0.0, target: 0.0, hovered: false } }
    pub fn scroll_by(&mut self, delta: f32) { self.target = (self.target + delta).clamp(0.0, 1.0); }
    pub fn tick(&mut self) -> bool {
        let diff = self.target - self.value;
        if diff.abs() > 0.002 {
            self.value += diff * 0.22;
            true
        } else {
            self.value = self.target;
            false
        }
    }
    pub fn paint(&self, canvas: &mut Canvas, x: i32, y: i32, h: i32) {
        let theme = get_theme();
        let w = if self.hovered { theme.pt(10.0) } else { theme.pt(6.0) };
        let thumb_h = (h / 4).max(theme.pt(24.0));
        let thumb_y = y + ((h - thumb_h) as f32 * self.value) as i32;
        canvas.draw_rect(x, y, w, h, 0x331E293B, (w / 2) as usize);
        let col = if self.hovered { theme.accent_hover } else { theme.accent };
        canvas.draw_rect(x, thumb_y, w, thumb_h, col, (w / 2) as usize);
    }
}
