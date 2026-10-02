use std::fs;
use crate::framework::*;

#[derive(Clone)]
pub struct FileEntry {
    pub name: String,
    pub size: usize,
    pub is_dir: bool,
}

pub struct FinderApp {
    bounds: Rect,
    sidebar_items: Vec<&'static str>,
    selected_sidebar: usize,
    columns: Vec<(Vec<FileEntry>, Option<usize>)>,
    active_path: Vec<String>,
}

impl FinderApp {
    pub fn new() -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            sidebar_items: vec!["EOS SHARE", "RootFS", "Initrd"],
            selected_sidebar: 0,
            columns: Vec::new(),
            active_path: vec!["/EOS SHARE".into()],
        };
        app.load_root();
        app
    }

    fn read_entries(path: &str) -> Vec<FileEntry> {
        let mut list = Vec::new();
        let clean_path = if path == "/" { "." } else { path.trim_start_matches('/') };

        if let Ok(entries) = fs::read_dir(path).or_else(|_| fs::read_dir(clean_path)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "." || name == ".." { continue; }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let size = entry.metadata().map(|m| m.len() as usize).unwrap_or(0);
                list.push(FileEntry { name, size, is_dir });
            }
        }

        list.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        list
    }

    fn load_root(&mut self) {
        self.columns.clear();
        let target = match self.selected_sidebar {
            0 => "/EOS SHARE",
            1 => "/RootFS",
            2 => "/Initrd",
            _ => "/EOS SHARE",
        };
        self.active_path = vec![target.to_string()];
        let root_files = Self::read_entries(target);
        self.columns.push((root_files, None));
    }
}

impl Widget for FinderApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect { self.bounds = Rect { x, y, w, h }; self.bounds }

    fn paint(&self, canvas: &mut Canvas) {
        // Scaled Toolbar (Height: 52px, Font: 20px)
        let tb_h = 52usize;
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, 0xFF1E293B, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, 0xFF334155);
        let current_full_path = self.active_path.join("/");
        canvas.draw_text(self.bounds.x + 24, self.bounds.y + 14, &format!("📂 {}", current_full_path), 0xFFF8FAFC, 20);

        // Scaled Sidebar (Width: 200px)
        let sb_w = 200usize;
        let content_y = self.bounds.y + tb_h + 1;
        let content_h = self.bounds.h.saturating_sub(tb_h + 1);
        canvas.draw_rect(self.bounds.x, content_y, sb_w, content_h, 0xFF0F172A, 0);
        canvas.draw_line_v(self.bounds.x + sb_w, content_y, content_h, 0xFF334155);

        canvas.draw_text(self.bounds.x + 18, content_y + 16, "FAVORITES", 0xFF64748B, 15);
        for (i, item) in self.sidebar_items.iter().enumerate() {
            let ry = content_y + 44 + (i * 40);
            if self.selected_sidebar == i {
                canvas.draw_rect(self.bounds.x + 10, ry, sb_w - 20, 34, 0xFF0284C7, 8);
                canvas.draw_text(self.bounds.x + 22, ry + 7, item, 0xFFFFFFFF, 18);
            } else {
                canvas.draw_text(self.bounds.x + 22, ry + 7, item, 0xFFCBD5E1, 18);
            }
        }

        // Scaled Columns View (Width: 280px, Row Height: 34px)
        let col_w = 280usize;
        let row_h = 34usize;
        let mut cur_x = self.bounds.x + sb_w + 1;

        for (col_idx, (files, selected)) in self.columns.iter().enumerate() {
            if cur_x + col_w > self.bounds.x + self.bounds.w { break; }
            canvas.draw_rect(cur_x, content_y, col_w, content_h, if col_idx % 2 == 0 { 0xFF182234 } else { 0xFF1E293B }, 0);
            canvas.draw_line_v(cur_x + col_w, content_y, content_h, 0xFF334155);

            let mut ry = content_y + 10;
            for (idx, f) in files.iter().enumerate() {
                if ry + row_h > content_y + content_h { break; }
                let is_sel = *selected == Some(idx);
                if is_sel {
                    canvas.draw_rect(cur_x + 6, ry, col_w - 12, row_h - 4, 0xFF0284C7, 6);
                }
                let icon = if f.is_dir { "📁" } else { "📄" };
                let txt_color = if is_sel { 0xFFFFFFFF } else { 0xFFE2E8F0 };
                canvas.draw_text(cur_x + 12, ry + 6, &format!("{} {}", icon, f.name), txt_color, 16);
                if f.is_dir {
                    canvas.draw_text(cur_x + col_w - 24, ry + 6, "›", 0xFF94A3B8, 18);
                }
                ry += row_h;
            }
            cur_x += col_w + 1;
        }

        // Scaled Inspector Pane
        if cur_x < self.bounds.x + self.bounds.w {
            let insp_w = (self.bounds.x + self.bounds.w).saturating_sub(cur_x);
            canvas.draw_rect(cur_x, content_y, insp_w, content_h, 0xFF0F172A, 0);
            if let Some((files, Some(sel))) = self.columns.last() {
                if let Some(target) = files.get(*sel) {
                    canvas.draw_rect(cur_x + 28, content_y + 28, 84, 84, 0xFF334155, 16);
                    canvas.draw_text(cur_x + 48, content_y + 58, if target.is_dir { "DIR" } else { "DOC" }, 0xFF38BDF8, 22);
                    canvas.draw_text(cur_x + 28, content_y + 135, &target.name, 0xFFFFFFFF, 18);
                    canvas.draw_text(cur_x + 28, content_y + 170, &format!("Size: {} Bytes", target.size), 0xFF94A3B8, 16);
                    canvas.draw_text(cur_x + 28, content_y + 200, if target.is_dir { "Folder" } else { "Document" }, 0xFF94A3B8, 16);
                }
            } else {
                canvas.draw_text(cur_x + 28, content_y + 48, "No item selected", 0xFF64748B, 18);
            }
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if !pressed { return true; }

        let tb_h = 52usize;
        let sb_w = 200usize;
        let content_y = self.bounds.y + tb_h + 1;

        if mx < self.bounds.x + sb_w && my >= content_y {
            let idx = (my.saturating_sub(content_y + 44)) / 40;
            if idx < self.sidebar_items.len() {
                self.selected_sidebar = idx;
                self.load_root();
                return true;
            }
        }

        let col_w = 281usize;
        let row_h = 34usize;
        let mut cur_x = self.bounds.x + sb_w + 1;

        for col_idx in 0..self.columns.len() {
            if mx >= cur_x && mx < cur_x + col_w {
                let row_idx = (my.saturating_sub(content_y + 10)) / row_h;
                if row_idx < self.columns[col_idx].0.len() {
                    self.columns[col_idx].1 = Some(row_idx);
                    let item = self.columns[col_idx].0[row_idx].clone();

                    self.columns.truncate(col_idx + 1);
                    self.active_path.truncate(col_idx + 1);

                    if item.is_dir {
                        self.active_path.push(item.name.clone());
                        let next_path = self.active_path.join("/");
                        let sub_entries = Self::read_entries(&next_path);
                        self.columns.push((sub_entries, None));
                    }
                    return true;
                }
            }
            cur_x += col_w;
        }
        true
    }
}

