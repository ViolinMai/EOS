use alloc::vec::Vec;
use alloc::boxed::Box;
use super::theme::Theme;
use super::draw::{self, FrameBuffer, Rect};
use super::apps::App;
use crate::input::InputEvent;

pub const TITLEBAR_HEIGHT: usize = 36;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ResizeEdge {
    None,
    Top, Bottom, Left, Right,
    TopLeft, TopRight, BottomLeft, BottomRight,
}

pub struct Window {
    pub id: usize, pub x: usize, pub y: usize, pub w: usize, pub h: usize,
    pub min_w: usize, pub min_h: usize,
    pub is_dragging: bool, pub drag_off_x: isize, pub drag_off_y: isize,
    pub resizing_edge: ResizeEdge,
    pub is_minimized: bool, pub is_maximized: bool,
    pub saved_x: usize, pub saved_y: usize, pub saved_w: usize, pub saved_h: usize,
    pub last_title_click: u64,
    pub active: bool, pub app: Box<dyn App>, pub surface: Vec<u32>, pub is_dirty: bool,
}

impl Window {
    pub fn redraw_surface(&mut self, theme: &Theme) {
        if self.surface.len() != self.w * self.h { self.surface.resize(self.w * self.h, 0); }
        let mut fb = FrameBuffer { pixels: &mut self.surface, width: self.w, height: self.h, pitch_pixels: self.w };
        fb.pixels.fill(0);
        
        let corner_r = if self.is_maximized { 0 } else { 10 };
        draw::draw_rect_rounded(&mut fb, 0, 0, self.w, self.h, corner_r, theme.window_bg, 255);
        draw::draw_rect_rounded(&mut fb, 0, 0, self.w, TITLEBAR_HEIGHT, corner_r, theme.titlebar_bg, 255);
        draw::draw_rect(&mut fb, 0, TITLEBAR_HEIGHT - 6, self.w, 6, theme.titlebar_bg);
        draw::draw_line_h(&mut fb, 0, TITLEBAR_HEIGHT, self.w, theme.titlebar_border);

        let title = self.app.title();
        let title_x = (self.w / 2).saturating_sub((title.len() * 9) / 2);
        draw::draw_text(&mut fb, title_x, 10, title, if self.active { theme.text_main } else { theme.text_dim }, 1);

        // Traffic Light buttons
        draw::draw_rect_rounded(&mut fb, 16, 11, 14, 14, 7, theme.btn_close, 255);
        draw::draw_rect_rounded(&mut fb, 38, 11, 14, 14, 7, theme.btn_min, 255);
        draw::draw_rect_rounded(&mut fb, 60, 11, 14, 14, 7, theme.btn_max, 255);
        
        if !self.is_maximized {
            draw::draw_rect_outline(&mut fb, 0, 0, self.w, self.h, if self.active { theme.window_border } else { theme.border }, 10);
        }

        self.app.draw(&mut fb, theme, 1, TITLEBAR_HEIGHT + 1, self.w.saturating_sub(2), self.h.saturating_sub(TITLEBAR_HEIGHT + 2));
        self.is_dirty = false;
    }
}

pub struct WindowManager {
    pub windows: Vec<Window>,
    pub screen_w: usize,
    pub screen_h: usize,
    pub next_id: usize,
}

impl WindowManager {
    pub fn new(screen_w: usize, screen_h: usize) -> Self {
        Self { windows: Vec::new(), screen_w, screen_h, next_id: 1 }
    }

    pub fn has_window(&self, title: &str) -> bool {
        self.windows.iter().any(|w| w.app.title() == title && !w.is_minimized)
    }

    pub fn unminimize_or_focus(&mut self, title: &str) -> bool {
        for win in self.windows.iter_mut() {
            if win.app.title() == title {
                win.is_minimized = false; win.active = true; win.is_dirty = true; return true;
            }
        }
        false
    }

