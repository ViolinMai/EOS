use std::collections::BTreeMap;
use std::fs;
use rusttype::Font;
use crate::framework::canvas::Canvas;
use crate::framework::menu::MenuBarWidget;
use crate::framework::theme::{get_theme, set_theme_scale, DESKTOP_WALLPAPER};
use crate::framework::widget::{Widget, WindowFrame, Rect};
use crate::framework::layout::ContainerWidget;
use crate::apps::finder::FinderApp;
use crate::apps::settings::SettingsApp;
use crate::apps::terminal::TerminalApp;
use crate::apps::activity_monitor::ActivityMonitorApp;
use crate::apps::textedit::TextEditApp;
use crate::apps::browser::BrowserApp;
use crate::apps::preview::PreviewApp;
use crate::syscall::{sys_present, sys_wait_event, sys_poll_event};

pub struct FrameworkApp<'a> {
    pub buffer: Vec<u32>,
    pub width: usize,
    pub height: usize,
    pub font: Option<Font<'a>>,
    pub font_cache: BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>,
    pub root_widgets: Vec<Box<dyn Widget>>,
    pub menubar_widget: MenuBarWidget,
    pub dock_items: Vec<String>,
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub dragging_window_idx: Option<usize>,
    pub dock_handler: Option<Box<dyn FnMut(usize)>>,
}

impl<'a> FrameworkApp<'a> {
    pub fn new(font_bytes: &'a [u8], width: usize, height: usize) -> Self {
        let font = if !font_bytes.is_empty() {
            Font::try_from_bytes(font_bytes)
        } else {
            None
        };

        let mut menubar = MenuBarWidget::new();
        menubar.layout(0, 0, width as i32, get_theme().pt(32.0));

        let mut app = Self {
            buffer: vec![0; width * height],
            width,
            height,
            font,
            font_cache: BTreeMap::new(),
            root_widgets: Vec::new(),
            menubar_widget: menubar,
            dock_items: vec![
                "Finder".into(),
                "Settings".into(),
                "Terminal".into(),
                "Activity Monitor".into(),
                "TextEdit".into(),
                "Browser".into(),
            ],
            mouse_x: (width / 2) as i32,
            mouse_y: (height / 2) as i32,
            dragging_window_idx: None,
            dock_handler: None,
        };

        app.load_persisted_wallpaper();
        app
    }

