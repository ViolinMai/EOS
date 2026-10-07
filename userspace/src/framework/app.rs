use std::collections::BTreeMap;
use std::fs;
use rusttype::Font;
use crate::framework::canvas::Canvas;
use crate::framework::input::{InputManager, PointerButton, PointerEvent};
use crate::framework::menu::MenuBarWidget;
use crate::framework::theme::{get_theme, DESKTOP_WALLPAPER, take_wallpaper_changed};
use crate::apps::finder::FinderApp;
use crate::apps::settings::SettingsApp;
use crate::apps::terminal::TerminalApp;
use crate::apps::activity_monitor::ActivityMonitorApp;
use crate::apps::browser::BrowserApp;
use crate::apps::preview::PreviewApp;
use crate::apps::textedit::TextEditApp;
use crate::syscall::{sys_present_rects, sys_poll_event, sys_wait_event};
use crate::framework::widget::{Rect, ResizeEdge, Widget, WindowFrame};
use crate::framework::tools::ContextMenu;

pub struct FocusManager {
    pub active_window_idx: Option<usize>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self { active_window_idx: None }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CursorType {
    Default,
    Pointer,
    ResizeCorner,
}

pub struct FrameworkApp<'a> {
    pub width: usize,
    pub height: usize,
    pub buffer: Vec<u32>,
    pub font: Option<Font<'a>>,
    pub font_cache: BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>,
    pub input_mgr: InputManager,
    pub focus_mgr: FocusManager,
    pub root_widgets: Vec<Box<dyn Widget>>,
    pub window_surfaces: Vec<crate::framework::surface::WindowSurface>,
    pub menubar_widget: MenuBarWidget,
    pub dock_items: Vec<String>,
    pub dock_handler: Option<Box<dyn FnMut(usize)>>,
    pub current_cursor: CursorType,
    pub damage: crate::framework::damage::DamageRegion,
    pub wallpaper_cache: Vec<u32>,
    pub wallpaper_dirty: bool,
    pub hovered_dock_idx: Option<usize>,
    pub captured_window_idx: Option<usize>,
    pub prev_pointer_x: i32,
    pub prev_pointer_y: i32,
    pub context_menu: ContextMenu,
}

impl<'a> FrameworkApp<'a> {
    pub fn new(font_bytes: &'a [u8], width: usize, height: usize) -> Self {
        let font = if !font_bytes.is_empty() {
            Font::try_from_bytes(font_bytes)
        } else {
            None
        };
        let mut menubar = MenuBarWidget::new();
        menubar.layout(0, 0, width as i32, 28);

        let mut app = Self {
            width,
            height,
            buffer: vec![0xFF0B1120; width * height],
            font,
            font_cache: BTreeMap::new(),
            input_mgr: InputManager::new(),
            focus_mgr: FocusManager::new(),
            root_widgets: Vec::new(),
            window_surfaces: Vec::new(),
            menubar_widget: menubar,
            dock_items: vec![
                "Finder".into(),
                "TextEdit".into(),
                "Terminal".into(),
                "Activity Monitor".into(),
                "Browser".into(),
                "Settings".into(),
            ],
            dock_handler: None,
            current_cursor: CursorType::Default,
            damage: crate::framework::damage::DamageRegion::new(),
            wallpaper_cache: vec![0xFF0B1120; width * height],
            wallpaper_dirty: true,
            hovered_dock_idx: None,
            captured_window_idx: None,
            prev_pointer_x: (width / 2) as i32,
            prev_pointer_y: (height / 2) as i32,
            context_menu: ContextMenu::new(),
        };

        app.input_mgr.pointer.x = (width / 2) as i32;
        app.input_mgr.pointer.y = (height / 2) as i32;
        app.load_startup_wallpaper();
        app
    }

