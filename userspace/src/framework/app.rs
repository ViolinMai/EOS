use std::collections::BTreeMap;
use std::fs;
use rusttype::Font;
use crate::framework::canvas::Canvas;
use crate::framework::input::{InputManager, PointerButton, PointerEvent};
use crate::framework::menu::MenuBarWidget;
use crate::framework::theme::{get_theme, DESKTOP_WALLPAPER};
use crate::framework::layout::ContainerWidget;
use crate::apps::finder::FinderApp;
use crate::apps::settings::SettingsApp;
use crate::apps::terminal::TerminalApp;
use crate::apps::activity_monitor::ActivityMonitorApp;
use crate::apps::browser::BrowserApp;
use crate::apps::preview::PreviewApp;
use crate::syscall::{sys_present, sys_present_rects, sys_poll_event, sys_wait_event};
use crate::framework::widget::{DamageTracker, Rect, ResizeEdge, Widget, WindowFrame, WindowTileState};

pub struct FocusManager {
    pub active_window_idx: Option<usize>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self { active_window_idx: None }
    }
}

pub struct FrameworkApp<'a> {
    pub buffer: Vec<u32>,
    pub width: usize,
    pub height: usize,
    pub font: Option<Font<'a>>,
    pub font_cache: BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>,
    pub root_widgets: Vec<Box<dyn Widget>>,
    pub window_surfaces: Vec<crate::framework::surface::WindowSurface>,
    pub menubar_widget: MenuBarWidget,
    pub dock_items: Vec<String>,
    pub input_mgr: InputManager,
    pub focus_mgr: FocusManager,
    pub damage: crate::framework::damage::DamageRegion,
    pub captured_window_idx: Option<usize>,
    pub dock_handler: Option<Box<dyn FnMut(usize)>>,
    pub prev_pointer_x: i32,
    pub prev_pointer_y: i32,
    pub wallpaper_cache: Vec<u32>,
    pub wallpaper_dirty: bool,
    pub active_wallpaper_path: String,
    pub show_debug_overlay: bool,
    pub frame_counter: usize,
    pub last_frame_time_ms: u64,
}

impl<'a> FrameworkApp<'a> {
    pub fn new(font_bytes: &'a [u8], width: usize, height: usize) -> Self {
        let font = if !font_bytes.is_empty() {
            Font::try_from_bytes(font_bytes)
        } else {
            None
        };

        let mut menubar = MenuBarWidget::new();
        menubar.layout(0, 0, width as i32, get_theme().pt(30.0));

        let mut app = Self {
            buffer: vec![0; width * height],
            width,
            height,
            font,
            font_cache: BTreeMap::new(),
            root_widgets: Vec::new(),
            window_surfaces: Vec::new(),
            menubar_widget: menubar,
            dock_items: vec![
                "Finder".into(),
                "Settings".into(),
                "Terminal".into(),
                "Activity Monitor".into(),
                "Browser".into(),
            ],
            input_mgr: InputManager::new(),
            focus_mgr: FocusManager::new(),
            damage: crate::framework::damage::DamageRegion::new(),
            captured_window_idx: None,
            dock_handler: None,
            prev_pointer_x: (width / 2) as i32,
            prev_pointer_y: (height / 2) as i32,
            wallpaper_cache: vec![0; width * height],
            wallpaper_dirty: true,
            active_wallpaper_path: String::new(),
            show_debug_overlay: false,
            frame_counter: 0,
            last_frame_time_ms: 0,
        };

        app.input_mgr.pointer.x = (width / 2) as i32;
        app.input_mgr.pointer.y = (height / 2) as i32;
        app.damage.add(Rect::new(0, 0, width as i32, height as i32));
        app.load_persisted_wallpaper();
        app
    }

