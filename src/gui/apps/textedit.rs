use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::input::{InputEvent, MOD_CTRL};
use super::App;
use crate::gui::draw::{self, FrameBuffer};
use crate::gui::theme::Theme;

pub struct TextEditApp {
    pub name: String,
    pub lines: Vec<String>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub scroll_y: usize,
    pub status: String,
    pub visible_lines: usize,
}

impl TextEditApp {
    pub fn new(name: String, content: String) -> Self {
        let lines: Vec<String> = content.lines().map(String::from).collect();
        Self { 
            name, 
            lines: if lines.is_empty() { vec![String::new()] } else { lines }, 
            cursor_x: 0, 
            cursor_y: 0, 
            scroll_y: 0, 
            status: String::from("Ready. Ctrl+S to save."),
            visible_lines: 20,
        }
    }

    fn save(&mut self) {
        let joined = self.lines.join("\n");
        if crate::fs::vfs_save_text_file(&self.name, joined.as_bytes()).is_ok() {
            self.status = String::from("Saved to Storage.");
        } else {
            self.status = String::from("Save error.");
        }
    }
}

impl App for TextEditApp {
    fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        draw::draw_rect(fb, x, y, w, h, theme.window_bg);
        let gutter_w = 40usize;
        draw::draw_rect(fb, x, y, gutter_w, h, theme.titlebar_bg);
        draw::draw_line_v(fb, x + gutter_w, y, h, theme.separator);

        let line_h = 22usize;
        let visible = h / line_h;
        for i in 0..visible {
            let idx = self.scroll_y + i;
            let row_y = y + 6 + (i * line_h);
            if idx < self.lines.len() {
                draw::draw_text(fb, x + 6, row_y, &alloc::format!("{:3}", idx + 1), theme.text_dim, 1);
                draw::draw_text(fb, x + gutter_w + 10, row_y, &self.lines[idx], theme.text_main, 1);
                if idx == self.cursor_y {
                    let cx = x + gutter_w + 10 + (self.cursor_x * 9);
                    draw::draw_line_v(fb, cx, row_y, 16, theme.accent);
                }
            }
        }
        
        let bar_h = 24usize;
        let bar_y = y + h.saturating_sub(bar_h);
        draw::draw_rect(fb, x, bar_y, w, bar_h, theme.titlebar_bg);
        draw::draw_line_h(fb, x, bar_y, w, theme.separator);
        draw::draw_text(fb, x + 10, bar_y + 6, &self.status, theme.text_dim, 1);
    }

    fn on_event(&mut self, event: &InputEvent, _mx: isize, _my: isize) {
        match event {
            InputEvent::KeyDown { keycode, mods } => {
                if *keycode == 0x1F && (mods & MOD_CTRL) != 0 { self.save(); }
                else if *keycode == 0x48 && self.cursor_y > 0 { self.cursor_y -= 1; self.cursor_x = self.cursor_x.min(self.lines[self.cursor_y].len()); }
                else if *keycode == 0x50 && self.cursor_y + 1 < self.lines.len() { self.cursor_y += 1; self.cursor_x = self.cursor_x.min(self.lines[self.cursor_y].len()); }
                else if *keycode == 0x4B && self.cursor_x > 0 { self.cursor_x -= 1; }
                else if *keycode == 0x4D && self.cursor_x < self.lines[self.cursor_y].len() { self.cursor_x += 1; }
            },
            InputEvent::Char(c) => {
                if *c == '\x08' {
                    if self.cursor_x > 0 {
                        self.lines[self.cursor_y].remove(self.cursor_x - 1);
                        self.cursor_x -= 1;
                    } else if self.cursor_y > 0 {
                        let text = self.lines.remove(self.cursor_y);
                        self.cursor_y -= 1;
                        self.cursor_x = self.lines[self.cursor_y].len();
                        self.lines[self.cursor_y].push_str(&text);
                    }
                } else if *c == '\n' {
                    let remainder = self.lines[self.cursor_y].split_off(self.cursor_x);
                    self.lines.insert(self.cursor_y + 1, remainder);
                    self.cursor_y += 1; self.cursor_x = 0;
                } else if *c >= ' ' && *c <= '~' {
                    self.lines[self.cursor_y].insert(self.cursor_x, *c);
                    self.cursor_x += 1;
                }
            },
            InputEvent::Scroll { dy } => {
                if *dy < 0 { self.scroll_y = self.scroll_y.saturating_add(1); }
                else { self.scroll_y = self.scroll_y.saturating_sub(1); }
            }
            _ => {}
        }
    }

    fn on_resize(&mut self, _new_w: usize, new_h: usize) {
        self.visible_lines = new_h / 22;
    }

    fn title(&self) -> &str { "TextEdit" }
    fn icon(&self) -> &'static str { "TXT" }
}