pub struct TerminalApp {
    bounds: Rect,
    history: Vec<(String, u32)>,
    input: String,
    cwd: String,
}

impl TerminalApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            history: vec![
                ("EOS Terminal v2.0 (x86_64-unknown-linux-musl)".into(), 0xFF94A3B8),
                ("Type 'help' for available commands.".into(), 0xFF64748B),
            ],
            input: String::new(),
            cwd: "/EOS SHARE".into(),
        }
    }

    fn execute(&mut self) {
        let cmd = self.input.trim().to_string();
        self.history.push((format!("{}> {}", self.cwd, cmd), 0xFF38BDF8));
        self.input.clear();

        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() { return; }

        match parts[0] {
            "help" => {
                self.history.push(("Available commands:".into(), 0xFFF8FAFC));
                self.history.push(("  ls [path]    - List files in folder".into(), 0xFFCBD5E1));
                self.history.push(("  cd <dir>     - Change directory".into(), 0xFFCBD5E1));
                self.history.push(("  pwd          - Current directory".into(), 0xFFCBD5E1));
                self.history.push(("  cat <file>   - Display file".into(), 0xFFCBD5E1));
                self.history.push(("  clear        - Clear console".into(), 0xFFCBD5E1));
            }
            "clear" => self.history.clear(),
            "pwd" => self.history.push((self.cwd.clone(), 0xFF22C55E)),
            "cd" => {
                if parts.len() > 1 {
                    self.cwd = parts[1].to_string();
                } else {
                    self.cwd = "/EOS SHARE".into();
                }
            }
            "ls" => {
                let target_dir = if parts.len() > 1 { parts[1] } else { &self.cwd };
                if let Ok(entries) = fs::read_dir(target_dir).or_else(|_| fs::read_dir(target_dir.trim_start_matches('/'))) {
                    let mut line = String::new();
                    for e in entries.flatten() {
                        let is_d = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        let name = e.file_name().to_string_lossy().to_string();
                        line.push_str(&format!("{} {}   ", if is_d { "📁" } else { "📄" }, name));
                    }
                    if !line.is_empty() {
                        self.history.push((line, 0xFFFFFFFF));
                    } else {
                        self.history.push(("(empty directory)".into(), 0xFF94A3B8));
                    }
                } else {
                    self.history.push((format!("ls: cannot access '{}': No such file or directory", target_dir), 0xFFEF4444));
                }
            }
            "cat" => {
                if parts.len() < 2 {
                    self.history.push(("Usage: cat <filename>".into(), 0xFFEF4444));
                } else {
                    let p = parts[1];
                    let full_path = if p.starts_with('/') { p.to_string() } else { format!("{}/{}", self.cwd, p) };
                    if let Ok(content) = fs::read_to_string(&full_path).or_else(|_| fs::read_to_string(p)) {
                        for l in content.lines() {
                            self.history.push((l.to_string(), 0xFFE2E8F0));
                        }
                    } else {
                        self.history.push((format!("cat: {}: No such file", p), 0xFFEF4444));
                    }
                }
            }
            _ => self.history.push((format!("command not found: {}", parts[0]), 0xFFEF4444)),
        }
    }
}

