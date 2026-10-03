use std::collections::BTreeMap;
use std::time::Duration;
use std::thread;
use rusttype::Font;

use super::canvas::Canvas;
use super::widget::{Widget, WindowFrame, Rect};
use super::theme::{get_theme, set_theme_scale};
use super::menu::MenuBar;
use crate::apps::{FinderApp, TerminalApp, SettingsApp, TextEditApp, ActivityMonitorApp, PreviewApp};
use crate::sdk;

pub struct FrameworkApp<'a> {
    pub width: usize,
    pub height: usize,
    pub buffer: Vec<u32>,
    font: Option<Font<'a>>,
    font_cache: BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>,
    pub root_widgets: Vec<Box<dyn Widget>>,
    pub mouse_x: usize,
    pub mouse_y: usize,
    pub mouse_clicked: bool,
    pub dock_items: Vec<(&'static str, &'static str, u32)>,
    pub dock_hovered: Option<usize>,
    pub on_dock_click: Option<Box<dyn FnMut(usize)>>,
    pub menubar: MenuBar,
    pub active_app_title: String,
    pub needs_redraw: bool,
}

impl<'a> FrameworkApp<'a> {
    pub fn new(font_bytes: &'a [u8], width: usize, height: usize) -> Self {
        let font = if !font_bytes.is_empty() && crate::is_valid_truetype_font(font_bytes) {
            Font::try_from_bytes(font_bytes)
        } else {
            None
        };

        let mut buffer = Vec::new();
        buffer.resize(width * height, 0xFF0F172A);

        set_theme_scale(1.5);
        let mut menubar = MenuBar::new();
        menubar.bounds = Rect::new(0, 0, width, get_theme().pt(32.0));

        Self {
            width, height, buffer, font, font_cache: BTreeMap::new(),
            root_widgets: Vec::new(),
            mouse_x: width / 2, mouse_y: height / 2,
            mouse_clicked: false,
            dock_items: vec![
                ("Finder", "FND", 0xFF0284C7),
                ("Terminal", "TRM", 0xFF18181B),
                ("Activity Monitor", "ACT", 0xFFE11D48),
                ("TextEdit", "TXT", 0xFF0D9488),
                ("Settings", "SET", 0xFF475569),
            ],
            dock_hovered: None,
            on_dock_click: None,
            menubar,
            active_app_title: "Finder".into(),
            needs_redraw: true,
        }
    }

    pub fn set_dock_handler(&mut self, handler: impl FnMut(usize) + 'static) {
        self.on_dock_click = Some(Box::new(handler));
    }

    pub fn spawn_app(&mut self, name: &str) {
        for w in &mut self.root_widgets {
            if let Some(frame) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                if frame.title == name {
                    frame.is_closed = false;
                    self.active_app_title = name.to_string();
                    self.needs_redraw = true;
                    return;
                }
            }
        }

        let theme = get_theme();
        let new_win: Box<dyn Widget> = match name {
            "Finder" => Box::new(WindowFrame::new("Finder", theme.pt(80.0), theme.pt(50.0), theme.pt(780.0), theme.pt(480.0), Box::new(FinderApp::new()))),
            "Terminal" => Box::new(WindowFrame::new("Terminal", theme.pt(140.0), theme.pt(100.0), theme.pt(640.0), theme.pt(420.0), Box::new(TerminalApp::new()))),
            "Activity Monitor" => Box::new(WindowFrame::new("Activity Monitor", theme.pt(160.0), theme.pt(90.0), theme.pt(680.0), theme.pt(440.0), Box::new(ActivityMonitorApp::new()))),
            "TextEdit" => Box::new(WindowFrame::new("TextEdit", theme.pt(180.0), theme.pt(110.0), theme.pt(620.0), theme.pt(440.0), Box::new(TextEditApp::new()))),
            "Settings" => Box::new(WindowFrame::new("Settings", theme.pt(200.0), theme.pt(80.0), theme.pt(600.0), theme.pt(400.0), Box::new(SettingsApp::new()))),
            _ => return,
        };

