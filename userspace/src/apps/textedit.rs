use crate::framework::*;
use std::fs;

pub struct TextEditApp {
    pub bounds: Rect,
    pub title: String,
    pub file_path: Option<String>,
    pub lines: Vec<String>,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub scroll_y: usize,
    pub is_modified: bool,
    pub status_text: String,
    // Selection & Clipboard
    pub sel_start: Option<(usize, usize)>, // (row, col)
    pub is_selecting: bool,
    pub clipboard: String,
    // Search (Ctrl + F)
    pub find_active: bool,
    pub find_query: String,
}

impl TextEditApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            title: "Untitled.txt".into(),
            file_path: None,
            lines: vec![String::new()],
            cursor_col: 0,
            cursor_row: 0,
            scroll_y: 0,
            is_modified: false,
            status_text: String::from("Ready"),
            sel_start: None,
            is_selecting: false,
            clipboard: String::new(),
            find_active: false,
            find_query: String::new(),
        }
    }

    pub fn with_content(title: String, content: String) -> Self {
        let mut app = Self::new();
        app.title = title;
        app.lines = content.lines().map(|s| s.to_string()).collect();
        if app.lines.is_empty() { app.lines.push(String::new()); }
        app
    }

    pub fn open_file(path: &str) -> Self {
        let mut app = Self::new();
        app.load_from(path);
        app
    }

    pub fn load_from(&mut self, path: &str) {
        if let Ok(content) = fs::read_to_string(path) {
            self.lines = content.lines().map(|s| s.to_string()).collect();
            if self.lines.is_empty() { self.lines.push(String::new()); }
            self.file_path = Some(path.to_string());
            self.title = path.rsplit('/').next().unwrap_or(path).to_string();
            self.is_modified = false;
            self.status_text = format!("Opened: {}", path);
        } else {
            self.status_text = format!("Failed to read: {}", path);
        }
    }

    pub fn save(&mut self) {
        if let Some(ref path) = self.file_path {
            let content = self.lines.join("\n");
            if fs::write(path, content).is_ok() {
                self.is_modified = false;
                self.status_text = format!("Saved: {}", path);
            } else {
                self.status_text = format!("Error saving: {}", path);
            }
        }
    }

    fn ensure_cursor_visible(&mut self, visible_lines: usize) {
        if self.cursor_row < self.scroll_y {
            self.scroll_y = self.cursor_row;
        } else if self.cursor_row >= self.scroll_y + visible_lines {
            self.scroll_y = self.cursor_row.saturating_sub(visible_lines - 1);
        }
    }

    fn get_selected_text(&self) -> Option<String> {
        let start = self.sel_start?;
        let end = (self.cursor_row, self.cursor_col);
        let ((r1, c1), (r2, c2)) = if start <= end { (start, end) } else { (end, start) };

        if r1 == r2 {
            if c1 == c2 { return None; }
            let line = &self.lines[r1];
            let end_col = c2.min(line.len());
            let start_col = c1.min(end_col);
            return Some(line[start_col..end_col].to_string());
        }

        let mut res = Vec::new();
        res.push(self.lines[r1][c1.min(self.lines[r1].len())..].to_string());
        for r in (r1 + 1)..r2 {
            res.push(self.lines[r].clone());
        }
        res.push(self.lines[r2][..c2.min(self.lines[r2].len())].to_string());
        Some(res.join("\n"))
    }

    fn delete_selection(&mut self) {
        let start = match self.sel_start { Some(s) => s, None => return };
        let end = (self.cursor_row, self.cursor_col);
        let ((r1, c1), (r2, c2)) = if start <= end { (start, end) } else { (end, start) };

        if r1 == r2 {
            let line = &mut self.lines[r1];
            let end_col = c2.min(line.len());
            let start_col = c1.min(end_col);
            line.drain(start_col..end_col);
        } else {
            let tail = self.lines[r2][c2.min(self.lines[r2].len())..].to_string();
            self.lines[r1].truncate(c1);
            self.lines[r1].push_str(&tail);
            self.lines.drain((r1 + 1)..=r2);
        }

        self.cursor_row = r1;
        self.cursor_col = c1;
        self.sel_start = None;
        self.is_modified = true;
    }

    fn find_next(&mut self) {
        if self.find_query.is_empty() { return; }
        let total = self.lines.len();
        for i in 0..total {
            let r = (self.cursor_row + i) % total;
            let line = &self.lines[r];
            let start_idx = if i == 0 { self.cursor_col + 1 } else { 0 };
            if start_idx < line.len() {
                if let Some(pos) = line[start_idx..].to_lowercase().find(&self.find_query.to_lowercase()) {
                    self.cursor_row = r;
                    self.cursor_col = start_idx + pos;
                    self.sel_start = Some((r, self.cursor_col));
                    self.cursor_col += self.find_query.len();
                    self.status_text = format!("Found at Line {}", r + 1);
                    return;
                }
            }
        }
        self.status_text = format!("Not found: {}", self.find_query);
    }
}