impl Widget for TerminalApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect { self.bounds = Rect { x, y, w, h }; self.bounds }

    fn paint(&self, canvas: &mut Canvas) {
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF090D16, 0);

        let line_h = 28usize;
        let max_lines = self.bounds.h.saturating_sub(44) / line_h;
        let start = if self.history.len() > max_lines { self.history.len() - max_lines } else { 0 };

        let mut cy = self.bounds.y + 14;
        for (line, color) in &self.history[start..] {
            canvas.draw_text(self.bounds.x + 20, cy, line, *color, 18);
            cy += line_h;
        }
        let prompt = format!("{}> {}_", self.cwd, self.input);
        canvas.draw_text(self.bounds.x + 20, cy, &prompt, 0xFFFFFFFF, 18);
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, _p: bool) -> bool { self.bounds.contains(mx, my) }

    fn handle_char(&mut self, c: char) -> bool {
        if c == '\n' {
            self.execute();
            return true;
        } else if c == '\x08' {
            self.input.pop();
            return true;
        } else if c >= ' ' && c <= '~' {
            self.input.push(c);
            return true;
        }
        false
    }
}

pub struct TextEditApp {
    bounds: Rect,
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

impl TextEditApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            lines: vec!["EOS TextEdit Editor".into(), "Type notes or code here...".into()],
            cursor_row: 1,
            cursor_col: 26,
        }
    }
}

impl Widget for TextEditApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect { self.bounds = Rect { x, y, w, h }; self.bounds }

    fn paint(&self, canvas: &mut Canvas) {
        // Scaled Toolbar (Height: 46px, Font: 18px)
        let tb_h = 46usize;
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, 0xFF1E293B, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, 0xFF334155);
        canvas.draw_text(self.bounds.x + 20, self.bounds.y + 12, "💾 TextEdit", 0xFFFFFFFF, 18);

        let editor_y = self.bounds.y + tb_h + 1;
        let editor_h = self.bounds.h.saturating_sub(tb_h + 36);
        canvas.draw_rect(self.bounds.x, editor_y, self.bounds.w, editor_h, 0xFF0F172A, 0);

        let gutter_w = 54usize;
        canvas.draw_rect(self.bounds.x, editor_y, gutter_w, editor_h, 0xFF182234, 0);
        canvas.draw_line_v(self.bounds.x + gutter_w, editor_y, editor_h, 0xFF334155);

        let line_h = 30usize;
        let mut cy = editor_y + 12;

        for (i, line) in self.lines.iter().enumerate() {
            if cy + line_h > editor_y + editor_h { break; }
            canvas.draw_text(self.bounds.x + 14, cy, &format!("{:2}", i + 1), 0xFF64748B, 16);
            canvas.draw_text(self.bounds.x + gutter_w + 16, cy, line, 0xFFF8FAFC, 18);

            if i == self.cursor_row {
                let cursor_x = self.bounds.x + gutter_w + 16 + (self.cursor_col * 10);
                canvas.draw_rect(cursor_x, cy, 3, 22, 0xFF38BDF8, 0);
            }
            cy += line_h;
        }

        let sb_y = self.bounds.y + self.bounds.h - 32;
        canvas.draw_rect(self.bounds.x, sb_y, self.bounds.w, 32, 0xFF18181B, 0);
        canvas.draw_line_h(self.bounds.x, sb_y, self.bounds.w, 0xFF27272A);
        canvas.draw_text(self.bounds.x + 16, sb_y + 8, &format!("Ln {}, Col {}", self.cursor_row + 1, self.cursor_col + 1), 0xFF94A3B8, 15);
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, _p: bool) -> bool { self.bounds.contains(mx, my) }

    fn handle_char(&mut self, c: char) -> bool {
        if c == '\n' {
            self.lines.insert(self.cursor_row + 1, String::new());
            self.cursor_row += 1;
            self.cursor_col = 0;
            return true;
        } else if c == '\x08' {
            if self.cursor_col > 0 {
                self.lines[self.cursor_row].remove(self.cursor_col - 1);
                self.cursor_col -= 1;
            } else if self.cursor_row > 0 {
                let prev_len = self.lines[self.cursor_row - 1].len();
                let curr_line = self.lines.remove(self.cursor_row);
                self.cursor_row -= 1;
                self.lines[self.cursor_row].push_str(&curr_line);
                self.cursor_col = prev_len;
            }
            return true;
        } else if c >= ' ' && c <= '~' {
            self.lines[self.cursor_row].insert(self.cursor_col, c);
            self.cursor_col += 1;
            return true;
        }
        false
    }
}

