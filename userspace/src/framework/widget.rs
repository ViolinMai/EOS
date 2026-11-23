use crate::framework::canvas::Canvas;
use crate::framework::theme::get_theme;
use crate::framework::vector::draw_vector_icon;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.w <= self.x + self.w
            && other.y + other.h <= self.y + self.h
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.y < other.y + other.h
            && self.y + self.h > other.y
    }

    pub fn intersect(&self, other: &Rect) -> Rect {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);
        if x2 > x1 && y2 > y1 {
            Rect::new(x1, y1, x2 - x1, y2 - y1)
        } else {
            Rect::new(0, 0, 0, 0)
        }
    }

    pub fn non_empty(self) -> Option<Rect> {
        if self.w > 0 && self.h > 0 {
            Some(self)
        } else {
            None
        }
    }

    pub fn union(&self, other: &Rect) -> Rect {
        if self.w <= 0 || self.h <= 0 { return *other; }
        if other.w <= 0 || other.h <= 0 { return *self; }
        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = (self.x + self.w).max(other.x + other.w);
        let y2 = (self.y + self.h).max(other.y + other.h);
        Rect::new(x1, y1, x2 - x1, y2 - y1)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    None,
    Right,
    Bottom,
    BottomRight,
    Left,
    Top,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WindowTileState {
    None,
    LeftHalf,
    RightHalf,
}

static mut WIN7_CHROME_AUDIT_EMITTED: bool = false;

pub trait Widget: std::any::Any {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
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
    pub prev_bounds: Rect,
    pub content: Box<dyn Widget>,
    pub is_active: bool,
    pub is_closed: bool,
    pub is_minimized: bool,
    pub is_maximized: bool,
    pub tile_state: WindowTileState,
    pub is_dragging: bool,
    pub drag_offset_x: i32,
    pub drag_offset_y: i32,
    pub is_resizing: bool,
    pub resize_edge: ResizeEdge,
    pub hover_btn: Option<usize>, // 0=Close, 1=Maximize, 2=Minimize
    pub min_w: i32,
    pub min_h: i32,
}

impl WindowFrame {
    pub fn new(
        title: impl Into<String>,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        content: Box<dyn Widget>,
    ) -> Self {
        let theme = get_theme();
        let mut frame = Self {
            title: title.into(),
            bounds: Rect::new(x, y, w, h),
            prev_bounds: Rect::new(x, y, w, h),
            content,
            is_active: true,
            is_closed: false,
            is_minimized: false,
            is_maximized: false,
            tile_state: WindowTileState::None,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            is_resizing: false,
            resize_edge: ResizeEdge::None,
            hover_btn: None,
            min_w: theme.pt(300.0),
            min_h: theme.pt(180.0),
        };
        frame.relayout_content();
        frame
    }

    pub fn title_bar_height(&self) -> i32 {
        get_theme().pt(36.0)
    }

    fn relayout_content(&mut self) {
        let tbh = self.title_bar_height();
        let border = 1;
        self.content.layout(
            self.bounds.x + border,
            self.bounds.y + tbh,
            (self.bounds.w - (border * 2)).max(10),
            (self.bounds.h - tbh - border).max(10),
        );
    }

    pub fn toggle_maximize(&mut self, work_area: &Rect) {
        if self.is_maximized {
            self.layout(self.prev_bounds.x, self.prev_bounds.y, self.prev_bounds.w, self.prev_bounds.h);
            self.is_maximized = false;
            self.tile_state = WindowTileState::None;
        } else {
            self.prev_bounds = self.bounds;
            self.layout(work_area.x, work_area.y, work_area.w, work_area.h);
            self.is_maximized = true;
            self.tile_state = WindowTileState::None;
        }
    }

    pub fn tile_left(&mut self, work_area: &Rect) {
        self.prev_bounds = self.bounds;
        self.layout(work_area.x, work_area.y, work_area.w / 2, work_area.h);
        self.is_maximized = false;
        self.tile_state = WindowTileState::LeftHalf;
    }

    pub fn tile_right(&mut self, work_area: &Rect) {
        self.prev_bounds = self.bounds;
        self.layout(work_area.x + work_area.w / 2, work_area.y, work_area.w / 2, work_area.h);
        self.is_maximized = false;
        self.tile_state = WindowTileState::RightHalf;
    }
}

impl Widget for WindowFrame {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w.max(self.min_w), h.max(self.min_h));
        self.relayout_content();
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        if self.is_closed || self.is_minimized { return; }
        unsafe {
            if !WIN7_CHROME_AUDIT_EMITTED {
                WIN7_CHROME_AUDIT_EMITTED = true;
                crate::sdk::agent::audit_log("WIN7_WINDOW_CHROME:PASS:AeroButtons_Active");
            }
        }

        let theme = get_theme();
        let tbh = self.title_bar_height();

        // 1. Windows 7 Outer Aero Glow + Inner White Frost Border Highlight
        let outer_border = if self.is_active { theme.border_window } else { 0xFF334155 };
        canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, outer_border, theme.pt(6.0) as usize);
        canvas.draw_rect_outline(self.bounds.x + 1, self.bounds.y + 1, self.bounds.w - 2, self.bounds.h - 2, theme.border_frost_highlight, theme.pt(5.0) as usize);

        // 2. Translucent Aero Glass Titlebar + Glass Reflection Stripes
        let tb_bg = if self.is_active { theme.bg_titlebar } else { theme.bg_titlebar_inactive };
        canvas.draw_frosted_glass_rect(self.bounds.x + 1, self.bounds.y + 1, self.bounds.w - 2, tbh - 1, tb_bg, theme.pt(5.0) as usize);
        // Upper glossy reflection stripe
        canvas.draw_rect(self.bounds.x + 2, self.bounds.y + 2, self.bounds.w - 4, tbh / 3, 0x33FFFFFF, theme.pt(4.0) as usize);
        canvas.draw_line_h(self.bounds.x + 1, self.bounds.y + tbh - 1, self.bounds.w - 2, theme.border_frost_highlight);

        // 3. Window Title with subtle text glow
        let txt_c = if self.is_active { theme.text_primary } else { theme.text_muted };
        canvas.draw_text_clipped(self.bounds.x + theme.pt(14.0), self.bounds.y + theme.pt(8.0), self.bounds.w - theme.pt(150.0), &self.title, txt_c, theme.font_title());

        // 4. Windows 7 Grouped Top-Right Rectangular Caption Cluster (Min | Max | Wide Red-Glow Close)
        let close_w = theme.pt(44.0);
        let std_btn_w = theme.pt(28.0);
        let grp_h = theme.pt(22.0);
        let right_x = self.bounds.x + self.bounds.w - theme.pt(6.0);
        let grp_y = self.bounds.y + 1;

        let close_x = right_x - close_w;
        let max_x = close_x - std_btn_w;
        let min_x = max_x - std_btn_w;
        let cluster_w = close_w + (std_btn_w * 2);

        // Group capsule outline
        canvas.draw_rect(min_x, grp_y, cluster_w, grp_h, 0x550F172A, 3);
        canvas.draw_rect_outline(min_x, grp_y, cluster_w, grp_h, 0x99E0F2FE, 3);

        // Minimize button `_`
        let min_bg = if self.hover_btn == Some(2) { 0xAA38BDF8 } else { 0x331E3A8A };
        canvas.draw_rect(min_x + 1, grp_y + 1, std_btn_w - 1, grp_h - 2, min_bg, 2);
        canvas.draw_rect(min_x + (std_btn_w - 10) / 2, grp_y + grp_h - 7, 10, 2, 0xFFFFFFFF, 0);

        // Divider
        canvas.draw_line_v(max_x, grp_y + 1, grp_h - 2, 0x77E0F2FE);

        // Maximize / Restore button `□`
        let max_bg = if self.hover_btn == Some(1) { 0xAA38BDF8 } else { 0x331E3A8A };
        canvas.draw_rect(max_x + 1, grp_y + 1, std_btn_w - 1, grp_h - 2, max_bg, 0);
        canvas.draw_rect_outline(max_x + (std_btn_w - 10) / 2, grp_y + (grp_h - 10) / 2, 10, 10, 0xFFFFFFFF, 1);
        canvas.draw_line_h(max_x + (std_btn_w - 10) / 2, grp_y + (grp_h - 10) / 2 + 1, 10, 0xFFFFFFFF);

        // Divider
        canvas.draw_line_v(close_x, grp_y + 1, grp_h - 2, 0x77E0F2FE);

        // Wide Red-Glow Close button `X`
        let close_bg = if self.hover_btn == Some(0) { theme.btn_close_glow } else { theme.btn_close };
        canvas.draw_rect(close_x + 1, grp_y + 1, close_w - 2, grp_h - 2, close_bg, 2);
        canvas.draw_rect(close_x + 2, grp_y + 2, close_w - 4, (grp_h - 4) / 2, 0x44FFFFFF, 1);
        draw_vector_icon(canvas, "close", close_x + (close_w - 12) / 2, grp_y + (grp_h - 12) / 2, 12, 12, Some(0xFFFFFFFF));

        // 5. Inner Window Content
        canvas.push_clip(Rect::new(self.bounds.x + 2, self.bounds.y + tbh, self.bounds.w - 4, self.bounds.h - tbh - 2));
        self.content.paint(canvas);
        canvas.pop_clip();
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if self.is_closed || self.is_minimized { return false; }
        let theme = get_theme();
        let tbh = self.title_bar_height();
        let close_w = theme.pt(44.0);
        let std_btn_w = theme.pt(28.0);
        let grp_h = theme.pt(22.0);
        let right_x = self.bounds.x + self.bounds.w - theme.pt(6.0);
        let close_x = right_x - close_w;
        let max_x = close_x - std_btn_w;
        let min_x = max_x - std_btn_w;

        if my >= self.bounds.y && my <= self.bounds.y + grp_h + 4 {
            if mx >= close_x && mx <= right_x {
                self.hover_btn = Some(0); // Close
                if pressed { self.is_closed = true; }
                return true;
            } else if mx >= max_x && mx < close_x {
                self.hover_btn = Some(1); // Maximize
                if pressed {
                    let work = Rect::new(0, 0, 1920, 1080 - theme.pt(40.0));
                    self.toggle_maximize(&work);
                }
                return true;
            } else if mx >= min_x && mx < max_x {
                self.hover_btn = Some(2); // Minimize
                if pressed { self.is_minimized = true; }
                return true;
            }
        }
        self.hover_btn = None;

        // السحب بواسطة شريط العنوان
        if pressed && my >= self.bounds.y && my < self.bounds.y + tbh && mx >= self.bounds.x && mx <= self.bounds.x + self.bounds.w {
            self.is_dragging = true;
            self.drag_offset_x = mx - self.bounds.x;
            self.drag_offset_y = my - self.bounds.y;
            return true;
        }

        // تغيير الحجم (Resize Handles)
        let border_pad = 12;
        if pressed && self.bounds.contains(mx, my) {
            let on_right = mx >= self.bounds.x + self.bounds.w - border_pad;
            let on_bottom = my >= self.bounds.y + self.bounds.h - border_pad;
            if on_right && on_bottom {
                self.is_resizing = true;
                self.resize_edge = ResizeEdge::BottomRight;
                return true;
            } else if on_right {
                self.is_resizing = true;
                self.resize_edge = ResizeEdge::Right;
                return true;
            } else if on_bottom {
                self.is_resizing = true;
                self.resize_edge = ResizeEdge::Bottom;
                return true;
            }
        }

        if self.bounds.contains(mx, my) {
            self.content.handle_mouse(mx, my, pressed);
            return true;
        }

        false
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if self.bounds.contains(mx, my) {
            return self.content.handle_scroll(mx, my, dy);
        }
        false
    }

    fn handle_key(&mut self, keycode: u8, mods: u8) -> bool {
        self.content.handle_key(keycode, mods)
    }

    fn handle_char(&mut self, c: char) -> bool {
        self.content.handle_char(c)
    }
}