impl Widget for TextEditApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF18181B, 0);

        let tb_h = theme.pt(32.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, 0xFF27272A, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, 0xFF3F3F46);

        let mod_indicator = if self.is_modified { " *" } else { "" };
        canvas.draw_text(self.bounds.x + theme.pt(12.0), self.bounds.y + theme.pt(6.0), &format!("TextEdit - {}{}", self.title, mod_indicator), 0xFFFFFFFF, theme.font_body());

        if self.find_active {
            let f_w = theme.pt(220.0);
            let f_x = self.bounds.x + self.bounds.w - f_w - theme.pt(10.0);
            canvas.draw_rect(f_x, self.bounds.y + theme.pt(4.0), f_w, theme.pt(24.0), 0xFF3F3F46, 4);
            canvas.draw_text(f_x + 6, self.bounds.y + theme.pt(6.0), &format!("Find: {}_", self.find_query), 0xFF38BDF8, theme.font_caption());
        }

        let line_h = theme.pt(20.0);
        let start_y = self.bounds.y + tb_h + theme.pt(4.0);
        let view_lines = ((self.bounds.h - tb_h - theme.pt(24.0)) / line_h).max(1) as usize;

        for (idx, line) in self.lines.iter().skip(self.scroll_y).take(view_lines).enumerate() {
            let cur_row = self.scroll_y + idx;
            let y = start_y + (idx as i32 * line_h);

            let num_str = format!("{:3} ", cur_row + 1);
            canvas.draw_text(self.bounds.x + theme.pt(8.0), y, &num_str, 0xFF71717A, theme.font_caption());

            let text_x = self.bounds.x + theme.pt(48.0);
            canvas.draw_text(text_x, y, line, 0xFFF4F4F5, theme.font_body());

            // Highlight selection
            if let Some(start) = self.sel_start {
                let end = (self.cursor_row, self.cursor_col);
                let (min_p, max_p) = if start <= end { (start, end) } else { (end, start) };
                if cur_row >= min_p.0 && cur_row <= max_p.0 {
                    let c_start = if cur_row == min_p.0 { min_p.1 } else { 0 };
                    let c_end = if cur_row == max_p.0 { max_p.1 } else { line.len() };
                    let (w_pre, _) = canvas.measure_text(&line[..c_start.min(line.len())], theme.font_body());
                    let (w_sel, _) = canvas.measure_text(&line[c_start.min(line.len())..c_end.min(line.len())], theme.font_body());
                    canvas.draw_rect(text_x + w_pre as i32, y, w_sel as i32, line_h, 0x5538BDF8, 0);
                }
            }

            // Draw cursor
            if cur_row == self.cursor_row {
                let sub = if self.cursor_col < line.len() { &line[..self.cursor_col] } else { line };
                let (cw, _) = canvas.measure_text(sub, theme.font_body());
                canvas.draw_line_v(text_x + cw as i32, y, line_h, 0xFF38BDF8);
            }
        }

        let sb_y = self.bounds.y + self.bounds.h - theme.pt(22.0);
        canvas.draw_rect(self.bounds.x, sb_y, self.bounds.w, theme.pt(22.0), 0xFF27272A, 0);
        canvas.draw_line_h(self.bounds.x, sb_y, self.bounds.w, 0xFF3F3F46);
        let info = format!("Ln {}, Col {} | {}", self.cursor_row + 1, self.cursor_col + 1, self.status_text);
        canvas.draw_text(self.bounds.x + theme.pt(12.0), sb_y + theme.pt(4.0), &info, 0xFFA1A1AA, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let tb_h = theme.pt(32.0);
        let line_h = theme.pt(20.0);
        if my > self.bounds.y + tb_h && my < self.bounds.y + self.bounds.h - theme.pt(22.0) {
            let row = ((my - self.bounds.y - tb_h - theme.pt(4.0)) / line_h) as usize + self.scroll_y;
            if row < self.lines.len() {
                self.cursor_row = row;
                self.cursor_col = ((mx - self.bounds.x - theme.pt(48.0)) / theme.pt(8.0)).max(0) as usize;
                self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
                if pressed {
                    self.sel_start = Some((self.cursor_row, self.cursor_col));
                }
            }
        }
        true
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if dy > 0 {
            self.scroll_y = self.scroll_y.saturating_add(3).min(self.lines.len().saturating_sub(1));
        } else {
            self.scroll_y = self.scroll_y.saturating_sub(3);
        }
        true
    }

    fn handle_key(&mut self, keycode: u8, mods: u8) -> bool {
        let is_ctrl = (mods & (1 << 1)) != 0;
        let is_shift = (mods & (1 << 0)) != 0;
        let theme = get_theme();
        let visible_lines = ((self.bounds.h - theme.pt(54.0)) / theme.pt(20.0)).max(1) as usize;

        if is_ctrl {
            match keycode {
                0x1F => { self.save(); return true; } // Ctrl+S
                0x2E => { // Ctrl+C (Copy)
                    if let Some(text) = self.get_selected_text() {
                        self.clipboard = text;
                        self.status_text = "Copied to clipboard".into();
                    }
                    return true;
                }
                0x2D => { // Ctrl+X (Cut)
                    if let Some(text) = self.get_selected_text() {
                        self.clipboard = text;
                        self.delete_selection();
                        self.status_text = "Cut to clipboard".into();
                    }
                    return true;
                }
                0x2F => { // Ctrl+V (Paste)
                    if !self.clipboard.is_empty() {
                        self.delete_selection();
                        let clip = self.clipboard.clone();
                        for c in clip.chars() {
                            if c == '\n' {
                                let remainder = self.lines[self.cursor_row][self.cursor_col..].to_string();
                                self.lines[self.cursor_row].truncate(self.cursor_col);
                                self.lines.insert(self.cursor_row + 1, remainder);
                                self.cursor_row += 1;
                                self.cursor_col = 0;
                            } else {
                                self.lines[self.cursor_row].insert(self.cursor_col, c);
                                self.cursor_col += 1;
                            }
                        }
                        self.is_modified = true;
                    }
                    return true;
                }
                0x21 => { // Ctrl+F (Find)
                    self.find_active = !self.find_active;
                    if self.find_active { self.find_query.clear(); }
                    return true;
                }
                0x1E => { // Ctrl+A (Select All)
                    self.sel_start = Some((0, 0));
                    self.cursor_row = self.lines.len() - 1;
                    self.cursor_col = self.lines.last().map(|l| l.len()).unwrap_or(0);
                    return true;
                }
                _ => {}
            }
        }

        if is_shift && self.sel_start.is_none() {
            self.sel_start = Some((self.cursor_row, self.cursor_col));
        } else if !is_shift && !is_ctrl && (keycode == 0x48 || keycode == 0x50 || keycode == 0x4B || keycode == 0x4D) {
            self.sel_start = None;
        }

        match keycode {
            0x48 => { // Up
                if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
                    self.ensure_cursor_visible(visible_lines);
                }
                true
            }
            0x50 => { // Down
                if self.cursor_row + 1 < self.lines.len() {
                    self.cursor_row += 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
                    self.ensure_cursor_visible(visible_lines);
                }
                true
            }
            0x4B => { // Left
                if self.cursor_col > 0 { self.cursor_col -= 1; }
                true
            }
            0x4D => { // Right
                if self.cursor_col < self.lines[self.cursor_row].len() { self.cursor_col += 1; }
                true
            }
            0x0E => { // Backspace
                if self.sel_start.is_some() {
                    self.delete_selection();
                } else if self.cursor_col > 0 {
                    self.lines[self.cursor_row].remove(self.cursor_col - 1);
                    self.cursor_col -= 1;
                    self.is_modified = true;
                } else if self.cursor_row > 0 {
                    let prev = self.lines.remove(self.cursor_row);
                    self.cursor_row -= 1;
                    self.cursor_col = self.lines[self.cursor_row].len();
                    self.lines[self.cursor_row].push_str(&prev);
                    self.is_modified = true;
                }
                self.ensure_cursor_visible(visible_lines);
                true
            }
            0x1C => { // Enter
                if self.find_active {
                    self.find_next();
                    return true;
                }
                self.delete_selection();
                let cur_str = &self.lines[self.cursor_row];
                let remainder = cur_str[self.cursor_col..].to_string();
                self.lines[self.cursor_row].truncate(self.cursor_col);
                self.lines.insert(self.cursor_row + 1, remainder);
                self.cursor_row += 1;
                self.cursor_col = 0;
                self.is_modified = true;
                self.ensure_cursor_visible(visible_lines);
                true
            }
            _ => false,
        }
    }

    fn handle_char(&mut self, c: char) -> bool {
        if self.find_active {
            if c == '\x08' { self.find_query.pop(); }
            else if c >= ' ' && c != '\x7F' { self.find_query.push(c); self.find_next(); }
            return true;
        }

        if c >= ' ' && c != '\x7F' {
            self.delete_selection();
            self.lines[self.cursor_row].insert(self.cursor_col, c);
            self.cursor_col += 1;
            self.is_modified = true;
            let theme = get_theme();
            let visible_lines = ((self.bounds.h - theme.pt(54.0)) / theme.pt(20.0)).max(1) as usize;
            self.ensure_cursor_visible(visible_lines);
            return true;
        }
        false
    }
}
