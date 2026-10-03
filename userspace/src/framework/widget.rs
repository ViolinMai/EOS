use std::any::Any;
use super::canvas::Canvas;
use super::theme::get_theme;
use crate::f_info;

#[derive(Clone, Copy, Default, Debug)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Rect {
    pub fn new(x: usize, y: usize, w: usize, h: usize) -> Self { Self { x, y, w, h } }
    #[inline(always)]
    pub fn contains(&self, px: usize, py: usize) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

pub trait Widget: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect;
    fn paint(&self, canvas: &mut Canvas);
    fn handle_mouse(&mut self, _mx: usize, _my: usize, _pressed: bool) -> bool { false }
    fn handle_char(&mut self, _c: char) -> bool { false }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ResizeEdge {
    None,
    Right,
    Bottom,
    BottomRight,
}

pub struct WindowFrame {
    pub title: String,
    pub bounds: Rect,
    pub is_dragging: bool,
    pub is_resizing: ResizeEdge,
    pub drag_offset_x: usize,
    pub drag_offset_y: usize,
    pub is_closed: bool,
    pub content: Box<dyn Widget>,
}

impl WindowFrame {
    pub fn new(title: impl Into<String>, x: usize, y: usize, w: usize, h: usize, mut content: Box<dyn Widget>) -> Self {
        let theme = get_theme();
        let tb_h = theme.pt(36.0);
        content.layout(x, y + tb_h, w, h.saturating_sub(tb_h));

        Self {
            title: title.into(),
            bounds: Rect::new(x, y, w, h),
            is_dragging: false,
            is_resizing: ResizeEdge::None,
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

    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        let theme = get_theme();
        let tb_h = theme.pt(36.0);
        self.content.layout(x, y + tb_h, w, h.saturating_sub(tb_h));
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        if self.is_closed { return; }
        let theme = get_theme();
        let tb_h = theme.pt(36.0);
        let r = theme.radius_window;

        // Window Background & Clean Rounded Outline
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_window, r);
        canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.border_window, r);

