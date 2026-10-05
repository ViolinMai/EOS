use alloc::vec::Vec;
use alloc::boxed::Box;
use super::theme::Theme;
use super::draw::{self, FrameBuffer, Rect, DamageTracker};
use super::apps::App;
use crate::input::InputEvent;
use crate::gui::metrics::get_metrics;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ResizeEdge {
    None,
    Top, Bottom, Left, Right,
    TopLeft, TopRight, BottomLeft, BottomRight,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum InteractionState {
    Idle, Dragging, Resizing(ResizeEdge), PressedTitlebar, PressedButton(u8),
}

pub struct Window {
    pub id: usize, pub x: usize, pub y: usize, pub w: usize, pub h: usize,
    pub min_w: usize, pub min_h: usize,
    pub interaction_state: InteractionState,
    pub drag_off_x: isize, pub drag_off_y: isize,
    pub is_minimized: bool, pub is_maximized: bool,
    pub saved_x: usize, pub saved_y: usize, pub saved_w: usize, pub saved_h: usize,
    pub last_title_click: u64,
    pub active: bool, pub app: Box<dyn App>, pub surface: Vec<u32>, pub is_dirty: bool,
}

impl Window {
    pub fn titlebar_height() -> usize { get_metrics().titlebar_height }

    pub fn redraw_surface(&mut self, theme: &Theme) {
        if self.surface.len() != self.w * self.h { self.surface.resize(self.w * self.h, 0); }
        let mut fb = FrameBuffer { pixels: &mut self.surface, width: self.w, height: self.h, pitch_pixels: self.w };
        fb.pixels.fill(0);
        
        let metrics = get_metrics();
        let tb_h = Self::titlebar_height();
        let corner_r = if self.is_maximized { 0 } else { metrics.corner_radius };
        draw::draw_rect_rounded(&mut fb, 0, 0, self.w, self.h, corner_r, theme.window_bg, 255);
        draw::draw_rect_rounded(&mut fb, 0, 0, self.w, tb_h, corner_r, theme.titlebar_bg, 255);
        draw::draw_rect(&mut fb, 0, tb_h.saturating_sub(corner_r), self.w, corner_r, theme.titlebar_bg);
        draw::draw_line_h(&mut fb, 0, tb_h, self.w, theme.titlebar_border);

        let title = self.app.title();
        let title_w = unsafe { crate::font::get_font_manager().measure_text(title, metrics.font_title) };
        let title_x = (self.w / 2).saturating_sub(title_w / 2);
        draw::draw_text(&mut fb, title_x, (tb_h / 2).saturating_sub(metrics.font_title / 2), title, if self.active { theme.text_main } else { theme.text_dim }, 2);

        let btn_y = (tb_h / 2).saturating_sub(7);
        draw::draw_rect_rounded(&mut fb, 16, btn_y, 14, 14, 7, theme.btn_close, 255);
        draw::draw_rect_rounded(&mut fb, 38, btn_y, 14, 14, 7, theme.btn_min, 255);
        draw::draw_rect_rounded(&mut fb, 60, btn_y, 14, 14, 7, theme.btn_max, 255);
        
        if !self.is_maximized {
            draw::draw_rect_outline(&mut fb, 0, 0, self.w, self.h, if self.active { theme.window_border } else { theme.border }, corner_r);
        }

        self.app.draw(&mut fb, theme, 1, tb_h + 1, self.w.saturating_sub(2), self.h.saturating_sub(tb_h + 2));
        self.is_dirty = false;
    }
}

pub struct DesktopGeometry {
    pub screen_w: usize, pub screen_h: usize,
    pub top_bar_height: usize, pub dock_height: usize,
}

impl DesktopGeometry {
    pub fn get_work_area(&self) -> Rect {
        Rect { x: 0, y: self.top_bar_height, w: self.screen_w, h: self.screen_h.saturating_sub(self.top_bar_height + self.dock_height) }
    }
}

pub struct WindowManager {
    pub windows: Vec<Window>,
    pub geometry: DesktopGeometry,
    pub next_id: usize,
    pub active_operation_win: Option<usize>,
}

impl WindowManager {
    pub fn new(screen_w: usize, screen_h: usize) -> Self {
        Self {
            windows: Vec::new(),
            geometry: DesktopGeometry { screen_w, screen_h, top_bar_height: 36, dock_height: 80 },
            next_id: 1,
            active_operation_win: None,
        }
    }

    pub fn has_window(&self, title: &str) -> bool { self.windows.iter().any(|w| w.app.title() == title && !w.is_minimized) }

    pub fn invalidate_all_windows(&mut self) {
        for win in self.windows.iter_mut() {
            win.is_dirty = true;
        }
    }

    pub fn unminimize_or_focus(&mut self, title: &str, damage: &mut DamageTracker) -> bool {
        let mut found_idx = None;
        for (i, win) in self.windows.iter().enumerate() { if win.app.title() == title { found_idx = Some(i); break; } }
        if let Some(idx) = found_idx {
            for win in self.windows.iter_mut() { if win.active { win.active = false; win.is_dirty = true; } }
            let mut win = self.windows.remove(idx);
            win.is_minimized = false;
            win.active = true;
            win.is_dirty = true;
            damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
            self.windows.push(win);
            return true;
        }
        false
    }

    pub fn add_window(&mut self, mut app: Box<dyn App>, x: usize, y: usize, w: usize, h: usize) {
        for win in self.windows.iter_mut() { win.active = false; win.is_dirty = true; }
        app.on_resize(w, h);
        self.windows.push(Window {
            id: self.next_id, x, y, w, h, min_w: 320, min_h: 220,
            interaction_state: InteractionState::Idle, drag_off_x: 0, drag_off_y: 0,
            is_minimized: false, is_maximized: false,
            saved_x: x, saved_y: y, saved_w: w, saved_h: h,
            last_title_click: 0,
            active: true, app, surface: Vec::new(), is_dirty: true,
        });
        self.next_id += 1;
    }

    pub fn handle_event(&mut self, event: &InputEvent, mx: isize, my: isize, damage: &mut draw::DamageTracker) -> bool {
        let mut handled = false;
        let mut to_remove = None;
        let mut to_front = None;

        if let InputEvent::KeyDown { .. } | InputEvent::Char(_) = event {
            if let Some(win) = self.windows.last_mut() {
                if !win.is_minimized && win.active {
                    win.app.on_event(event, 0, 0);
                    if win.app.wants_redraw() { win.is_dirty = true; damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h }); }
                    return true;
                }
            }
            return false;
        }

        match event {
            InputEvent::MouseButton { button: 0, pressed: false } => {
                self.active_operation_win = None;
                for win in self.windows.iter_mut() {
                    match win.interaction_state {
                        InteractionState::Dragging | InteractionState::Resizing(_) | InteractionState::PressedTitlebar => {
                            win.interaction_state = InteractionState::Idle;
                        }
                        InteractionState::PressedButton(b) => {
                            win.interaction_state = InteractionState::Idle;
                            let wx = win.x as isize; let wy = win.y as isize;
                            let tb_h = Window::titlebar_height() as isize; let btn_y = (tb_h / 2) - 7;
                            let in_close = mx >= wx + 16 && mx <= wx + 30 && my >= wy + btn_y && my <= wy + btn_y + 14;
                            let in_min = mx >= wx + 38 && mx <= wx + 52 && my >= wy + btn_y && my <= wy + btn_y + 14;
                            let in_max = mx >= wx + 60 && mx <= wx + 74 && my >= wy + btn_y && my <= wy + btn_y + 14;

                            if b == 0 && in_close { damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h }); to_remove = Some(win.id); }
                            if b == 1 && in_min { damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h }); win.is_minimized = true; win.active = false; }
                            if b == 2 && in_max {
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                                if !win.is_maximized {
                                    win.saved_x = win.x; win.saved_y = win.y; win.saved_w = win.w; win.saved_h = win.h;
                                    let wa = self.geometry.get_work_area(); win.x = wa.x; win.y = wa.y; win.w = wa.w; win.h = wa.h;
                                    win.is_maximized = true;
                                } else {
                                    win.x = win.saved_x; win.y = win.saved_y; win.w = win.saved_w; win.h = win.saved_h; win.is_maximized = false;
                                }
                                win.app.on_resize(win.w, win.h); win.is_dirty = true;
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                            }
                        }
                        _ => {}
                    }
                }
                handled = true;
            }
            InputEvent::MouseButton { button: 0, pressed: true } => {
                for (i, win) in self.windows.iter_mut().enumerate().rev() {
                    if win.is_minimized { continue; }
                    let wx = win.x as isize; let wy = win.y as isize;
                    let ww = win.w as isize; let wh = win.h as isize;
                    let tb_h = Window::titlebar_height() as isize;

                    if mx >= wx && mx <= wx + ww && my >= wy && my <= wy + wh {
                        if !win.active { to_front = Some(i); }
                        handled = true;

                        let btn_y = (tb_h / 2) - 7;
                        if mx >= wx + 16 && mx <= wx + 30 && my >= wy + btn_y && my <= wy + btn_y + 14 { win.interaction_state = InteractionState::PressedButton(0); break; }
                        if mx >= wx + 38 && mx <= wx + 52 && my >= wy + btn_y && my <= wy + btn_y + 14 { win.interaction_state = InteractionState::PressedButton(1); break; }
                        if mx >= wx + 60 && mx <= wx + 74 && my >= wy + btn_y && my <= wy + btn_y + 14 { win.interaction_state = InteractionState::PressedButton(2); break; }

                        if !win.is_maximized {
                            let margin = 12isize;
                            let on_left = mx >= wx && mx <= wx + margin;
                            let on_right = mx >= wx + ww - margin && mx <= wx + ww;
                            let on_top = my >= wy && my <= wy + margin;
                            let on_bottom = my >= wy + wh - margin && my <= wy + wh;

                            let edge = if on_bottom && on_right { ResizeEdge::BottomRight }
                                else if on_bottom && on_left { ResizeEdge::BottomLeft }
                                else if on_top && on_right { ResizeEdge::TopRight }
                                else if on_top && on_left { ResizeEdge::TopLeft }
                                else if on_right { ResizeEdge::Right }
                                else if on_bottom { ResizeEdge::Bottom }
                                else if on_left { ResizeEdge::Left }
                                else if on_top { ResizeEdge::Top }
                                else { ResizeEdge::None };

                            if edge != ResizeEdge::None {
                                win.interaction_state = InteractionState::Resizing(edge);
                                self.active_operation_win = Some(win.id);
                                break;
                            }
                        }

                        if my <= wy + tb_h {
                            let now = crate::arch::x86_64::pit::get_ticks();
                            if now.saturating_sub(win.last_title_click) < 350 {
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                                if !win.is_maximized {
                                    win.saved_x = win.x; win.saved_y = win.y; win.saved_w = win.w; win.saved_h = win.h;
                                    let wa = self.geometry.get_work_area(); win.x = wa.x; win.y = wa.y; win.w = wa.w; win.h = wa.h;
                                    win.is_maximized = true;
                                } else {
                                    win.x = win.saved_x; win.y = win.saved_y; win.w = win.saved_w; win.h = win.saved_h; win.is_maximized = false;
                                }
                                win.app.on_resize(win.w, win.h); win.is_dirty = true;
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                                win.last_title_click = 0;
                            } else {
                                win.last_title_click = now;
                                if !win.is_maximized {
                                    win.interaction_state = InteractionState::Dragging;
                                    win.drag_off_x = mx - wx;
                                    win.drag_off_y = my - wy;
                                    self.active_operation_win = Some(win.id);
                                }
                            }
                        } else {
                            win.app.on_event(event, mx - wx, my - wy - tb_h);
                            if win.app.wants_redraw() { win.is_dirty = true; damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h }); }
                        }
                        break;
                    }
                }
            }
            InputEvent::MouseMove { .. } => {
                if let Some(op_id) = self.active_operation_win {
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == op_id) {
                        match win.interaction_state {
                            InteractionState::Dragging => {
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                                win.x = (mx - win.drag_off_x).clamp(0, (self.geometry.screen_w.saturating_sub(60)) as isize) as usize;
                                win.y = (my - win.drag_off_y).clamp(self.geometry.top_bar_height as isize, (self.geometry.screen_h.saturating_sub(40)) as isize) as usize;
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                                handled = true;
                            }
                            InteractionState::Resizing(edge) => {
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                                match edge {
                                    ResizeEdge::BottomRight => {
                                        win.w = (mx - win.x as isize).clamp(win.min_w as isize, (self.geometry.screen_w - win.x) as isize) as usize;
                                        win.h = (my - win.y as isize).clamp(win.min_h as isize, (self.geometry.screen_h - win.y) as isize) as usize;
                                    }
                                    ResizeEdge::Right => {
                                        win.w = (mx - win.x as isize).clamp(win.min_w as isize, (self.geometry.screen_w - win.x) as isize) as usize;
                                    }
                                    ResizeEdge::Bottom => {
                                        win.h = (my - win.y as isize).clamp(win.min_h as isize, (self.geometry.screen_h - win.y) as isize) as usize;
                                    }
                                    ResizeEdge::Left => {
                                        let old_right = win.x + win.w;
                                        let new_x = mx.clamp(0, (old_right.saturating_sub(win.min_w)) as isize) as usize;
                                        win.w = old_right - new_x;
                                        win.x = new_x;
                                    }
                                    ResizeEdge::Top => {
                                        let old_bottom = win.y + win.h;
                                        let new_y = my.clamp(self.geometry.top_bar_height as isize, (old_bottom.saturating_sub(win.min_h)) as isize) as usize;
                                        win.h = old_bottom - new_y;
                                        win.y = new_y;
                                    }
                                    ResizeEdge::BottomLeft => {
                                        let old_right = win.x + win.w;
                                        let new_x = mx.clamp(0, (old_right.saturating_sub(win.min_w)) as isize) as usize;
                                        win.w = old_right - new_x;
                                        win.x = new_x;
                                        win.h = (my - win.y as isize).clamp(win.min_h as isize, (self.geometry.screen_h - win.y) as isize) as usize;
                                    }
                                    ResizeEdge::TopRight => {
                                        let old_bottom = win.y + win.h;
                                        let new_y = my.clamp(self.geometry.top_bar_height as isize, (old_bottom.saturating_sub(win.min_h)) as isize) as usize;
                                        win.h = old_bottom - new_y;
                                        win.y = new_y;
                                        win.w = (mx - win.x as isize).clamp(win.min_w as isize, (self.geometry.screen_w - win.x) as isize) as usize;
                                    }
                                    ResizeEdge::TopLeft => {
                                        let old_right = win.x + win.w;
                                        let new_x = mx.clamp(0, (old_right.saturating_sub(win.min_w)) as isize) as usize;
                                        win.w = old_right - new_x;
                                        win.x = new_x;
                                        let old_bottom = win.y + win.h;
                                        let new_y = my.clamp(self.geometry.top_bar_height as isize, (old_bottom.saturating_sub(win.min_h)) as isize) as usize;
                                        win.h = old_bottom - new_y;
                                        win.y = new_y;
                                    }
                                    ResizeEdge::None => {}
                                }
                                win.app.on_resize(win.w, win.h);
                                win.is_dirty = true;
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                                handled = true;
                            }
                            _ => {}
                        }
                    }
                }

                if !handled {
                    for win in self.windows.iter_mut().rev() {
                        if win.is_minimized { continue; }
                        let wx = win.x as isize; let wy = win.y as isize;
                        if mx >= wx && mx <= wx + win.w as isize && my >= wy && my <= wy + win.h as isize {
                            let tb_h = Window::titlebar_height() as isize;
                            win.app.on_event(event, mx - wx, my - wy - tb_h);
                            if win.app.wants_redraw() { win.is_dirty = true; damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h }); }
                            break;
                        }
                    }
                }
            }
            _ => {}
        }

        if let Some(id_to_remove) = to_remove {
            self.windows.retain(|w| w.id != id_to_remove);
            if let Some(win) = self.windows.last_mut() { win.active = true; win.is_dirty = true; damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h }); }
        } else if let Some(idx) = to_front {
            for w in self.windows.iter_mut() { if w.active { w.active = false; w.is_dirty = true; damage.add(Rect { x: w.x, y: w.y, w: w.w, h: w.h }); } }
            let mut win = self.windows.remove(idx);
            win.active = true; win.is_dirty = true;
            damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
            self.windows.push(win);
        }
        handled
    }

    pub fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, damage: &draw::DamageTracker) {
        let max_len = fb.pixels.len();
        for win in self.windows.iter_mut() {
            if win.is_minimized { continue; }
            if win.is_dirty { win.redraw_surface(theme); }
            let w_rect = Rect { x: win.x, y: win.y, w: win.w, h: win.h };
            for d_rect in damage.get_rects() {
                if !w_rect.intersects(d_rect) { continue; }
                let ix = w_rect.x.max(d_rect.x);
                let iy = w_rect.y.max(d_rect.y);
                let ex = (w_rect.x + w_rect.w).min(d_rect.x + d_rect.w).min(fb.width);
                let ey = (w_rect.y + w_rect.h).min(d_rect.y + d_rect.h).min(fb.height);
                if ix >= ex || iy >= ey { continue; }
                let rw = ex - ix;
                for cy in iy..ey {
                    let dst_idx = cy * fb.pitch_pixels + ix;
                    let src_idx = (cy.saturating_sub(win.y)) * win.w + (ix.saturating_sub(win.x));
                    for offset in 0..rw {
                        if dst_idx + offset < max_len && src_idx + offset < win.surface.len() {
                            let fg = win.surface[src_idx + offset];
                            if fg != 0 {
                                fb.pixels[dst_idx + offset] = draw::blend_color(fb.pixels[dst_idx + offset], fg, 255);
                            }
                        }
                    }
                }
            }
        }
    }
}