    pub fn set_dock_handler<F: FnMut(usize) + 'static>(&mut self, handler: F) {
        self.dock_handler = Some(Box::new(handler));
    }

    pub fn get_work_area(&self) -> Rect {
        let theme = get_theme();
        let top = self.menubar_widget.bounds.h;
        let dock_h = theme.pt(54.0) + theme.pt(16.0);
        Rect {
            x: 0,
            y: top,
            w: self.width as i32,
            h: (self.height as i32).saturating_sub(top + dock_h),
        }
    }

    fn load_persisted_wallpaper(&mut self) {
        let candidates_cfg = ["RootFS/settings.ini", "settings.ini", "/EOS SHARE/settings.ini"];
        let mut content = String::new();
        for cfg in &candidates_cfg {
            if let Ok(c) = fs::read_to_string(cfg) {
                if !c.is_empty() {
                    content = c;
                    break;
                }
            }
        }

        if content.is_empty() { return; }

        for line in content.lines() {
            if let Some(val) = line.strip_prefix("wallpaper=") {
                let fname = val.trim();
                if !fname.is_empty() {
                    let candidates = [
                        format!("EOS SHARE/{}", fname),
                        format!("/EOS SHARE/{}", fname),
                        fname.to_string(),
                    ];
                    for path in &candidates {
                        if let Ok(data) = fs::read(path) {
                            let mut decoded: Option<(Vec<u32>, usize, usize)> = None;
                            if data.len() >= 8 && &data[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
                                if let Ok((px, w, h)) = crate::png::decode_png(&data) {
                                    decoded = Some((px, w, h));
                                }
                            } else if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
                                let mut dec = zune_jpeg::JpegDecoder::new(&data);
                                if let Ok(raw) = dec.decode() {
                                    if let Some(info) = dec.info() {
                                        let orig_w = info.width as usize;
                                        let orig_h = info.height as usize;
                                        let step = ((orig_w + 1919) / 1920).max((orig_h + 1079) / 1080).max(1);
                                        let out_w = orig_w / step;
                                        let out_h = orig_h / step;
                                        let mut px = vec![0u32; out_w * out_h];
                                        for dy in 0..out_h {
                                            let sy = dy * step;
                                            let src_row = sy * orig_w * 3;
                                            let dst_row = dy * out_w;
                                            for dx in 0..out_w {
                                                let sx = dx * step;
                                                let o = src_row + (sx * 3);
                                                if o + 2 < raw.len() {
                                                    px[dst_row + dx] = (0xFF << 24)
                                                        | ((raw[o] as u32) << 16)
                                                        | ((raw[o+1] as u32) << 8)
                                                        | (raw[o+2] as u32);
                                                }
                                            }
                                        }
                                        decoded = Some((px, out_w, out_h));
                                    }
                                }
                            }

                            if let Some((px, w, h)) = decoded {
                                unsafe { DESKTOP_WALLPAPER = Some((px, w, h)); }
                                self.wallpaper_dirty = true;
                                self.damage.add(Rect::new(0, 0, self.width as i32, self.height as i32));
                                return;
                            }
                        }
                    }
                }
            }
        }
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
                    self.damage.add(frame.bounds);
                    return;
                }
            }
        }

        let content: Box<dyn Widget> = match name {
            "Finder" => Box::new(FinderApp::new()),
            "Settings" => Box::new(SettingsApp::new()),
            "Terminal" => Box::new(TerminalApp::new()),
            "Activity Monitor" => Box::new(ActivityMonitorApp::new()),
            "Browser" => Box::new(BrowserApp::new()),
            _ => Box::new(ContainerWidget::vbox()),
        };

        let offset = (self.root_widgets.len() as i32) * theme.pt(25.0);
        let win_w = theme.pt(640.0);
        let win_h = theme.pt(440.0);
        let win = Box::new(WindowFrame::new(
            name,
            theme.pt(80.0) + offset,
            self.menubar_widget.bounds.h + theme.pt(20.0) + offset,
            win_w,
            win_h,
            content,
        ));
        self.damage.add(win.bounds);
        self.root_widgets.push(win);
        self.window_surfaces.push(crate::framework::surface::WindowSurface::new(win_w as usize, win_h as usize));
        self.focus_mgr.active_window_idx = Some(self.root_widgets.len() - 1);
    }

    fn route_pointer_event(&mut self, ev: PointerEvent) {
        let theme = get_theme();
        let dock_h = theme.pt(54.0);
        let dock_w = (self.dock_items.len() as i32) * theme.pt(52.0) + theme.pt(20.0);
        let dock_x = ((self.width as i32) - dock_w) / 2;
        let dock_y = (self.height as i32) - dock_h - theme.pt(12.0);

        match ev {
            PointerEvent::Move { x, y, .. } => {
                let cur_cursor_rect = Rect::new(x, y, 20, 24);
                let prev_cursor_rect = Rect::new(self.prev_pointer_x, self.prev_pointer_y, 20, 24);
                self.damage.add(cur_cursor_rect);
                self.damage.add(prev_cursor_rect);
                self.prev_pointer_x = x;
                self.prev_pointer_y = y;

                for (idx, w) in self.root_widgets.iter_mut().enumerate() {
                    let old_hover = if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() { f.hover_btn } else { None };
                    w.handle_mouse(x, y, false);
                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                        if old_hover != f.hover_btn {
                            let btn_area_w = theme.pt(28.0) * 3;
                            let btn_area = Rect::new(f.bounds.x + f.bounds.w - btn_area_w, f.bounds.y, btn_area_w, f.title_bar_height());
                            self.damage.add(btn_area);
                            if let Some(surf) = self.window_surfaces.get_mut(idx) {
                                surf.is_dirty = true;
                            }
                        }
                    }
                }

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
                                let mut b = frame.bounds;
                                match frame.resize_edge {
                                    ResizeEdge::Right => b.w = (x - b.x).max(frame.min_w),
                                    ResizeEdge::Bottom => b.h = (y - b.y).max(frame.min_h),
                                    ResizeEdge::BottomRight => {
                                        b.w = (x - b.x).max(frame.min_w);
                                        b.h = (y - b.y).max(frame.min_h);
                                    }
                                    ResizeEdge::Left => {
                                        let nw = (b.x + b.w - x).max(frame.min_w);
                                        b.x += b.w - nw;
                                        b.w = nw;
                                    }
                                    ResizeEdge::Top => {
                                        let nh = (b.y + b.h - y).max(frame.min_h);
                                        b.y += b.h - nh;
                                        b.h = nh;
                                    }
                                    _ => {}
                                }
                                frame.layout(b.x, b.y, b.w, b.h);
                                if let Some(surf) = self.window_surfaces.get_mut(idx) {
                                    surf.resize(b.w as usize, b.h as usize);
                                    surf.is_dirty = true;
                                }
                                self.damage.add(old_b);
                                self.damage.add(frame.bounds);
                            }
                        }
                    }
                }
            }
            PointerEvent::Down { x, y, button } => {
                if button == PointerButton::Primary {
                    if y >= dock_y && y <= dock_y + dock_h && x >= dock_x && x <= dock_x + dock_w {
                        let idx = ((x - dock_x - theme.pt(12.0)) / theme.pt(52.0)) as usize;
                        if idx < self.dock_items.len() {
                            if let Some(ref mut handler) = self.dock_handler { handler(idx); }
                            let app_name = self.dock_items[idx].clone();
                            self.spawn_app(&app_name);
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
                        let work_area = self.get_work_area();
                        if let Some(f) = self.root_widgets[i].as_any_mut().downcast_mut::<WindowFrame>() {
                            if f.hover_btn == Some(1) {
                                let old_b = f.bounds;
                                f.toggle_maximize(&work_area);
                                if let Some(surf) = self.window_surfaces.get_mut(i) {
                                    surf.resize(f.bounds.w as usize, f.bounds.h as usize);
                                    surf.is_dirty = true;
                                }
                                self.damage.add(old_b);
                                self.damage.add(f.bounds);
                                return;
                            }
                        }

                        let win = self.root_widgets.remove(i);
                        let surf = self.window_surfaces.remove(i);
                        self.root_widgets.push(win);
                        self.window_surfaces.push(surf);
                        let top_idx = self.root_widgets.len() - 1;
                        self.focus_mgr.active_window_idx = Some(top_idx);

                        for (k, w) in self.root_widgets.iter_mut().enumerate() {
                            if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                f.is_active = k == top_idx;
                                if let Some(s) = self.window_surfaces.get_mut(k) {
                                    s.is_dirty = true;
                                }
                            }
                        }

                        if let Some(f) = self.root_widgets[top_idx].as_any_mut().downcast_mut::<WindowFrame>() {
                            if f.is_dragging || f.is_resizing {
                                self.captured_window_idx = Some(top_idx);
                            }
                            self.damage.add(f.bounds);
                        }
                    }
                }
            }
            PointerEvent::Up { button, .. } => {
                if button == PointerButton::Primary {
                    if let Some(idx) = self.captured_window_idx {
                        if idx < self.root_widgets.len() {
                            if let Some(f) = self.root_widgets[idx].as_any_mut().downcast_mut::<WindowFrame>() {
                                f.is_dragging = false;
                                f.is_resizing = false;
                                self.damage.add(f.bounds);
                            }
                        }
                    }
                    self.captured_window_idx = None;
                }
            }
            PointerEvent::Scroll { x, y, dy } => {
                for (idx, w) in self.root_widgets.iter_mut().enumerate().rev() {
                    if w.handle_scroll(x, y, dy) {
                        if let Some(surf) = self.window_surfaces.get_mut(idx) {
                            surf.is_dirty = true;
                        }
                        break;
                    }
                }
                if let Some(idx) = self.focus_mgr.active_window_idx {
                    if let Some(w) = self.root_widgets.get(idx) {
                        if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                            self.damage.add(f.bounds);
                        }
                    }
                }
            }
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

    pub fn handle_window_tiling_shortcut(&mut self, keycode: u8, mods: u8) -> bool {
        const MOD_WIN: u8 = 1 << 4;
        if (mods & MOD_WIN) == 0 { return false; }

        if let Some(idx) = self.focus_mgr.active_window_idx {
            if idx < self.root_widgets.len() {
                let work_area = self.get_work_area();
                if let Some(f) = self.root_widgets[idx].as_any_mut().downcast_mut::<WindowFrame>() {
                    let old_b = f.bounds;
                    let handled = match keycode {
                        0x4B => { f.tile_left(&work_area); true }
                        0x4D => { f.tile_right(&work_area); true }
                        0x48 => { f.toggle_maximize(&work_area); true }
                        0x50 => {
                            if f.is_maximized || f.tile_state != WindowTileState::None {
                                f.layout(f.prev_bounds.x, f.prev_bounds.y, f.prev_bounds.w, f.prev_bounds.h);
                                f.is_maximized = false;
                                f.tile_state = WindowTileState::None;
                                true
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };

                    if handled {
                        if let Some(surf) = self.window_surfaces.get_mut(idx) {
                            surf.resize(f.bounds.w as usize, f.bounds.h as usize);
                            surf.is_dirty = true;
                        }
                        self.damage.add(old_b);
                        self.damage.add(f.bounds);
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn run_loop(&mut self) {
        loop {
            let mut got_any_event = false;
            let mut raw_ev = sys_wait_event(16);

            while let Some(ev) = raw_ev {
                got_any_event = true;
                match ev[0] {
                    1 => {
                        let dx = (ev[1] as u32) as i32;
                        let dy = ((ev[1] >> 32) as u32) as i32;
                        let ptr_ev = self.input_mgr.handle_raw_motion(dx, dy, self.width as i32, self.height as i32);
                        self.route_pointer_event(ptr_ev);
                    }
                    2 => {
                        let btn = (ev[1] & 0xFF) as u8;
                        let pressed = ((ev[1] >> 8) & 0xFF) != 0;
                        let ptr_ev = self.input_mgr.handle_raw_button(btn, pressed);
                        self.route_pointer_event(ptr_ev);
                    }
                    3 => {
                        let keycode = (ev[1] & 0xFF) as u8;
                        let mods = ((ev[1] >> 8) & 0xFF) as u8;

                        if keycode == 0x44 {
                            self.show_debug_overlay = !self.show_debug_overlay;
                            self.damage.add(Rect::new(0, 0, self.width as i32, self.height as i32));
                        } else if !self.handle_window_tiling_shortcut(keycode, mods) {
                            let _key_ev = self.input_mgr.handle_raw_key(keycode, mods, true);
                            if let Some(idx) = self.focus_mgr.active_window_idx {
                                if let Some(w) = self.root_widgets.get_mut(idx) {
                                    w.handle_key(keycode, mods);
                                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                                        self.damage.add(f.bounds);
                                        if let Some(s) = self.window_surfaces.get_mut(idx) {
                                            s.is_dirty = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    4 => {
                        let c_code = ev[1] as u32;
                        if let Some(c) = char::from_u32(c_code) {
                            if let Some(idx) = self.focus_mgr.active_window_idx {
                                if let Some(w) = self.root_widgets.get_mut(idx) {
                                    w.handle_char(c);
                                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                                        self.damage.add(f.bounds);
                                        if let Some(s) = self.window_surfaces.get_mut(idx) {
                                            s.is_dirty = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    5 => {
                        let dy = (ev[1] as u32) as i32;
                        let ptr_ev = PointerEvent::Scroll {
                            x: self.input_mgr.pointer.x,
                            y: self.input_mgr.pointer.y,
                            dy,
                        };
                        self.route_pointer_event(ptr_ev);
                    }
                    _ => {}
                }
                raw_ev = sys_poll_event();
            }

            let mut removed_indices = Vec::new();
            for (idx, w) in self.root_widgets.iter().enumerate() {
                if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                    if f.is_closed {
                        removed_indices.push(idx);
                        self.damage.add(f.bounds);
                    }
                }
            }
            for &idx in removed_indices.iter().rev() {
                self.root_widgets.remove(idx);
                self.window_surfaces.remove(idx);
            }
            if !removed_indices.is_empty() {
                self.focus_mgr.active_window_idx = if self.root_widgets.is_empty() { None } else { Some(self.root_widgets.len() - 1) };
            }

            let mut image_to_open = None;
            for w in &mut self.root_widgets {
                if let Some(frame) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                    if let Some(finder) = frame.content.as_any_mut().downcast_mut::<FinderApp>() {
                        if let Some(req) = finder.pending_open_image.take() {
                            image_to_open = Some(req);
                            break;
                        }
                    }
                }
            }

            if let Some((name, path)) = image_to_open {
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
            }

            if crate::framework::theme::take_wallpaper_changed() {
                self.wallpaper_dirty = true;
            }

            unsafe {
                if DESKTOP_WALLPAPER.is_some() && self.wallpaper_dirty {
                    self.rebuild_wallpaper_cache();
                    self.damage.add(Rect::new(0, 0, self.width as i32, self.height as i32));
                }
            }

            if !got_any_event && self.damage.is_empty() {
                continue;
            }

            let theme = get_theme();
            let dock_h = theme.pt(54.0);
            let dock_w = (self.dock_items.len() as i32) * theme.pt(52.0) + theme.pt(20.0);
            let dock_x = ((self.width as i32) - dock_w) / 2;
            let dock_y = (self.height as i32) - dock_h - theme.pt(12.0);
            let dock_rect = Rect::new(dock_x, dock_y, dock_w, dock_h);
            let menubar_rect = self.menubar_widget.bounds;

            self.damage.clamp_to_screen(self.width, self.height);

            for (idx, w) in self.root_widgets.iter_mut().enumerate() {
                if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                    let surf = &mut self.window_surfaces[idx];
                    if surf.is_dirty {
                        surf.resize(f.bounds.w as usize, f.bounds.h as usize);
                        surf.pixels.fill(0);
                        let mut local_canvas = Canvas::new(
                            &mut surf.pixels,
                            surf.width,
                            surf.height,
                            self.font.as_ref(),
                            &mut self.font_cache,
                        );

                        let orig_bounds = f.bounds;
                        let tb_h = f.title_bar_height();
                        f.content.layout(0, tb_h, orig_bounds.w, (orig_bounds.h - tb_h).max(0));

                        let zero_frame = WindowFrame {
                            title: f.title.clone(),
                            bounds: Rect::new(0, 0, orig_bounds.w, orig_bounds.h),
                            prev_bounds: f.prev_bounds,
                            content: Box::new(ContainerWidget::vbox()),
                            is_active: f.is_active,
                            is_dragging: false,
                            is_resizing: false,
                            drag_offset_x: 0,
                            drag_offset_y: 0,
                            resize_edge: ResizeEdge::None,
                            is_closed: false,
                            is_minimized: false,
                            is_maximized: f.is_maximized,
                            tile_state: f.tile_state,
                            min_w: f.min_w,
                            min_h: f.min_h,
                            hover_btn: f.hover_btn,
                        };
                        zero_frame.paint(&mut local_canvas);
                        f.content.paint(&mut local_canvas);

                        f.content.layout(orig_bounds.x, orig_bounds.y + tb_h, orig_bounds.w, (orig_bounds.h - tb_h).max(0));
                        surf.is_dirty = false;
                    }
                }
            }

            for dmg in &self.damage.rects {
                let rx = dmg.x as usize;
                let ry = dmg.y as usize;
                let rw = dmg.w as usize;
                let rh = dmg.h as usize;

                for cy in ry..(ry + rh) {
                    let off = cy * self.width + rx;
                    self.buffer[off..off + rw].copy_from_slice(&self.wallpaper_cache[off..off + rw]);
                }

                for (idx, w) in self.root_widgets.iter().enumerate() {
                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                        if !f.is_closed && !f.is_minimized && f.bounds.intersects(dmg) {
                            let surf = &self.window_surfaces[idx];
                            surf.blit_to(&mut self.buffer, self.width, self.height, f.bounds.x, f.bounds.y, dmg);
                        }
                    }
                }

                if dock_rect.intersects(dmg) {
                    let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                    canvas.push_clip(*dmg);
                    canvas.draw_rect(dock_x, dock_y, dock_w, dock_h, 0xDD0F172A, theme.pt(16.0) as usize);
                    canvas.draw_rect_outline(dock_x, dock_y, dock_w, dock_h, 0x44FFFFFF, theme.pt(16.0) as usize);
                    for (i, item) in self.dock_items.iter().enumerate() {
                        let ix = dock_x + theme.pt(12.0) + (i as i32 * theme.pt(52.0));
                        let iy = dock_y + theme.pt(7.0);
                        canvas.draw_rect(ix, iy, theme.pt(40.0), theme.pt(40.0), theme.accent, theme.pt(8.0) as usize);
                        let initial = item.chars().next().unwrap_or('A').to_string();
                        let (tw, th) = canvas.measure_text(&initial, theme.font_title());
                        canvas.draw_text(ix + ((theme.pt(40.0) - tw as i32) / 2), iy + ((theme.pt(40.0) - th as i32) / 2), &initial, 0xFFFFFFFF, theme.font_title());
                    }
                    canvas.pop_clip();
                }

                if menubar_rect.intersects(dmg) {
                    let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                    canvas.push_clip(*dmg);
                    self.menubar_widget.paint(&mut canvas);
                    canvas.pop_clip();
                }

                {
                    let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                    canvas.push_clip(*dmg);
                    canvas.draw_linux_cursor(self.input_mgr.pointer.x, self.input_mgr.pointer.y);
                    canvas.pop_clip();
                }
            }

            if self.show_debug_overlay {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                for dmg in &self.damage.rects {
                    canvas.draw_rect_outline(dmg.x, dmg.y, dmg.w, dmg.h, 0xAA00FF00, 0);
                }
                let stats = format!("FPS: 60 | Damage Rects: {}", self.damage.rects.len());
                canvas.draw_rect(10, self.height as i32 - 40, 260, 28, 0xDD000000, 4);
                canvas.draw_text(20, self.height as i32 - 34, &stats, 0xFF00FF00, 14);
            }

            let rect_tuples: Vec<[u32; 4]> = self.damage.rects.iter().map(|r| {
                [r.x.max(0) as u32, r.y.max(0) as u32, r.w.max(0) as u32, r.h.max(0) as u32]
            }).collect();
            sys_present_rects(self.buffer.as_ptr(), self.width, self.height, &rect_tuples);
            self.damage.clear();
        }
    }
}