    pub fn add_window(&mut self, mut app: Box<dyn App>, x: usize, y: usize, w: usize, h: usize) {
        for win in self.windows.iter_mut() { win.active = false; }
        app.on_resize(w, h);
        self.windows.push(Window {
            id: self.next_id, x, y, w, h, min_w: 320, min_h: 220,
            is_dragging: false, drag_off_x: 0, drag_off_y: 0,
            resizing_edge: ResizeEdge::None,
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

        if let InputEvent::MouseButton { button: 0, pressed } = event {
            if !pressed {
                for w in self.windows.iter_mut() {
                    w.is_dragging = false;
                    w.resizing_edge = ResizeEdge::None;
                }
            } else {
                for (i, win) in self.windows.iter_mut().enumerate().rev() {
                    if win.is_minimized { continue; }
                    let wx = win.x as isize; let wy = win.y as isize;
                    let ww = win.w as isize; let wh = win.h as isize;

                    if mx >= wx && mx <= wx + ww && my >= wy && my <= wy + wh {
                        to_front = Some(i);
                        handled = true;

                        // Close (16..30)
                        if mx >= wx + 16 && mx <= wx + 30 && my >= wy + 11 && my <= wy + 25 {
                            damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                            to_remove = Some(i); break;
                        }
                        // Minimize (38..52)
                        if mx >= wx + 38 && mx <= wx + 52 && my >= wy + 11 && my <= wy + 25 {
                            damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                            win.is_minimized = true; win.active = false; break;
                        }
                        // Fullscreen / Zoom (60..74)
                        if mx >= wx + 60 && mx <= wx + 74 && my >= wy + 11 && my <= wy + 25 {
                            damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                            if !win.is_maximized {
                                win.saved_x = win.x; win.saved_y = win.y; win.saved_w = win.w; win.saved_h = win.h;
                                win.x = 0; win.y = 36; win.w = self.screen_w; win.h = self.screen_h - 36 - 88;
                                win.is_maximized = true;
                            } else {
                                win.x = win.saved_x; win.y = win.saved_y; win.w = win.saved_w; win.h = win.saved_h;
                                win.is_maximized = false;
                            }
                            win.app.on_resize(win.w, win.h);
                            win.is_dirty = true;
                            damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                            break;
                        }

                        // Generous 10px resizing zone for all edges and corners
                        if !win.is_maximized {
                            let margin = 10isize;
                            let on_left = mx >= wx && mx <= wx + margin;
                            let on_right = mx >= wx + ww - margin && mx <= wx + ww;
                            let on_top = my >= wy && my <= wy + margin;
                            let on_bottom = my >= wy + wh - margin && my <= wy + wh;

                            if on_bottom && on_right { win.resizing_edge = ResizeEdge::BottomRight; break; }
                            if on_bottom && on_left { win.resizing_edge = ResizeEdge::BottomLeft; break; }
                            if on_top && on_right { win.resizing_edge = ResizeEdge::TopRight; break; }
                            if on_top && on_left { win.resizing_edge = ResizeEdge::TopLeft; break; }
                            if on_right { win.resizing_edge = ResizeEdge::Right; break; }
                            if on_bottom { win.resizing_edge = ResizeEdge::Bottom; break; }
                            if on_left { win.resizing_edge = ResizeEdge::Left; break; }
                            if on_top { win.resizing_edge = ResizeEdge::Top; break; }
                        }

                        // Title Bar Dragging & Double-Click Maximize/Restore
                        if my <= wy + TITLEBAR_HEIGHT as isize {
                            let now = crate::arch::x86_64::pit::get_ticks();
                            if now.saturating_sub(win.last_title_click) < 45 {
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                                if !win.is_maximized {
                                    win.saved_x = win.x; win.saved_y = win.y; win.saved_w = win.w; win.saved_h = win.h;
                                    win.x = 0; win.y = 36; win.w = self.screen_w; win.h = self.screen_h - 36 - 88;
                                    win.is_maximized = true;
                                } else {
                                    win.x = win.saved_x; win.y = win.saved_y; win.w = win.saved_w; win.h = win.saved_h;
                                    win.is_maximized = false;
                                }
                                win.app.on_resize(win.w, win.h);
                                win.is_dirty = true;
                                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                                win.last_title_click = 0;
                                break;
                            } else {
                                win.last_title_click = now;
                                if !win.is_maximized {
                                    win.is_dragging = true;
                                    win.drag_off_x = mx - wx;
                                    win.drag_off_y = my - wy;
                                }
                            }
                        } else {
                            win.app.on_event(event, mx - wx, my - (wy + TITLEBAR_HEIGHT as isize));
                            win.is_dirty = true;
                            damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                        }
                        break;
                    }
                }
            }
        } else if let InputEvent::MouseMove { .. } = event {
            for win in self.windows.iter_mut().rev() {
                if win.is_minimized { continue; }
                if win.resizing_edge != ResizeEdge::None {
                    damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                    match win.resizing_edge {
                        ResizeEdge::BottomRight => {
                            win.w = (mx - win.x as isize).clamp(win.min_w as isize, (self.screen_w - win.x) as isize) as usize;
                            win.h = (my - win.y as isize).clamp(win.min_h as isize, (self.screen_h - win.y) as isize) as usize;
                        }
                        ResizeEdge::Right => {
                            win.w = (mx - win.x as isize).clamp(win.min_w as isize, (self.screen_w - win.x) as isize) as usize;
                        }
                        ResizeEdge::Bottom => {
                            win.h = (my - win.y as isize).clamp(win.min_h as isize, (self.screen_h - win.y) as isize) as usize;
                        }
                        ResizeEdge::Left => {
                            let new_x = mx.clamp(0, (win.x + win.w - win.min_w) as isize) as usize;
                            win.w = win.w + (win.x - new_x);
                            win.x = new_x;
                        }
                        ResizeEdge::Top => {
                            let new_y = my.clamp(36, (win.y + win.h - win.min_h) as isize) as usize;
                            win.h = win.h + (win.y - new_y);
                            win.y = new_y;
                        }
                        _ => {}
                    }
                    win.app.on_resize(win.w, win.h);
                    win.is_dirty = true;
                    damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                    handled = true;
                    break;
                } else if win.is_dragging {
                    damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                    win.x = (mx - win.drag_off_x).clamp(0, (self.screen_w.saturating_sub(80)) as isize) as usize;
                    win.y = (my - win.drag_off_y).clamp(36, (self.screen_h.saturating_sub(60)) as isize) as usize;
                    damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
                    handled = true;
                    break;
                }
            }
        }

        if let Some(idx) = to_remove {
            self.windows.remove(idx);
            if let Some(win) = self.windows.last_mut() {
                win.active = true; win.is_dirty = true;
                damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
            }
        } else if let Some(idx) = to_front {
            for w in self.windows.iter_mut() { w.active = false; w.is_dirty = true; }
            let mut win = self.windows.remove(idx);
            win.active = true; win.is_dirty = true;
            damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h });
            self.windows.push(win);
        }
        handled
    }

    pub fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, damage: &draw::DamageTracker) {
        for win in self.windows.iter_mut() {
            if win.is_minimized { continue; }
            if win.is_dirty { win.redraw_surface(theme); }
            let w_rect = Rect { x: win.x, y: win.y, w: win.w, h: win.h };
            for d_rect in damage.get_rects() {
                if !w_rect.intersects(d_rect) { continue; }
                let ix = w_rect.x.max(d_rect.x); let iy = w_rect.y.max(d_rect.y);
                let ex = (w_rect.x + w_rect.w).min(d_rect.x + d_rect.w); let ey = (w_rect.y + w_rect.h).min(d_rect.y + d_rect.h);
                let rw = ex - ix;
                for cy in iy..ey {
                    let dst_idx = cy * fb.pitch_pixels + ix;
                    let src_idx = (cy - win.y) * win.w + (ix - win.x);
                    for offset in 0..rw {
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