    pub fn set_dock_handler<F: FnMut(usize) + 'static>(&mut self, handler: F) {
        self.dock_handler = Some(Box::new(handler));
    }

    fn load_persisted_wallpaper(&mut self) {
        if let Ok(content) = fs::read_to_string("settings.ini") {
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("wallpaper=") {
                    let path = val.trim();
                    if !path.is_empty() {
                        if let Ok(data) = fs::read(path) {
                            if data.len() >= 8 && &data[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
                                if let Ok((px, w, h)) = crate::png::decode_png(&data) {
                                    unsafe { DESKTOP_WALLPAPER = Some((px, w, h)); }
                                    return;
                                }
                            }
                            if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
                                let mut dec = zune_jpeg::JpegDecoder::new(&data);
                                if let Ok(raw) = dec.decode() {
                                    if let Some(info) = dec.info() {
                                        let w = info.width as usize;
                                        let h = info.height as usize;
                                        let mut px = vec![0u32; w * h];
                                        for i in 0..(w * h) {
                                            let o = i * 3;
                                            if o + 2 < raw.len() {
                                                px[i] = (0xFF << 24) | ((raw[o] as u32) << 16) | ((raw[o+1] as u32) << 8) | (raw[o+2] as u32);
                                            }
                                        }
                                        unsafe { DESKTOP_WALLPAPER = Some((px, w, h)); }
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

    pub fn spawn_app(&mut self, name: &str) {
        let theme = get_theme();
        for w in &mut self.root_widgets {
            if let Some(frame) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                if frame.title.contains(name) {
                    frame.is_closed = false;
                    frame.is_minimized = false;
                    frame.is_active = true;
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

        let win = Box::new(WindowFrame::new(
            name,
            theme.pt(80.0) + (self.root_widgets.len() as i32 * theme.pt(25.0)),
            theme.pt(60.0) + (self.root_widgets.len() as i32 * theme.pt(25.0)),
            theme.pt(620.0),
            theme.pt(420.0),
            content,
        ));
        self.root_widgets.push(win);
    }

    pub fn run_loop(&mut self) {
        loop {
            let theme = get_theme();

            let mut first_ev = sys_wait_event(16);
            while let Some(ev) = first_ev {
                let ev_type = ev[0];
                let dock_h = theme.pt(54.0);
                let dock_w = (self.dock_items.len() as i32) * theme.pt(52.0) + theme.pt(20.0);
                let dock_x = ((self.width as i32) - dock_w) / 2;
                let dock_y = (self.height as i32) - dock_h - theme.pt(12.0);

                match ev_type {
                    1 => {
                        let dx = (ev[1] as u32) as i32;
                        let dy = ((ev[1] >> 32) as u32) as i32;

                        self.mouse_x = (self.mouse_x + dx).clamp(0, self.width as i32 - 1);
                        self.mouse_y = (self.mouse_y + dy).clamp(0, self.height as i32 - 1);

                        if let Some(drag_idx) = self.dragging_window_idx {
                            if drag_idx < self.root_widgets.len() {
                                if let Some(frame) = self.root_widgets[drag_idx].as_any_mut().downcast_mut::<WindowFrame>() {
                                    if frame.is_dragging {
                                        let new_x = (self.mouse_x - frame.drag_offset_x).clamp(0, self.width as i32 - 100);
                                        let new_y = (self.mouse_y - frame.drag_offset_y).clamp(theme.pt(32.0), self.height as i32 - 60);
                                        frame.layout(new_x, new_y, frame.bounds.w, frame.bounds.h);
                                    }
                                }
                            }
                        } else {
                            for w in self.root_widgets.iter_mut().rev() {
                                if w.handle_mouse(self.mouse_x, self.mouse_y, false) {
                                    break;
                                }
                            }
                        }
                    }
                    2 => {
                        let btn = (ev[1] & 0xFF) as u8;
                        let pressed = ((ev[1] >> 8) & 0xFF) != 0;

                        if btn == 0 && !pressed {
                            self.dragging_window_idx = None;
                            for w in &mut self.root_widgets {
                                if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                    f.is_dragging = false;
                                }
                            }
                        }

                        if btn == 0 && pressed {
                            if self.mouse_y >= dock_y && self.mouse_y <= dock_y + dock_h && self.mouse_x >= dock_x && self.mouse_x <= dock_x + dock_w {
                                let idx = ((self.mouse_x - dock_x - theme.pt(12.0)) / theme.pt(52.0)) as usize;
                                if idx < self.dock_items.len() {
                                    if let Some(ref mut handler) = self.dock_handler {
                                        handler(idx);
                                    }
                                    let app_name = self.dock_items[idx].clone();
                                    self.spawn_app(&app_name);
                                    first_ev = sys_poll_event();
                                    continue;
                                }
                            }
                        }

                        let mut hit_idx = None;
                        for (idx, w) in self.root_widgets.iter_mut().enumerate().rev() {
                            if w.handle_mouse(self.mouse_x, self.mouse_y, pressed) {
                                hit_idx = Some(idx);
                                break;
                            }
                        }

                        if let Some(idx) = hit_idx {
                            if pressed {
                                for (i, w) in self.root_widgets.iter_mut().enumerate() {
                                    if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                        f.is_active = i == idx;
                                    }
                                }
                                let win = self.root_widgets.remove(idx);
                                self.root_widgets.push(win);
                                let last_idx = self.root_widgets.len() - 1;
                                if let Some(f) = self.root_widgets[last_idx].as_any_mut().downcast_mut::<WindowFrame>() {
                                    if f.is_dragging {
                                        self.dragging_window_idx = Some(last_idx);
                                    }
                                }
                            }
                        }
                    }
                    3 => {
                        let keycode = (ev[1] & 0xFF) as u8;
                        let mods = ((ev[1] >> 8) & 0xFF) as u8;
                        if let Some(active_win) = self.root_widgets.last_mut() {
                            active_win.handle_key(keycode, mods);
                        }
                    }
                    4 => {
                        let c_code = ev[1] as u32;
                        if let Some(c) = char::from_u32(c_code) {
                            if let Some(active_win) = self.root_widgets.last_mut() {
                                active_win.handle_char(c);
                            }
                        }
                    }
                    5 => {
                        let dy = (ev[1] as u32) as i32;
                        for w in self.root_widgets.iter_mut().rev() {
                            if w.handle_scroll(self.mouse_x, self.mouse_y, dy) {
                                break;
                            }
                        }
                    }
                    _ => {}
                }

                first_ev = sys_poll_event();
            }

            // فحص فتح الصور من Finder
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
                let prev_app = Box::new(PreviewApp::new(name.clone(), path));
                let win = Box::new(WindowFrame::new(
                    format!("Preview - {}", name),
                    theme.pt(100.0),
                    theme.pt(60.0),
                    theme.pt(680.0),
                    theme.pt(480.0),
                    prev_app,
                ));
                self.root_widgets.push(win);
            }

            // رسم خلفية سطح المكتب مع Aspect-Fill و Center-Crop
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
                            self.buffer[row_idx + x] = wp_px[src_row + sx];
                        }
                    }
                } else {
                    for p in &mut self.buffer {
                        *p = theme.bg_desktop;
                    }
                }
            }

            // رسم النوافذ والتطبيقات
            {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                for w in &self.root_widgets {
                    w.paint(&mut canvas);
                }
            }

            let dock_h = theme.pt(54.0);
            let dock_w = (self.dock_items.len() as i32) * theme.pt(52.0) + theme.pt(20.0);
            let dock_x = ((self.width as i32) - dock_w) / 2;
            let dock_y = (self.height as i32) - dock_h - theme.pt(12.0);

            {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);

                // رسم الـ Dock الشفاف مع Frosted Glass Blur
                canvas.draw_frosted_glass_rect(dock_x, dock_y, dock_w, dock_h, 0xCC0F172A, theme.pt(16.0) as usize);
                canvas.draw_rect_outline(dock_x, dock_y, dock_w, dock_h, 0x44FFFFFF, theme.pt(16.0) as usize);

                for (i, item) in self.dock_items.iter().enumerate() {
                    let ix = dock_x + theme.pt(12.0) + (i as i32 * theme.pt(52.0));
                    let iy = dock_y + theme.pt(7.0);
                    canvas.draw_rect(ix, iy, theme.pt(40.0), theme.pt(40.0), theme.accent, theme.pt(8.0) as usize);
                    let initial = item.chars().next().unwrap_or('A').to_string();
                    let (tw, th) = canvas.measure_text(&initial, theme.font_title());
                    canvas.draw_text(ix + ((theme.pt(40.0) - tw as i32) / 2), iy + ((theme.pt(40.0) - th as i32) / 2), &initial, 0xFFFFFFFF, theme.font_title());
                }

                // رسم شريط الـ MenuBar كـ Widget
                self.menubar_widget.paint(&mut canvas);

                canvas.draw_linux_cursor(self.mouse_x, self.mouse_y);
            }

            sys_present(self.buffer.as_ptr(), self.width, self.height);
        }
    }
}