pub struct SettingsApp {
    bounds: Rect,
    selected_tab: usize,
}

impl SettingsApp {
    pub fn new() -> Self {
        Self { bounds: Rect::default(), selected_tab: 0 }
    }
}

impl Widget for SettingsApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect { self.bounds = Rect { x, y, w, h }; self.bounds }

    fn paint(&self, canvas: &mut Canvas) {
        let sb_w = 220usize;
        canvas.draw_rect(self.bounds.x, self.bounds.y, sb_w, self.bounds.h, 0xFF18181B, 0);
        canvas.draw_line_v(self.bounds.x + sb_w, self.bounds.y, self.bounds.h, 0xFF27272A);

        let tabs = ["Appearance", "Display", "Hardware & SMP"];
        for (i, t) in tabs.iter().enumerate() {
            let ry = self.bounds.y + 32 + (i * 48);
            if self.selected_tab == i {
                canvas.draw_rect(self.bounds.x + 10, ry, sb_w - 20, 38, 0xFF0284C7, 8);
                canvas.draw_text(self.bounds.x + 24, ry + 9, t, 0xFFFFFFFF, 18);
            } else {
                canvas.draw_text(self.bounds.x + 24, ry + 9, t, 0xFF94A3B8, 18);
            }
        }

        let content_x = self.bounds.x + sb_w + 32;
        let content_y = self.bounds.y + 32;
        canvas.draw_rect(self.bounds.x + sb_w + 1, self.bounds.y, self.bounds.w.saturating_sub(sb_w + 1), self.bounds.h, 0xFF0F172A, 0);

        match self.selected_tab {
            0 => {
                canvas.draw_text(content_x, content_y, "Appearance & Interface", 0xFFFFFFFF, 24);
                canvas.draw_text(content_x, content_y + 48, "Theme Mode: Dark Interface [Active]", 0xFF94A3B8, 18);
                canvas.draw_rect(content_x, content_y + 85, 180, 44, 0xFF0284C7, 10);
                canvas.draw_text(content_x + 24, content_y + 97, "Dark Mode [ON]", 0xFFFFFFFF, 18);
            }
            1 => {
                canvas.draw_text(content_x, content_y, "Display Preferences", 0xFFFFFFFF, 24);
                canvas.draw_text(content_x, content_y + 48, "Resolution: 1920x1080 @ 60 FPS", 0xFF94A3B8, 18);
                canvas.draw_text(content_x, content_y + 82, "Frame Buffer: 32-bit TrueColor ARGB8888", 0xFF94A3B8, 18);
            }
            2 => {
                canvas.draw_text(content_x, content_y, "System & Hardware Overview", 0xFFFFFFFF, 24);
                canvas.draw_text(content_x, content_y + 48, "Architecture: x86_64 SMP (8 Cores)", 0xFF22C55E, 18);
                canvas.draw_text(content_x, content_y + 82, "Ring Privilege: Ring-3 Musl Userspace", 0xFF94A3B8, 18);
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if pressed && mx < self.bounds.x + 220 {
            let idx = (my.saturating_sub(self.bounds.y + 32)) / 48;
            if idx < 3 {
                self.selected_tab = idx;
                return true;
            }
        }
        true
    }
}