        self.active_app_title = name.to_string();
        self.root_widgets.push(new_win);
        self.needs_redraw = true;
    }

    pub fn spawn_preview(&mut self, name: String, data: Vec<u8>) {
        let theme = get_theme();
        let win = Box::new(WindowFrame::new(format!("Preview - {}", name), theme.pt(160.0), theme.pt(90.0), theme.pt(640.0), theme.pt(460.0), Box::new(PreviewApp::new(name, data))));
        self.root_widgets.push(win);
        self.needs_redraw = true;
    }

    pub fn render_frame(&mut self) {
        let theme = get_theme();
        self.buffer.fill(theme.bg_desktop);

        // 1. Draw Windows
        {
            let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
            for w in &self.root_widgets {
                w.paint(&mut canvas);
            }
        }

        // 2. Top MenuBar
        {
            let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
            self.menubar.paint(&mut canvas, &self.active_app_title);
        }

        // 3. Dock
        {
            let item_count = self.dock_items.len();
            let base_item_w = theme.pt(52.0);
            let gap = theme.pt(14.0);
            let total_dock_w = (item_count * base_item_w) + ((item_count + 1) * gap);
            let dock_h = theme.pt(68.0);
            let dock_x = (self.width.saturating_sub(total_dock_w)) / 2;
            let dock_y = self.height.saturating_sub(dock_h + theme.pt(14.0));

            let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
            canvas.draw_rect(dock_x, dock_y, total_dock_w, dock_h, theme.bg_dock, theme.pt(20.0));
            canvas.draw_rect_outline(dock_x, dock_y, total_dock_w, dock_h, theme.border_dock, theme.pt(20.0));

            let mut cur_x = dock_x + gap;
            for (idx, &(name, label, color)) in self.dock_items.iter().enumerate() {
                let is_hovered = self.dock_hovered == Some(idx);
                let sz = if is_hovered { theme.pt(58.0) } else { base_item_w };
                let iy = if is_hovered { dock_y + theme.pt(4.0) } else { dock_y + theme.pt(8.0) };

                canvas.draw_rect(cur_x, iy, sz, sz, color, theme.pt(14.0));
                let (lw, _) = canvas.measure_text(label, theme.font_body());
                canvas.draw_text(cur_x + (sz.saturating_sub(lw) / 2), iy + (sz / 3), label, 0xFFFFFFFF, theme.font_body());

                let is_running = self.root_widgets.iter().any(|w| {
                    if let Some(frame) = w.as_any().downcast_ref::<WindowFrame>() {
                        frame.title.contains(name) && !frame.is_closed
                    } else { false }
                });
                if is_running {
                    canvas.draw_rect(cur_x + (sz / 2) - 3, dock_y + dock_h - theme.pt(6.0), 6, 6, theme.accent_hover, 3);
                }

                if is_hovered {
                    let tip_w = name.len() * theme.pt(8.0) + theme.pt(24.0);
                    let tip_x = cur_x.saturating_add(sz / 2).saturating_sub(tip_w / 2);
                    let tip_y = dock_y.saturating_sub(theme.pt(32.0));
                    canvas.draw_rect(tip_x, tip_y, tip_w, theme.pt(24.0), theme.bg_dock, theme.pt(6.0));
                    canvas.draw_rect_outline(tip_x, tip_y, tip_w, theme.pt(24.0), theme.border_dock, theme.pt(6.0));
                    canvas.draw_text(tip_x + theme.pt(10.0), tip_y + theme.pt(4.0), name, theme.text_primary, theme.font_caption());
                }

                cur_x += base_item_w + gap;
            }
        }

        // 4. Cursor
        {
            let cursor_col = if self.mouse_clicked { theme.accent_hover } else { 0xFFFFFFFF };
            let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
            let cur_sz = theme.pt(12.0);
            canvas.draw_rect(self.mouse_x.saturating_sub(2), self.mouse_y.saturating_sub(2), cur_sz, cur_sz, cursor_col, cur_sz / 2);
        }

        // Direct Full Frame Present
        sdk::window::present(self.buffer.as_ptr(), self.width, self.height);
    }

    pub fn run_loop(&mut self) -> ! {
        self.render_frame();
        let mut last_click_state = false;

        loop {
            let mut has_input = false;
            while let Some(ev) = sdk::window::poll_event() {
                has_input = true;
                let (etype, data) = (ev[0], ev[1]);
                match etype {
                    1 => {
                        let dx = (data & 0xFFFFFFFF) as i32;
                        let dy = ((data >> 32) & 0xFFFFFFFF) as i32;
                        self.mouse_x = (self.mouse_x as isize + dx as isize).clamp(0, (self.width - 1) as isize) as usize;
                        self.mouse_y = (self.mouse_y as isize + dy as isize).clamp(0, (self.height - 1) as isize) as usize;
                    }
                    2 => {
                        self.mouse_clicked = (data >> 8) == 1;
                    }
                    4 => {
                        let c = (data as u8) as char;
                        for w in self.root_widgets.iter_mut().rev() {
                            if w.handle_char(c) { break; }
                        }
                    }
                    _ => {}
                }
            }

            if has_input { self.needs_redraw = true; }

            let mut pending_img = None;
            for w in &mut self.root_widgets {
                if let Some(frame) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                    if let Some(finder) = frame.content.as_any_mut().downcast_mut::<FinderApp>() {
                        if let Some(img) = finder.pending_open_image.take() {
                            pending_img = Some(img);
                            break;
                        }
                    }
                }
            }
            if let Some((name, data)) = pending_img {
                self.spawn_preview(name, data);
            }

            if let Some(action) = self.menubar.handle_mouse(self.mouse_x, self.mouse_y, self.mouse_clicked && !last_click_state) {
                match action {
                    101 => println!("[MENU] About EOS macOS"),
                    102 => self.spawn_app("Settings"),
                    103 => println!("[MENU] Reboot requested"),
                    201 => self.spawn_app(&self.active_app_title.clone()),
                    203 => {
                        if let Some(top) = self.root_widgets.last_mut() {
                            if let Some(frame) = top.as_any_mut().downcast_mut::<WindowFrame>() {
                                frame.is_closed = true;
                            }
                        }
                    }
                    _ => println!("[MENU] Action triggered: {}", action),
                }
                self.needs_redraw = true;
            }

            let theme = get_theme();
            let item_count = self.dock_items.len();
            let base_item_w = theme.pt(52.0);
            let gap = theme.pt(14.0);
            let total_dock_w = (item_count * base_item_w) + ((item_count + 1) * gap);
            let dock_h = theme.pt(68.0);
            let dock_x = (self.width.saturating_sub(total_dock_w)) / 2;
            let dock_y = self.height.saturating_sub(dock_h + theme.pt(14.0));

            let old_dock_hover = self.dock_hovered;
            self.dock_hovered = None;
            let mut dock_clicked = false;

            if self.mouse_y >= dock_y && self.mouse_y <= dock_y + dock_h && self.mouse_x >= dock_x && self.mouse_x <= dock_x + total_dock_w {
                let mut cur_x = dock_x + gap;
                for idx in 0..item_count {
                    if self.mouse_x >= cur_x && self.mouse_x <= cur_x + base_item_w {
                        self.dock_hovered = Some(idx);
                        if self.mouse_clicked && !last_click_state {
                            dock_clicked = true;
                            let app_name = self.dock_items[idx].0;
                            self.spawn_app(app_name);
                        }
                        break;
                    }
                    cur_x += base_item_w + gap;
                }
            }

            if old_dock_hover != self.dock_hovered {
                self.needs_redraw = true;
            }

            if !dock_clicked && self.mouse_clicked && !last_click_state && self.mouse_y > self.menubar.bounds.h {
                let mut clicked_win_idx = None;
                for (idx, w) in self.root_widgets.iter_mut().enumerate().rev() {
                    if w.handle_mouse(self.mouse_x, self.mouse_y, false) {
                        clicked_win_idx = Some(idx);
                        break;
                    }
                }
                if let Some(idx) = clicked_win_idx {
                    if idx < self.root_widgets.len() - 1 {
                        let top_win = self.root_widgets.remove(idx);
                        if let Some(frame) = top_win.as_any().downcast_ref::<WindowFrame>() {
                            self.active_app_title = frame.title.clone();
                        }
                        self.root_widgets.push(top_win);
                        self.needs_redraw = true;
                    }
                }
            }

            let mut handled = false;
            for w in self.root_widgets.iter_mut().rev() {
                if has_input && !handled && !dock_clicked && self.mouse_y > self.menubar.bounds.h {
                    if w.handle_mouse(self.mouse_x, self.mouse_y, self.mouse_clicked) {
                        handled = true;
                        self.needs_redraw = true;
                    }
                }
                w.layout(0, 0, self.width, self.height);
            }

            last_click_state = self.mouse_clicked;

            if self.needs_redraw {
                self.render_frame();
                self.needs_redraw = false;
            } else {
                thread::sleep(Duration::from_millis(8));
            }
        }
    }
}
