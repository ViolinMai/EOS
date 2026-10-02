use std::collections::BTreeMap;
use std::time::Duration;
use std::thread;
use rusttype::Font;

use super::canvas::Canvas;
use super::widget::{Widget, WindowFrame};
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
}

impl<'a> FrameworkApp<'a> {
    pub fn new(font_bytes: &'a [u8], width: usize, height: usize) -> Self {
        let font = if !font_bytes.is_empty() {
            Font::try_from_bytes(font_bytes)
        } else {
            None
        };

        let mut buffer = Vec::new();
        buffer.resize(width * height, 0xFF0F172A);

        Self {
            width,
            height,
            buffer,
            font,
            font_cache: BTreeMap::new(),
            root_widgets: Vec::new(),
            mouse_x: width / 2,
            mouse_y: height / 2,
            mouse_clicked: false,
            dock_items: vec![
                ("Finder", "FND", 0xFF0284C7),
                ("Terminal", "TRM", 0xFF18181B),
                ("Settings", "SET", 0xFF475569),
                ("Activity", "ACT", 0xFFE11D48),
                ("TextEdit", "TXT", 0xFF0D9488),
            ],
            dock_hovered: None,
            on_dock_click: None,
        }
    }

    pub fn add_widget(&mut self, widget: Box<dyn Widget>) {
        self.root_widgets.push(widget);
    }

    pub fn set_dock_handler(&mut self, handler: impl FnMut(usize) + 'static) {
        self.on_dock_click = Some(Box::new(handler));
    }

    pub fn render_frame(&mut self) {
        self.buffer.fill(0xFF0F172A);

        {
            let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
            canvas.draw_rect(0, 0, self.width, 36, 0xFF18181B, 0);
            canvas.draw_text(20, 9, "EOS macOS Desktop (Sugoi UI Engine)", 0xFFFFFFFF, 15);
            canvas.draw_text(self.width - 240, 9, "SMP: 8 Cores | 60 FPS V-Sync", 0xFF38BDF8, 14);
        }

        {
            let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
            for w in &self.root_widgets {
                w.paint(&mut canvas);
            }
        }

        {
            let item_count = self.dock_items.len();
            let base_item_w = 54usize;
            let gap = 14usize;
            let total_dock_w = (item_count * base_item_w) + ((item_count + 1) * gap);
            let dock_h = 68usize;
            let dock_x = (self.width - total_dock_w) / 2;
            let dock_y = self.height - dock_h - 14;

            let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
            canvas.draw_rect(dock_x, dock_y, total_dock_w, dock_h, 0xFF1E293B, 20);

            let mut cur_x = dock_x + gap;
            for (idx, &(name, label, color)) in self.dock_items.iter().enumerate() {
                let is_hovered = self.dock_hovered == Some(idx);
                let sz = if is_hovered { 60 } else { base_item_w };
                let iy = if is_hovered { dock_y + 4 } else { dock_y + 8 };

                canvas.draw_rect(cur_x, iy, sz, sz, color, 14);
                canvas.draw_text(cur_x + 12, iy + 18, label, 0xFFFFFFFF, 15);

                if is_hovered {
                    let tip_w = name.len() * 9 + 20;
                    let tip_x = cur_x.saturating_add(sz / 2).saturating_sub(tip_w / 2);
                    let tip_y = dock_y.saturating_sub(32);
                    canvas.draw_rect(tip_x, tip_y, tip_w, 24, 0xFF334155, 6);
                    canvas.draw_text(tip_x + 10, tip_y + 4, name, 0xFFFFFFFF, 13);
                }

                cur_x += base_item_w + gap;
            }
        }

        {
            let cursor_col = if self.mouse_clicked { 0xFF22C55E } else { 0xFFFFFFFF };
            let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
            canvas.draw_rect(self.mouse_x.saturating_sub(2), self.mouse_y.saturating_sub(2), 14, 14, cursor_col, 7);
        }

        sdk::window::present(self.buffer.as_ptr(), self.width, self.height);
    }

    pub fn run_loop(&mut self) -> ! {
        for w in &mut self.root_widgets {
            w.layout(0, 0, self.width, self.height);
        }
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
                        
                        let new_x = (self.mouse_x as isize + dx as isize).clamp(0, (self.width - 1) as isize) as usize;
                        let new_y = (self.mouse_y as isize + dy as isize).clamp(0, (self.height - 1) as isize) as usize;
                        self.mouse_x = new_x;
                        self.mouse_y = new_y;
                    }
                    2 => {
                        self.mouse_clicked = (data >> 8) == 1;
                    }
                    4 => {
                        let c = (data as u8) as char;
                        for w in self.root_widgets.iter_mut().rev() {
                            if w.handle_char(c) {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }

            let item_count = self.dock_items.len();
            let base_item_w = 54usize;
            let gap = 14usize;
            let total_dock_w = (item_count * base_item_w) + ((item_count + 1) * gap);
            let dock_h = 68usize;
            let dock_x = (self.width - total_dock_w) / 2;
            let dock_y = self.height - dock_h - 14;

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
                            let mut found_pos = None;
                            for (pos, w) in self.root_widgets.iter().enumerate() {
                                if let Some(frame) = w.as_any().downcast_ref::<WindowFrame>() {
                                    if frame.title == app_name {
                                        found_pos = Some(pos);
                                        break;
                                    }
                                }
                            }
                            if let Some(pos) = found_pos {
                                let is_top = pos == self.root_widgets.len() - 1;
                                if let Some(frame) = self.root_widgets[pos].as_any_mut().downcast_mut::<WindowFrame>() {
                                    if frame.is_closed {
                                        frame.is_closed = false;
                                    } else if is_top {
                                        frame.is_closed = true;
                                    }
                                }
                                if !is_top {
                                    let win = self.root_widgets.remove(pos);
                                    self.root_widgets.push(win);
                                }
                            }
                            if let Some(ref mut cb) = self.on_dock_click {
                                cb(idx);
                            }
                        }
                        break;
                    }
                    cur_x += base_item_w + gap;
                }
            }

            if !dock_clicked && self.mouse_clicked && !last_click_state {
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
                        self.root_widgets.push(top_win);
                    }
                }
            }

            let mut handled = false;
            for w in self.root_widgets.iter_mut().rev() {
                if has_input && !handled && !dock_clicked {
                    if w.handle_mouse(self.mouse_x, self.mouse_y, self.mouse_clicked) {
                        handled = true;
                    }
                }
                w.layout(0, 0, self.width, self.height);
            }

            last_click_state = self.mouse_clicked;
            self.render_frame();
            thread::sleep(Duration::from_millis(16));
        }
    }
}