    fn load_startup_wallpaper(&mut self) {
        let paths = ["RootFS/settings.ini", "settings.ini"];
        for p in &paths {
            if let Ok(content) = fs::read_to_string(p) {
                for line in content.lines() {
                    if let Some(wp) = line.trim().strip_prefix("wallpaper=") {
                        let wp_name = wp.trim();
                        if !wp_name.is_empty() {
                            let candidates = [
                                format!("EOS SHARE/{}", wp_name),
                                format!("/EOS SHARE/{}", wp_name),
                                wp_name.to_string(),
                            ];
                            for cp in &candidates {
                                if let Ok(bytes) = fs::read(cp) {
                                    if bytes.len() >= 8 && &bytes[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
                                        if let Ok((px, w, h)) = crate::png::decode_png(&bytes) {
                                            unsafe { DESKTOP_WALLPAPER = Some((px, w, h)); }
                                            self.wallpaper_dirty = true;
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn set_dock_handler<F: 'static + FnMut(usize)>(&mut self, handler: F) {
        self.dock_handler = Some(Box::new(handler));
    }

    pub fn spawn_app(&mut self, name: &str) {
        let theme = get_theme();
        for (i, w) in self.root_widgets.iter_mut().enumerate() {
            if let Some(frame) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                if frame.title.contains(name) {
                    frame.is_closed = false;
                    frame.is_minimized = false;
                    frame.is_active = true;
                    self.focus_mgr.active_window_idx = Some(i);
                    self.menubar_widget.set_active_app(name);
                    self.request_full_redraw();
                    return;
                }
            }
        }

        let initial_w = theme.pt(680.0);
        let initial_h = theme.pt(460.0);
        let pos_offset = (self.root_widgets.len() as i32 * 28) % 240;
        let x = theme.pt(80.0) + pos_offset;
        let y = theme.pt(50.0) + pos_offset;

        let content_widget: Box<dyn Widget> = match name {
            "Finder" => Box::new(FinderApp::new()),
            "TextEdit" => Box::new(TextEditApp::new()),
            "Terminal" => Box::new(TerminalApp::new()),
            "Activity Monitor" => Box::new(ActivityMonitorApp::new()),
            "Browser" => Box::new(BrowserApp::new()),
            "Settings" => Box::new(SettingsApp::new()),
            _ => Box::new(FinderApp::new()),
        };

        let mut frame = WindowFrame::new(name, x, y, initial_w, initial_h, content_widget);
        let top_bar = self.menubar_widget.bounds.h;
        frame.bounds.y = frame.bounds.y.max(top_bar);

        let mut surface = crate::framework::surface::WindowSurface::new(frame.bounds.w as usize, frame.bounds.h as usize);
        surface.is_dirty = true;

        self.root_widgets.push(Box::new(frame));
        self.window_surfaces.push(surface);

        let new_idx = self.root_widgets.len() - 1;
        self.focus_mgr.active_window_idx = Some(new_idx);
        self.menubar_widget.set_active_app(name);
        self.request_full_redraw();
    }

    pub fn spawn_text_editor(&mut self, file_path: &str) {
        let theme = get_theme();
        let initial_w = theme.pt(700.0);
        let initial_h = theme.pt(480.0);
        let x = theme.pt(100.0);
        let y = theme.pt(60.0);

        let content = Box::new(TextEditApp::open_file(file_path));
        let title = format!("TextEdit - {}", file_path);
        let frame = WindowFrame::new(&title, x, y, initial_w, initial_h, content);

        let mut surface = crate::framework::surface::WindowSurface::new(initial_w as usize, initial_h as usize);
        surface.is_dirty = true;

        self.root_widgets.push(Box::new(frame));
        self.window_surfaces.push(surface);

        let new_idx = self.root_widgets.len() - 1;
        self.focus_mgr.active_window_idx = Some(new_idx);
        self.menubar_widget.set_active_app("TextEdit");
        self.request_full_redraw();
    }

    pub fn request_full_redraw(&mut self) {
        self.damage.add(Rect::new(0, 0, self.width as i32, self.height as i32));
    }

    pub fn update_cursor_state(&mut self, x: i32, y: i32) {
        let theme = get_theme();
        let dock_h = theme.pt(58.0);
        let dock_w = (self.dock_items.len() as i32) * theme.pt(54.0) + theme.pt(24.0);
        let dock_x = ((self.width as i32) - dock_w) / 2;
        let dock_y = (self.height as i32) - dock_h - theme.pt(12.0);

        let mut new_cursor = CursorType::Default;

        if y >= dock_y && y <= dock_y + dock_h && x >= dock_x && x <= dock_x + dock_w {
            new_cursor = CursorType::Pointer;
        } else {
            for w in self.root_widgets.iter() {
                if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                    if f.bounds.contains(x, y) {
                        let border_pad = 14;
                        if x >= f.bounds.x + f.bounds.w - border_pad && y >= f.bounds.y + f.bounds.h - border_pad {
                            new_cursor = CursorType::ResizeCorner;
                            break;
                        } else if f.hover_btn.is_some() {
                            new_cursor = CursorType::Pointer;
                            break;
                        }
                    }
                }
            }
        }

        if self.current_cursor != new_cursor {
            self.current_cursor = new_cursor;
            self.damage.add(Rect::new(x - 4, y - 4, 32, 32));
        }
    }

    pub fn route_pointer_event(&mut self, ev: PointerEvent) {
        let theme = get_theme();
        let dock_h = theme.pt(58.0);
        let dock_w = (self.dock_items.len() as i32) * theme.pt(54.0) + theme.pt(24.0);
        let dock_x = ((self.width as i32) - dock_w) / 2;
        let dock_y = (self.height as i32) - dock_h - theme.pt(12.0);

        match ev {
            PointerEvent::Move { x, y, .. } => {
                let cur_cursor_rect = Rect::new(x - 4, y - 4, 32, 32);
                let prev_cursor_rect = Rect::new(self.prev_pointer_x - 4, self.prev_pointer_y - 4, 32, 32);
                self.damage.add(cur_cursor_rect);
                self.damage.add(prev_cursor_rect);
                self.prev_pointer_x = x;
                self.prev_pointer_y = y;

                if y >= dock_y && y <= dock_y + dock_h && x >= dock_x && x <= dock_x + dock_w {
                    let idx = ((x - dock_x - theme.pt(12.0)) / theme.pt(54.0)) as usize;
                    if idx < self.dock_items.len() && self.hovered_dock_idx != Some(idx) {
                        self.hovered_dock_idx = Some(idx);
                        self.damage.add(Rect::new(dock_x - 10, dock_y - 15, dock_w + 20, dock_h + 20));
                    }
                } else if self.hovered_dock_idx.is_some() {
                    self.hovered_dock_idx = None;
                    self.damage.add(Rect::new(dock_x - 10, dock_y - 15, dock_w + 20, dock_h + 20));
                }

                for (idx, w) in self.root_widgets.iter_mut().enumerate() {
                    let old_hover = if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() { f.hover_btn } else { None };
                    w.handle_mouse(x, y, false);
                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                        if old_hover != f.hover_btn {
                            self.damage.add(Rect::new(f.bounds.x + f.bounds.w - theme.pt(100.0), f.bounds.y, theme.pt(100.0), f.title_bar_height()));
                            if let Some(surf) = self.window_surfaces.get_mut(idx) {
                                surf.is_dirty = true;
                            }
                        }
                    }
                }

                self.update_cursor_state(x, y);

                if let Some(idx) = self.captured_window_idx {
                    if idx < self.root_widgets.len() {
                        if let Some(frame) = self.root_widgets[idx].as_any_mut().downcast_mut::<WindowFrame>() {
                            let old_b = frame.bounds;
                            if frame.is_dragging {
                                let new_x = (x - frame.drag_offset_x).clamp(0, self.width as i32 - 80);
                                let new_y = (y - frame.drag_offset_y).clamp(self.menubar_widget.bounds.h, self.height as i32 - 40);
                                frame.layout(new_x, new_y, frame.bounds.w, frame.bounds.h);
                                self.damage.add(old_b);
                                self.damage.add(frame.bounds);
                            } else if frame.is_resizing {
                                let max_avail_w = (self.width as i32).saturating_sub(frame.bounds.x);
                                let max_avail_h = (self.height as i32).saturating_sub(frame.bounds.y);
                                let new_w = (x - frame.bounds.x).max(frame.min_w).min(max_avail_w).max(200);
                                let new_h = (y - frame.bounds.y).max(frame.min_h).min(max_avail_h).max(150);
                                frame.layout(frame.bounds.x, frame.bounds.y, new_w, new_h);
                                if idx < self.window_surfaces.len() {
                                    self.window_surfaces[idx].resize(new_w as usize, new_h as usize);
                                }
                                self.damage.add(old_b);
                                self.damage.add(frame.bounds);
                            }
                        }
                    }
                }
            }
            PointerEvent::Down { x, y, button } => {
                if button == PointerButton::Secondary {
                    let mut found_target = None;
                    for w in self.root_widgets.iter().rev() {
                        if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                            if f.bounds.contains(x, y) {
                                found_target = Some("file");
                                break;
                            }
                        }
                    }
                    if found_target.is_some() {
                        self.context_menu.show(x, y, vec!["Open".into(), "Rename".into(), "Delete".into()], Some("file".into()));
                    } else {
                        self.context_menu.show(x, y, vec!["New File".into(), "Open Terminal".into(), "Refresh".into()], None);
                    }
                    self.damage.add(self.context_menu.bounds);
                    return;
                }

                if self.context_menu.is_visible {
                    let old_bounds = self.context_menu.bounds;
                    let action = self.context_menu.handle_mouse(x, y, true);
                    self.damage.add(old_bounds);
                    if let Some(act) = action {
                        match act.as_str() {
                            "Open Terminal" => self.spawn_app("Terminal"),
                            "New File" => self.spawn_app("TextEdit"),
                            _ => {}
                        }
                        self.request_full_redraw();
                        return;
                    }
                }

                if button == PointerButton::Primary {
                    if y >= dock_y && y <= dock_y + dock_h && x >= dock_x && x <= dock_x + dock_w {
                        let idx = ((x - dock_x - theme.pt(12.0)) / theme.pt(54.0)) as usize;
                        if idx < self.dock_items.len() {
                            if let Some(ref mut handler) = self.dock_handler { handler(idx); }
                            let app_name = self.dock_items[idx].clone();
                            self.spawn_app(&app_name);
                            self.update_cursor_state(x, y);
                            return;
                        }
                    }

                    let mut hit_idx = None;
                    for (i, w) in self.root_widgets.iter_mut().enumerate().rev() {
                        if w.handle_mouse(x, y, true) {
                            hit_idx = Some(i);
                            break;
                        }
                    }

                    if let Some(i) = hit_idx {
                        let win = self.root_widgets.remove(i);
                        let surf = self.window_surfaces.remove(i);
                        self.root_widgets.push(win);
                        self.window_surfaces.push(surf);
                        let top_idx = self.root_widgets.len() - 1;
                        self.focus_mgr.active_window_idx = Some(top_idx);

                        if let Some(f) = self.root_widgets[top_idx].as_any_mut().downcast_mut::<WindowFrame>() {
                            let border_pad = 16;
                            if x >= f.bounds.x + f.bounds.w - border_pad && y >= f.bounds.y + f.bounds.h - border_pad {
                                f.is_resizing = true;
                                f.resize_edge = ResizeEdge::BottomRight;
                                self.captured_window_idx = Some(top_idx);
                            } else if f.is_dragging {
                                self.captured_window_idx = Some(top_idx);
                            }
                        }
                        self.request_full_redraw();
                    }
                }
                self.update_cursor_state(x, y);
            }
            PointerEvent::Up { button, .. } => {
                if button == PointerButton::Primary {
                    if let Some(idx) = self.captured_window_idx {
                        if idx < self.root_widgets.len() {
                            if let Some(f) = self.root_widgets[idx].as_any_mut().downcast_mut::<WindowFrame>() {
                                f.is_dragging = false;
                                f.is_resizing = false;
                                f.hover_btn = None;
                                self.damage.add(f.bounds);
                            }
                        }
                    }
                    self.captured_window_idx = None;
                }
                self.update_cursor_state(self.prev_pointer_x, self.prev_pointer_y);
            }
            PointerEvent::Scroll { x, y, dy } => {
                for (idx, w) in self.root_widgets.iter_mut().enumerate().rev() {
                    if w.handle_scroll(x, y, dy) {
                        if let Some(surf) = self.window_surfaces.get_mut(idx) {
                            surf.is_dirty = true;
                        }
                        if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                            self.damage.add(f.bounds);
                        }
                        break;
                    }
                }
            }
        }
    }

    fn handle_raw_event(&mut self, raw: [u64; 3]) {
        match raw[0] {
            1 => {
                let dx = (raw[1] as i32) as isize;
                let dy = ((raw[1] >> 32) as i32) as isize;
                let new_x = (self.prev_pointer_x + dx as i32).clamp(0, self.width as i32 - 1);
                let new_y = (self.prev_pointer_y + dy as i32).clamp(0, self.height as i32 - 1);
                self.input_mgr.pointer.x = new_x;
                self.input_mgr.pointer.y = new_y;
                self.route_pointer_event(PointerEvent::Move { x: new_x, y: new_y, dx: dx as i32, dy: dy as i32 });
            }
            2 => {
                let btn = (raw[1] & 0xFF) as u8;
                let pressed = ((raw[1] >> 8) & 0x01) != 0;
                let ptr_ev = self.input_mgr.handle_raw_button(btn, pressed);
                self.route_pointer_event(ptr_ev);
            }
            3 => {
                let keycode = (raw[1] & 0xFF) as u8;
                let mods = ((raw[1] >> 8) & 0xFF) as u8;
                if let Some(idx) = self.focus_mgr.active_window_idx {
                    if idx < self.root_widgets.len() {
                        self.root_widgets[idx].handle_key(keycode, mods);
                        self.request_full_redraw();
                    }
                }
            }
            4 => {
                let c = (raw[1] as u32) as u8 as char;
                if let Some(idx) = self.focus_mgr.active_window_idx {
                    if idx < self.root_widgets.len() {
                        self.root_widgets[idx].handle_char(c);
                        self.request_full_redraw();
                    }
                }
            }
            5 => {
                let dy = raw[1] as i64 as i32;
                let ptr_ev = PointerEvent::Scroll {
                    x: self.prev_pointer_x,
                    y: self.prev_pointer_y,
                    dy,
                };
                self.route_pointer_event(ptr_ev);
            }
            _ => {}
        }
    }

    fn rebuild_wallpaper_cache(&mut self) {
        let theme = get_theme();
        unsafe {
            if let Some((ref wp_px, wp_w, wp_h)) = DESKTOP_WALLPAPER {
                let scale_x = self.width as f32 / wp_w as f32;
                let scale_y = self.height as f32 / wp_h as f32;
                let scale = scale_x.max(scale_y);

                let crop_w = (self.width as f32 / scale) as usize;
                let crop_h = (self.height as f32 / scale) as usize;
                let start_x = (wp_w.saturating_sub(crop_w)) / 2;
                let start_y = (wp_h.saturating_sub(crop_h)) / 2;

                for y in 0..self.height {
                    let sy = (start_y + ((y as f32 / scale) as usize)).min(wp_h - 1);
                    let row_idx = y * self.width;
                    let src_row = sy * wp_w;
                    for x in 0..self.width {
                        let sx = (start_x + ((x as f32 / scale) as usize)).min(wp_w - 1);
                        self.wallpaper_cache[row_idx + x] = wp_px[src_row + sx];
                    }
                }
            } else {
                self.wallpaper_cache.fill(theme.bg_desktop);
            }
        }
        self.wallpaper_dirty = false;
    }

    pub fn run_loop(&mut self) {
        self.rebuild_wallpaper_cache();
        self.request_full_redraw();

        loop {
            if take_wallpaper_changed() || self.wallpaper_dirty {
                self.rebuild_wallpaper_cache();
                self.request_full_redraw();
            }

            let mut removed = Vec::new();
            for (idx, w) in self.root_widgets.iter().enumerate() {
                if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                    if f.is_closed {
                        removed.push(idx);
                        self.damage.add(f.bounds);
                    }
                }
            }
            for &idx in removed.iter().rev() {
                if idx < self.root_widgets.len() {
                    self.root_widgets.remove(idx);
                }
                if idx < self.window_surfaces.len() {
                    self.window_surfaces.remove(idx);
                }
            }
            if !removed.is_empty() {
                self.focus_mgr.active_window_idx = if self.root_widgets.is_empty() { None } else { Some(self.root_widgets.len() - 1) };
                self.captured_window_idx = None;
                self.request_full_redraw();
            }

            let mut pending_open_text = None;
            let mut pending_open_img = None;

            for w in self.root_widgets.iter_mut() {
                if let Some(frame) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                    if let Some(finder) = frame.content.as_any_mut().downcast_mut::<FinderApp>() {
                        if let Some(path) = finder.pending_open_text.take() {
                            pending_open_text = Some(path);
                        }
                        if let Some(req) = finder.pending_open_image.take() {
                            pending_open_img = Some(req);
                        }
                    }
                }
            }

            if let Some(path) = pending_open_text {
                self.spawn_text_editor(&path);
            }

            if let Some((name, path)) = pending_open_img {
                let theme = get_theme();
                let prev_app = Box::new(PreviewApp::new(name.clone(), path));
                let win_w = theme.pt(680.0);
                let win_h = theme.pt(480.0);
                let win = Box::new(WindowFrame::new(
                    format!("Preview - {}", name),
                    theme.pt(120.0),
                    self.menubar_widget.bounds.h + theme.pt(30.0),
                    win_w,
                    win_h,
                    prev_app,
                ));
                self.damage.add(win.bounds);
                self.root_widgets.push(win);
                self.window_surfaces.push(crate::framework::surface::WindowSurface::new(win_w as usize, win_h as usize));
                self.focus_mgr.active_window_idx = Some(self.root_widgets.len() - 1);
                self.request_full_redraw();
            }

            if let Some(ev) = sys_wait_event(16) {
                self.handle_raw_event(ev);
            }

            let mut pending_dx: i32 = 0;
            let mut pending_dy: i32 = 0;
            let mut has_mouse_move = false;

            while let Some(raw) = sys_poll_event() {
                if raw[0] == 1 {
                    pending_dx += raw[1] as i32;
                    pending_dy += (raw[1] >> 32) as i32;
                    has_mouse_move = true;
                } else {
                    if has_mouse_move {
                        let new_x = (self.prev_pointer_x + pending_dx).clamp(0, self.width as i32 - 1);
                        let new_y = (self.prev_pointer_y + pending_dy).clamp(0, self.height as i32 - 1);
                        self.input_mgr.pointer.x = new_x;
                        self.input_mgr.pointer.y = new_y;
                        self.route_pointer_event(PointerEvent::Move { x: new_x, y: new_y, dx: pending_dx, dy: pending_dy });
                        pending_dx = 0;
                        pending_dy = 0;
                        has_mouse_move = false;
                    }
                    self.handle_raw_event(raw);
                }
            }

            if has_mouse_move {
                let new_x = (self.prev_pointer_x + pending_dx).clamp(0, self.width as i32 - 1);
                let new_y = (self.prev_pointer_y + pending_dy).clamp(0, self.height as i32 - 1);
                self.input_mgr.pointer.x = new_x;
                self.input_mgr.pointer.y = new_y;
                self.route_pointer_event(PointerEvent::Move { x: new_x, y: new_y, dx: pending_dx, dy: pending_dy });
            }

            self.render();
        }
    }

    pub fn render(&mut self) {
        let theme = get_theme();
        let dock_h = theme.pt(58.0);
        let dock_w = (self.dock_items.len() as i32) * theme.pt(54.0) + theme.pt(24.0);
        let dock_x = ((self.width as i32) - dock_w) / 2;
        let dock_y = (self.height as i32) - dock_h - theme.pt(12.0);

        if self.damage.is_empty() { return; }
        let dirty_rects = self.damage.rects.clone();

        let dock_rect = Rect::new(dock_x - 10, dock_y - 15, dock_w + 20, dock_h + 20);

        for r in &dirty_rects {
            let cx1 = r.x.clamp(0, self.width as i32) as usize;
            let cy1 = r.y.clamp(0, self.height as i32) as usize;
            let cx2 = (r.x + r.w).clamp(0, self.width as i32) as usize;
            let cy2 = (r.y + r.h).clamp(0, self.height as i32) as usize;

            for y in cy1..cy2 {
                let row = y * self.width;
                self.buffer[row + cx1..row + cx2].copy_from_slice(&self.wallpaper_cache[row + cx1..row + cx2]);
            }

            for (_idx, w) in self.root_widgets.iter().enumerate() {
                if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                    if let Some(intersect) = f.bounds.intersect(&r).non_empty() {
                        let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                        canvas.push_clip(intersect);
                        w.paint(&mut canvas);
                        canvas.pop_clip();
                    }
                }
            }

            if let Some(intersect) = self.menubar_widget.bounds.intersect(&r).non_empty() {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                canvas.push_clip(intersect);
                self.menubar_widget.paint(&mut canvas);
                canvas.pop_clip();
            }

            if let Some(intersect) = dock_rect.intersect(&r).non_empty() {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                canvas.push_clip(intersect);
                canvas.draw_frosted_glass_rect(dock_x, dock_y, dock_w, dock_h, 0xDD18181B, theme.pt(16.0) as usize);
                canvas.draw_rect_outline(dock_x, dock_y, dock_w, dock_h, theme.border_window, theme.pt(16.0) as usize);

                for (i, item) in self.dock_items.iter().enumerate() {
                    let is_hovered = self.hovered_dock_idx == Some(i);
                    let lift_y = if is_hovered { theme.pt(8.0) } else { 0 };
                    let icon_box_sz = if is_hovered { theme.pt(44.0) } else { theme.pt(40.0) };

                    let ix = dock_x + theme.pt(12.0) + (i as i32 * theme.pt(54.0)) - if is_hovered { 2 } else { 0 };
                    let iy = dock_y + theme.pt(6.0) - lift_y;

                    let bg_c = match item.as_str() {
                        "Finder" => 0xFF1E3A8A,
                        "TextEdit" => 0xFF7C2D12,
                        "Terminal" => 0xFF064E3B,
                        "Activity Monitor" => 0xFF0C4A6E,
                        "Browser" => 0xFF581C87,
                        _ => 0xFF334155,
                    };

                    canvas.draw_rect(ix, iy, icon_box_sz, icon_box_sz, bg_c, theme.pt(10.0) as usize);
                    canvas.draw_rect_outline(ix, iy, icon_box_sz, icon_box_sz, 0x88FFFFFF, theme.pt(10.0) as usize);

                    let cx = ix + (icon_box_sz / 2);
                    let cy = iy + (icon_box_sz / 2);

                    match item.as_str() {
                        "Finder" => {
                            canvas.draw_rect(cx - 10, cy - 8, 8, 4, 0xFF38BDF8, 1);
                            canvas.draw_rect(cx - 10, cy - 5, 20, 14, 0xFF0284C7, 2);
                            canvas.draw_rect(cx - 8, cy - 3, 16, 10, 0xFFFFFFFF, 1);
                            canvas.draw_rect(cx - 6, cy - 1, 12, 6, 0xFF0284C7, 0);
                        }
                        "TextEdit" => {
                            canvas.draw_rect(cx - 8, cy - 10, 16, 20, 0xFFF8FAFC, 2);
                            canvas.draw_line_h(cx - 5, cy - 6, 10, 0xFF94A3B8);
                            canvas.draw_line_h(cx - 5, cy - 2, 10, 0xFF94A3B8);
                            canvas.draw_line_h(cx - 5, cy + 2, 7, 0xFF94A3B8);
                            canvas.draw_rect(cx + 2, cy + 1, 4, 8, 0xFFEA580C, 1);
                        }
                        "Terminal" => {
                            canvas.draw_rect(cx - 11, cy - 9, 22, 18, 0xFF022C22, 2);
                            canvas.draw_rect_outline(cx - 11, cy - 9, 22, 18, 0xFF10B981, 2);
                            canvas.draw_text(cx - 8, cy - 6, ">", 0xFF34D399, 13);
                            canvas.draw_line_h(cx - 1, cy + 4, 7, 0xFF34D399);
                        }
                        "Activity Monitor" => {
                            canvas.draw_rect(cx - 11, cy - 9, 22, 18, 0xFF082F49, 2);
                            canvas.draw_line_h(cx - 9, cy, 4, 0xFF38BDF8);
                            canvas.draw_line_v(cx - 5, cy - 6, 8, 0xFF38BDF8);
                            canvas.draw_line_v(cx - 1, cy + 1, 6, 0xFF38BDF8);
                            canvas.draw_line_h(cx + 3, cy, 6, 0xFF38BDF8);
                        }
                        "Browser" => {
                            canvas.draw_rect(cx - 9, cy - 9, 18, 18, 0xFF7C3AED, 9);
                            canvas.draw_rect_outline(cx - 9, cy - 9, 18, 18, 0xFFE9D5FF, 9);
                            canvas.draw_line_h(cx - 8, cy, 16, 0xFFFFFFFF);
                            canvas.draw_line_v(cx, cy - 8, 16, 0xFFFFFFFF);
                        }
                        _ => {
                            canvas.draw_rect(cx - 8, cy - 8, 16, 16, 0xFF64748B, 8);
                            canvas.draw_rect(cx - 3, cy - 3, 6, 6, 0xFF1E293B, 3);
                        }
                    }
                }
                canvas.pop_clip();
            }

            if self.context_menu.is_visible {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                self.context_menu.draw(&mut canvas);
            }

            let cur_cursor_rect = Rect::new(self.prev_pointer_x - 4, self.prev_pointer_y - 4, 32, 32);
            if cur_cursor_rect.intersect(&r).non_empty().is_some() {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                match self.current_cursor {
                    CursorType::Default => canvas.draw_linux_cursor(self.prev_pointer_x, self.prev_pointer_y),
                    CursorType::Pointer => canvas.draw_pointer_hand_cursor(self.prev_pointer_x, self.prev_pointer_y),
                    CursorType::ResizeCorner => canvas.draw_resize_corner_cursor(self.prev_pointer_x, self.prev_pointer_y),
                }
            }
        }

        let rect_data: Vec<[u32; 4]> = dirty_rects.iter().map(|r| [r.x as u32, r.y as u32, r.w as u32, r.h as u32]).collect();
        sys_present_rects(self.buffer.as_ptr(), self.width, self.height, &rect_data);
        self.damage.clear();
    }
}