        // Titlebar Top
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, r);
        canvas.draw_rect(self.bounds.x, self.bounds.y + tb_h.saturating_sub(r), self.bounds.w, r, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);

        // Generous Window Controls
        let btn_sz = theme.pt(15.0);
        let btn_y = self.bounds.y + (tb_h.saturating_sub(btn_sz) / 2);
        let btn_rad = btn_sz / 2;

        let c_x = self.bounds.x + theme.pt(16.0);
        let m_x = c_x + btn_sz + theme.pt(8.0);
        let x_x = m_x + btn_sz + theme.pt(8.0);

        canvas.draw_rect(c_x, btn_y, btn_sz, btn_sz, theme.btn_close, btn_rad);
        canvas.draw_rect(m_x, btn_y, btn_sz, btn_sz, theme.btn_min, btn_rad);
        canvas.draw_rect(x_x, btn_y, btn_sz, btn_sz, theme.btn_max, btn_rad);

        // Title Text with Ellipsis protection
        let (tw, _) = canvas.measure_text(&self.title, theme.font_title());
        let tx = self.bounds.x + (self.bounds.w.saturating_sub(tw) / 2).max(theme.pt(80.0));
        let ty = self.bounds.y + (tb_h.saturating_sub(theme.font_title()) / 2);
        canvas.draw_text_clipped(tx, ty, self.bounds.w.saturating_sub(theme.pt(160.0)), &self.title, theme.text_primary, theme.font_title());

        // STRICT SCISSOR CLIPPING: Content can NEVER escape this window's bounds
        let old_clip = canvas.clip_rect;
        let content_rect = Rect::new(
            self.bounds.x + 1,
            self.bounds.y + tb_h + 1,
            self.bounds.w.saturating_sub(2),
            self.bounds.h.saturating_sub(tb_h + 2),
        );
        canvas.intersect_clip(content_rect);
        self.content.paint(canvas);
        canvas.set_clip(old_clip);

        // Corner Resize Grip Marker
        let grip_sz = theme.pt(8.0);
        let gx = self.bounds.x + self.bounds.w.saturating_sub(grip_sz + 2);
        let gy = self.bounds.y + self.bounds.h.saturating_sub(grip_sz + 2);
        canvas.draw_rect(gx, gy, grip_sz, 2, theme.text_muted, 0);
        canvas.draw_rect(gx + 3, gy + 3, grip_sz - 3, 2, theme.text_muted, 0);
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if self.is_closed { return false; }

        if self.is_dragging {
            if !pressed {
                self.is_dragging = false;
            } else {
                let nx = mx.saturating_sub(self.drag_offset_x);
                let ny = my.saturating_sub(self.drag_offset_y);
                self.layout(nx, ny, self.bounds.w, self.bounds.h);
            }
            return true;
        }

        if self.is_resizing != ResizeEdge::None {
            if !pressed {
                self.is_resizing = ResizeEdge::None;
                f_info!("WINDOW", "Window '{}' resize finished ({}x{}).", self.title, self.bounds.w, self.bounds.h);
            } else {
                let theme = get_theme();
                let min_w = theme.pt(320.0);
                let min_h = theme.pt(200.0);
                match self.is_resizing {
                    ResizeEdge::BottomRight => {
                        let nw = (mx.saturating_sub(self.bounds.x)).max(min_w);
                        let nh = (my.saturating_sub(self.bounds.y)).max(min_h);
                        self.layout(self.bounds.x, self.bounds.y, nw, nh);
                    }
                    ResizeEdge::Right => {
                        let nw = (mx.saturating_sub(self.bounds.x)).max(min_w);
                        self.layout(self.bounds.x, self.bounds.y, nw, self.bounds.h);
                    }
                    ResizeEdge::Bottom => {
                        let nh = (my.saturating_sub(self.bounds.y)).max(min_h);
                        self.layout(self.bounds.x, self.bounds.y, self.bounds.w, nh);
                    }
                    ResizeEdge::None => {}
                }
            }
            return true;
        }

        if !self.bounds.contains(mx, my) { return false; }

        let theme = get_theme();
        let tb_h = theme.pt(36.0);
        let resize_margin = theme.pt(12.0);

        // Check if cursor clicked on resize border
        if pressed {
            let on_right = mx >= self.bounds.x + self.bounds.w.saturating_sub(resize_margin);
            let on_bottom = my >= self.bounds.y + self.bounds.h.saturating_sub(resize_margin);
            if on_right && on_bottom {
                self.is_resizing = ResizeEdge::BottomRight;
                return true;
            } else if on_right {
                self.is_resizing = ResizeEdge::Right;
                return true;
            } else if on_bottom {
                self.is_resizing = ResizeEdge::Bottom;
                return true;
            }
        }

        // Titlebar controls & dragging
        if my < self.bounds.y + tb_h {
            let close_hitbox = Rect::new(self.bounds.x + theme.pt(8.0), self.bounds.y, theme.pt(30.0), tb_h);
            let min_hitbox   = Rect::new(self.bounds.x + theme.pt(38.0), self.bounds.y, theme.pt(28.0), tb_h);

            if pressed && close_hitbox.contains(mx, my) {
                f_info!("WINDOW", "Window '{}' closed via button.", self.title);
                self.is_closed = true;
                return true;
            }

            if pressed && min_hitbox.contains(mx, my) {
                f_info!("WINDOW", "Window '{}' minimized.", self.title);
                self.is_closed = true;
                return true;
            }

            if pressed {
                self.is_dragging = true;
                self.drag_offset_x = mx.saturating_sub(self.bounds.x);
                self.drag_offset_y = my.saturating_sub(self.bounds.y);
            }
            return true;
        }

        let _ = self.content.handle_mouse(mx, my, pressed);
        true
    }

    fn handle_char(&mut self, c: char) -> bool {
        if self.is_closed { return false; }
        self.content.handle_char(c)
    }
}
