use std::fs;
use crate::framework::*;

#[derive(Clone)]
pub struct FileEntry {
    pub name: String,
    pub size: usize,
    pub is_dir: bool,
    pub kind_str: String,
}

pub struct FinderApp {
    bounds: Rect,
    sidebar_items: Vec<&'static str>,
    selected_sidebar: usize,
    columns: Vec<(Vec<FileEntry>, Option<usize>)>,
    active_path: Vec<String>,
    last_click_tick: u64,
    last_clicked_file: Option<String>,
    pub pending_open_image: Option<(String, Vec<u8>)>,
}

impl FinderApp {
    pub fn new() -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            sidebar_items: vec!["EOS SHARE", "RootFS", "Initrd"],
            selected_sidebar: 0,
            columns: Vec::new(),
            active_path: vec!["/EOS SHARE".into()],
            last_click_tick: 0,
            last_clicked_file: None,
            pending_open_image: None,
        };
        app.load_root();
        app
    }

    fn detect_kind(name: &str, is_dir: bool) -> String {
        if is_dir { return "Folder".into(); }
        let l = name.to_ascii_lowercase();
        if l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg") {
            "Image".into()
        } else if l.ends_with(".elf") {
            "Executable".into()
        } else if l.ends_with(".otf") || l.ends_with(".ttf") {
            "Font".into()
        } else if l.ends_with(".txt") || l.ends_with(".rs") || l.ends_with(".toml") {
            "Document".into()
        } else {
            "File".into()
        }
    }

    fn read_entries(path: &str) -> Vec<FileEntry> {
        let mut list = Vec::new();
        let clean_path = if path == "/" { "." } else { path.trim_start_matches('/') };

        if let Ok(entries) = fs::read_dir(path).or_else(|_| fs::read_dir(clean_path)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "." || name == ".." { continue; }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let full_subpath = format!("{}/{}", path.trim_end_matches('/'), name);
                let size = fs::metadata(&full_subpath).or_else(|_| entry.metadata()).map(|m| m.len() as usize).unwrap_or(0);
                let kind_str = Self::detect_kind(&name, is_dir);
                list.push(FileEntry { name, size, is_dir, kind_str });
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
        let theme = get_theme();
        let tb_h = theme.pt(40.0);
        let sb_w = theme.pt(160.0);
        let col_w = theme.pt(220.0);
        let row_h = theme.pt(30.0);

        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);
        let current_full_path = self.active_path.join("/");
        canvas.draw_text(self.bounds.x + theme.pt(18.0), self.bounds.y + theme.pt(10.0), &format!("📁 {}", current_full_path), theme.text_primary, theme.font_body());

        let content_y = self.bounds.y + tb_h + 1;
        let content_h = self.bounds.h.saturating_sub(tb_h + 1);
        canvas.draw_rect(self.bounds.x, content_y, sb_w, content_h, theme.bg_window, 0);
        canvas.draw_line_v(self.bounds.x + sb_w, content_y, content_h, theme.border_window);

        canvas.draw_text(self.bounds.x + theme.pt(16.0), content_y + theme.pt(14.0), "FAVORITES", theme.text_muted, theme.font_caption());
        for (i, item) in self.sidebar_items.iter().enumerate() {
            let ry = content_y + theme.pt(38.0) + (i * theme.pt(36.0));
            if self.selected_sidebar == i {
                canvas.draw_rect(self.bounds.x + theme.pt(10.0), ry, sb_w - theme.pt(20.0), theme.pt(30.0), theme.accent, theme.pt(6.0));
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ry + theme.pt(6.0), item, 0xFFFFFFFF, theme.font_body());
            } else {
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ry + theme.pt(6.0), item, theme.text_secondary, theme.font_body());
            }
        }

        let mut cur_x = self.bounds.x + sb_w + 1;
        for (col_idx, (files, selected)) in self.columns.iter().enumerate() {
            if cur_x + col_w > self.bounds.x + self.bounds.w { break; }
            canvas.draw_rect(cur_x, content_y, col_w, content_h, if col_idx % 2 == 0 { 0xFF141E2E } else { theme.bg_window }, 0);
            canvas.draw_line_v(cur_x + col_w, content_y, content_h, theme.border_window);

            let mut ry = content_y + theme.pt(8.0);
            for (idx, f) in files.iter().enumerate() {
                if ry + row_h > content_y + content_h { break; }
                let is_sel = *selected == Some(idx);
                if is_sel {
                    canvas.draw_rect(cur_x + theme.pt(6.0), ry, col_w - theme.pt(12.0), row_h - 4, theme.accent, theme.pt(6.0));
                }
                let icon = if f.is_dir { "📁" } else if f.kind_str == "Image" { "🖼" } else if f.kind_str == "Executable" { "⚙" } else { "📄" };
                let txt_color = if is_sel { 0xFFFFFFFF } else { theme.text_primary };
                canvas.draw_text(cur_x + theme.pt(10.0), ry + theme.pt(6.0), &format!("{} {}", icon, f.name), txt_color, theme.font_body());
                if f.is_dir {
                    canvas.draw_text(cur_x + col_w - theme.pt(20.0), ry + theme.pt(6.0), "›", theme.text_muted, theme.font_title());
                }
                ry += row_h;
            }
            cur_x += col_w + 1;
        }

        if cur_x < self.bounds.x + self.bounds.w {
            let insp_w = (self.bounds.x + self.bounds.w).saturating_sub(cur_x);
            canvas.draw_rect(cur_x, content_y, insp_w, content_h, theme.bg_window, 0);
            if let Some((files, Some(sel))) = self.columns.last() {
                if let Some(target) = files.get(*sel) {
                    let icon_sz = theme.pt(72.0);
                    canvas.draw_rect(cur_x + theme.pt(24.0), content_y + theme.pt(24.0), icon_sz, icon_sz, theme.bg_titlebar, theme.pt(14.0));
                    canvas.draw_rect_outline(cur_x + theme.pt(24.0), content_y + theme.pt(24.0), icon_sz, icon_sz, theme.border_window, theme.pt(14.0));
                    let tag = if target.is_dir { "DIR" } else if target.kind_str == "Image" { "IMG" } else { "DOC" };
                    canvas.draw_text(cur_x + theme.pt(40.0), content_y + theme.pt(48.0), tag, theme.accent_hover, theme.font_title());
                    canvas.draw_text(cur_x + theme.pt(24.0), content_y + theme.pt(110.0), &target.name, theme.text_primary, theme.font_title());
                    canvas.draw_text(cur_x + theme.pt(24.0), content_y + theme.pt(140.0), &format!("Size: {} Bytes", target.size), theme.text_secondary, theme.font_body());
                    canvas.draw_text(cur_x + theme.pt(24.0), content_y + theme.pt(168.0), &format!("Kind: {}", target.kind_str), theme.accent_hover, theme.font_body());
                }
            } else {
                canvas.draw_text(cur_x + theme.pt(24.0), content_y + theme.pt(48.0), "No item selected", theme.text_muted, theme.font_body());
            }
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if !pressed { return true; }

        let theme = get_theme();
        let tb_h = theme.pt(40.0);
        let sb_w = theme.pt(160.0);
        let content_y = self.bounds.y + tb_h + 1;

        if mx < self.bounds.x + sb_w && my >= content_y {
            let idx = (my.saturating_sub(content_y + theme.pt(38.0))) / theme.pt(36.0);
            if idx < self.sidebar_items.len() {
                self.selected_sidebar = idx;
                self.load_root();
                return true;
            }
        }

        let col_w = theme.pt(220.0) + 1;
        let row_h = theme.pt(30.0);
        let mut cur_x = self.bounds.x + sb_w + 1;

        for col_idx in 0..self.columns.len() {
            if mx >= cur_x && mx < cur_x + col_w {
                let row_idx = (my.saturating_sub(content_y + theme.pt(8.0))) / row_h;
                if row_idx < self.columns[col_idx].0.len() {
                    self.columns[col_idx].1 = Some(row_idx);
                    let item = self.columns[col_idx].0[row_idx].clone();

                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                    let is_double_click = self.last_clicked_file == Some(item.name.clone()) && now.saturating_sub(self.last_click_tick) < 450;
                    self.last_click_tick = now;
                    self.last_clicked_file = Some(item.name.clone());

                    self.columns.truncate(col_idx + 1);
                    self.active_path.truncate(col_idx + 1);

                    if item.is_dir {
                        self.active_path.push(item.name.clone());
                        let next_path = self.active_path.join("/");
                        let sub_entries = Self::read_entries(&next_path);
                        self.columns.push((sub_entries, None));
                    } else if is_double_click && item.kind_str == "Image" {
                        let full_image_path = format!("{}/{}", self.active_path.join("/"), item.name);
                        if let Ok(bytes) = fs::read(&full_image_path).or_else(|_| fs::read(full_image_path.trim_start_matches('/'))) {
                            self.pending_open_image = Some((item.name.clone(), bytes));
                        }
                    }
                    return true;
                }
            }
            cur_x += col_w;
        }
        true
    }
}

pub struct PreviewApp {
    bounds: Rect,
    pub title: String,
    pub image_data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub raw_pixels: Vec<u32>,
}

impl PreviewApp {
    pub fn new(title: String, data: Vec<u8>) -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            title,
            image_data: data,
            width: 0,
            height: 0,
            raw_pixels: Vec::new(),
        };
        app.decode();
        app
    }

    fn decode(&mut self) {
        if self.image_data.is_empty() { return; }
        if self.image_data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            let mut decoder = zune_jpeg::JpegDecoder::new(&self.image_data);
            if let Ok(px) = decoder.decode() {
                if let Some(info) = decoder.info() {
                    self.width = info.width as usize;
                    self.height = info.height as usize;
                    self.raw_pixels = vec![0u32; self.width * self.height];
                    for i in 0..self.width * self.height {
                        let o = i * 3;
                        if o + 2 < px.len() {
                            self.raw_pixels[i] = (0xFF << 24) | ((px[o] as u32) << 16) | ((px[o+1] as u32) << 8) | (px[o+2] as u32);
                        }
                    }
                }
            }
        } else if self.image_data.starts_with(b"\x89PNG\r\n\x1a\n") {
            if let Ok((px, w, h)) = decode_userspace_png(&self.image_data) {
                self.width = w;
                self.height = h;
                self.raw_pixels = px;
            }
        }
    }
}

fn decode_userspace_png(data: &[u8]) -> Result<(Vec<u32>, usize, usize), &'static str> {
    if data.len() < 33 { return Err("PNG too short"); }
    let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as usize;
    let _bpp = match data[25] { 2 => 3, 6 => 4, _ => return Err("Unsupported color type") };
    let mut out = vec![0xFF38BDF8; w * h];
    for y in 0..h {
        for x in 0..w {
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                out[y * w + x] = 0xFFFFFFFF;
            }
        }
    }
    Ok((out, w, h))
}

impl Widget for PreviewApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect { self.bounds = Rect { x, y, w, h }; self.bounds }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF050811, 0);

        if self.width == 0 || self.height == 0 || self.raw_pixels.is_empty() {
            let msg = format!("Image Loaded: {} ({} bytes)", self.title, self.image_data.len());
            canvas.draw_text(self.bounds.x + theme.pt(20.0), self.bounds.y + theme.pt(40.0), &msg, theme.text_primary, theme.font_title());
            canvas.draw_text(self.bounds.x + theme.pt(20.0), self.bounds.y + theme.pt(80.0), "Ready for render inspection", theme.text_secondary, theme.font_body());
            return;
        }

        let step_x = (self.width + self.bounds.w - 1) / self.bounds.w;
        let step_y = (self.height + self.bounds.h - 1) / self.bounds.h;
        let step = step_x.max(step_y).max(1);

        let disp_w = (self.width / step).min(self.bounds.w);
        let disp_h = (self.height / step).min(self.bounds.h);
        let off_x = self.bounds.x + (self.bounds.w.saturating_sub(disp_w) / 2);
        let off_y = self.bounds.y + (self.bounds.h.saturating_sub(disp_h) / 2);

        for dy in 0..disp_h {
            let src_y = dy * step;
            let dst_y = off_y + dy;
            if dst_y >= canvas.height { break; }
            for dx in 0..disp_w {
                let src_x = dx * step;
                let dst_x = off_x + dx;
                if dst_x >= canvas.width { break; }
                let col = self.raw_pixels[src_y * self.width + src_x];
                canvas.buffer[dst_y * canvas.width + dst_x] = col;
            }
        }
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
        let echo = format!("{}> {}", self.cwd, cmd);
        self.history.push((echo.clone(), 0xFF38BDF8));
        println!("[TERM] {}", echo);
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
            "pwd" => {
                self.history.push((self.cwd.clone(), 0xFF22C55E));
                println!("{}", self.cwd);
            },
            "cd" => {
                if parts.len() > 1 {
                    self.cwd = parts[1].to_string();
                } else {
                    self.cwd = "/EOS SHARE".into();
                }
            }
            "ls" => {
                let target_dir = if parts.len() > 1 { parts[1] } else { &self.cwd };
                if let Ok(entries) = std::fs::read_dir(target_dir).or_else(|_| std::fs::read_dir(target_dir.trim_start_matches('/'))) {
                    let mut line = String::new();
                    for e in entries.flatten() {
                        let is_d = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        let name = e.file_name().to_string_lossy().to_string();
                        let label = format!("{} {}   ", if is_d { "📁" } else { "📄" }, name);
                        line.push_str(&label);
                        println!("{}", label.trim());
                    }
                    if !line.is_empty() {
                        self.history.push((line, 0xFFFFFFFF));
                    } else {
                        self.history.push(("(empty directory)".into(), 0xFF94A3B8));
                    }
                } else {
                    let err = format!("ls: cannot access '{}': No such file or directory", target_dir);
                    self.history.push((err.clone(), 0xFFEF4444));
                    println!("{}", err);
                }
            }
            "cat" => {
                if parts.len() < 2 {
                    self.history.push(("Usage: cat <filename>".into(), 0xFFEF4444));
                } else {
                    let p = parts[1];
                    let full_path = if p.starts_with('/') { p.to_string() } else { format!("{}/{}", self.cwd, p) };
                    if let Ok(content) = std::fs::read_to_string(&full_path).or_else(|_| std::fs::read_to_string(p)) {
                        for l in content.lines() {
                            self.history.push((l.to_string(), 0xFFE2E8F0));
                            println!("{}", l);
                        }
                    } else {
                        let err = format!("cat: {}: No such file", p);
                        self.history.push((err.clone(), 0xFFEF4444));
                        println!("{}", err);
                    }
                }
            }
            _ => {
                let err = format!("command not found: {}", parts[0]);
                self.history.push((err.clone(), 0xFFEF4444));
                println!("{}", err);
            },
        }
    }
}

impl Widget for TerminalApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect { self.bounds = Rect { x, y, w, h }; self.bounds }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF090D16, 0);

        let line_h = theme.pt(24.0);
        let max_lines = self.bounds.h.saturating_sub(theme.pt(36.0)) / line_h;
        let start = if self.history.len() > max_lines { self.history.len() - max_lines } else { 0 };

        let mut cy = self.bounds.y + theme.pt(12.0);
        for (line, color) in &self.history[start..] {
            canvas.draw_text(self.bounds.x + theme.pt(18.0), cy, line, *color, theme.font_body());
            cy += line_h;
        }
        let prompt = format!("{}> {}_", self.cwd, self.input);
        canvas.draw_text(self.bounds.x + theme.pt(18.0), cy, &prompt, 0xFFFFFFFF, theme.font_body());
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
        let theme = get_theme();
        let tb_h = theme.pt(36.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);
        canvas.draw_text(self.bounds.x + theme.pt(18.0), self.bounds.y + theme.pt(8.0), "💾 TextEdit", theme.text_primary, theme.font_body());

        let editor_y = self.bounds.y + tb_h + 1;
        let editor_h = self.bounds.h.saturating_sub(tb_h + theme.pt(30.0));
        canvas.draw_rect(self.bounds.x, editor_y, self.bounds.w, editor_h, theme.bg_window, 0);

        let gutter_w = theme.pt(48.0);
        canvas.draw_rect(self.bounds.x, editor_y, gutter_w, editor_h, 0xFF141E2E, 0);
        canvas.draw_line_v(self.bounds.x + gutter_w, editor_y, editor_h, theme.border_window);

        let line_h = theme.pt(26.0);
        let mut cy = editor_y + theme.pt(8.0);

        for (i, line) in self.lines.iter().enumerate() {
            if cy + line_h > editor_y + editor_h { break; }
            canvas.draw_text(self.bounds.x + theme.pt(12.0), cy, &format!("{:2}", i + 1), theme.text_muted, theme.font_caption());
            canvas.draw_text(self.bounds.x + gutter_w + theme.pt(16.0), cy, line, theme.text_primary, theme.font_body());

            if i == self.cursor_row {
                let cursor_x = self.bounds.x + gutter_w + theme.pt(16.0) + (self.cursor_col * theme.pt(9.0));
                canvas.draw_rect(cursor_x, cy, 3, theme.pt(18.0), theme.accent_hover, 0);
            }
            cy += line_h;
        }

        let sb_y = self.bounds.y + self.bounds.h - theme.pt(28.0);
        canvas.draw_rect(self.bounds.x, sb_y, self.bounds.w, theme.pt(28.0), theme.bg_menubar, 0);
        canvas.draw_line_h(self.bounds.x, sb_y, self.bounds.w, theme.border_menubar);
        canvas.draw_text(self.bounds.x + theme.pt(16.0), sb_y + theme.pt(6.0), &format!("Ln {}, Col {}", self.cursor_row + 1, self.cursor_col + 1), theme.text_secondary, theme.font_caption());
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

pub struct ActivityMonitorApp {
    bounds: Rect,
    selected_tab: usize,
    sample_tick: usize,
}

impl ActivityMonitorApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            selected_tab: 0,
            sample_tick: 0,
        }
    }
}

impl Widget for ActivityMonitorApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect { self.bounds = Rect { x, y, w, h }; self.bounds }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_window, 0);

        let tb_h = theme.pt(44.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);

        let tabs = ["CPU", "Memory", "Disk", "Network"];
        let seg_w = theme.pt(80.0);
        let start_x = self.bounds.x + (self.bounds.w.saturating_sub(tabs.len() * seg_w) / 2);

        for (i, &t) in tabs.iter().enumerate() {
            let bx = start_x + (i * seg_w);
            let by = self.bounds.y + theme.pt(8.0);
            if self.selected_tab == i {
                canvas.draw_rect(bx, by, seg_w, theme.pt(28.0), theme.accent, theme.pt(6.0));
                let (tw, _) = canvas.measure_text(t, theme.font_body());
                canvas.draw_text(bx + (seg_w.saturating_sub(tw) / 2), by + theme.pt(5.0), t, 0xFFFFFFFF, theme.font_body());
            } else {
                canvas.draw_rect_outline(bx, by, seg_w, theme.pt(28.0), theme.border_window, theme.pt(6.0));
                let (tw, _) = canvas.measure_text(t, theme.font_body());
                canvas.draw_text(bx + (seg_w.saturating_sub(tw) / 2), by + theme.pt(5.0), t, theme.text_secondary, theme.font_body());
            }
        }

        let content_y = self.bounds.y + tb_h + theme.pt(16.0);
        let pad_x = self.bounds.x + theme.pt(24.0);

        match self.selected_tab {
            0 => {
                let card_h = theme.pt(120.0);
                let card_w = self.bounds.w.saturating_sub(theme.pt(48.0));
                canvas.draw_rect(pad_x, content_y, card_w, card_h, theme.bg_titlebar, theme.pt(10.0));
                canvas.draw_rect_outline(pad_x, content_y, card_w, card_h, theme.border_window, theme.pt(10.0));

                canvas.draw_text(pad_x + theme.pt(20.0), content_y + theme.pt(16.0), "Processor: 8 Symmetrical Cores (x86_64 SMP)", theme.text_primary, theme.font_title());
                canvas.draw_text(pad_x + theme.pt(20.0), content_y + theme.pt(48.0), "System Load: 14% | User Space: 8% | Idle: 78%", theme.accent_hover, theme.font_body());

                let bar_w = card_w.saturating_sub(theme.pt(40.0)) / 8;
                for c in 0..8 {
                    let bx = pad_x + theme.pt(20.0) + (c * bar_w);
                    let by = content_y + theme.pt(80.0);
                    canvas.draw_rect(bx, by, bar_w.saturating_sub(theme.pt(6.0)), theme.pt(24.0), 0xFF141E2E, theme.pt(4.0));
                    let load_fill = ((c * 11 + 25) % 85) * (bar_w.saturating_sub(theme.pt(6.0))) / 100;
                    canvas.draw_rect(bx, by, load_fill, theme.pt(24.0), theme.accent, theme.pt(4.0));
                }

                let table_y = content_y + card_h + theme.pt(18.0);
                canvas.draw_rect(pad_x, table_y, card_w, theme.pt(32.0), theme.bg_titlebar, 0);
                canvas.draw_line_h(pad_x, table_y + theme.pt(32.0), card_w, theme.border_window);

                canvas.draw_text(pad_x + theme.pt(16.0), table_y + theme.pt(6.0), "Process Name", theme.text_muted, theme.font_caption());
                canvas.draw_text(pad_x + theme.pt(220.0), table_y + theme.pt(6.0), "PID", theme.text_muted, theme.font_caption());
                canvas.draw_text(pad_x + theme.pt(320.0), table_y + theme.pt(6.0), "% CPU", theme.text_muted, theme.font_caption());
                canvas.draw_text(pad_x + theme.pt(440.0), table_y + theme.pt(6.0), "Memory", theme.text_muted, theme.font_caption());

                let rows = [
                    ("kernel_compositor", "0", "4.2%", "24 MB"),
                    ("user_app (GUI)", "2", "8.1%", "64 MB"),
                    ("ata_driver_daemon", "1", "0.2%", "8 MB"),
                    ("e1000_net_worker", "3", "0.0%", "6 MB"),
                ];

                for (idx, (pname, pid, cpu, mem)) in rows.iter().enumerate() {
                    let ry = table_y + theme.pt(36.0) + (idx * theme.pt(30.0));
                    if ry + theme.pt(30.0) > self.bounds.y + self.bounds.h { break; }
                    canvas.draw_text(pad_x + theme.pt(16.0), ry + theme.pt(5.0), pname, theme.text_primary, theme.font_body());
                    canvas.draw_text(pad_x + theme.pt(220.0), ry + theme.pt(5.0), pid, theme.text_secondary, theme.font_body());
                    canvas.draw_text(pad_x + theme.pt(320.0), ry + theme.pt(5.0), cpu, theme.accent_hover, theme.font_body());
                    canvas.draw_text(pad_x + theme.pt(440.0), ry + theme.pt(5.0), mem, theme.text_secondary, theme.font_body());
                }
            }
            1 => {
                canvas.draw_text(pad_x, content_y, "Physical Memory Configuration", theme.text_primary, theme.font_large());
                canvas.draw_text(pad_x, content_y + theme.pt(36.0), "Total RAM: 8179 MB Configured", theme.text_secondary, theme.font_body());
                canvas.draw_text(pad_x, content_y + theme.pt(66.0), "Kernel Heap: 256 MB Dedicated", theme.accent_hover, theme.font_body());
            }
            _ => {
                canvas.draw_text(pad_x, content_y, "Subsystem Monitor Active", theme.text_primary, theme.font_large());
            }
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let tb_h = theme.pt(44.0);

        if pressed && my <= self.bounds.y + tb_h {
            let seg_w = theme.pt(80.0);
            let start_x = self.bounds.x + (self.bounds.w.saturating_sub(4 * seg_w) / 2);
            if mx >= start_x && mx <= start_x + (4 * seg_w) {
                self.selected_tab = (mx - start_x) / seg_w;
                return true;
            }
        }
        true
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
        let theme = get_theme();
        let sb_w = theme.pt(160.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, sb_w, self.bounds.h, theme.bg_titlebar, 0);
        canvas.draw_line_v(self.bounds.x + sb_w, self.bounds.y, self.bounds.h, theme.border_window);

        let tabs = ["Appearance", "Display", "Hardware & SMP"];
        for (i, t) in tabs.iter().enumerate() {
            let ry = self.bounds.y + theme.pt(24.0) + (i * theme.pt(38.0));
            if self.selected_tab == i {
                canvas.draw_rect(self.bounds.x + theme.pt(10.0), ry, sb_w - theme.pt(20.0), theme.pt(32.0), theme.accent, theme.pt(8.0));
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ry + theme.pt(6.0), t, 0xFFFFFFFF, theme.font_body());
            } else {
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ry + theme.pt(6.0), t, theme.text_secondary, theme.font_body());
            }
        }

        let content_x = self.bounds.x + sb_w + theme.pt(24.0);
        let content_y = self.bounds.y + theme.pt(24.0);
        canvas.draw_rect(self.bounds.x + sb_w + 1, self.bounds.y, self.bounds.w.saturating_sub(sb_w + 1), self.bounds.h, theme.bg_window, 0);

        match self.selected_tab {
            0 => {
                canvas.draw_text(content_x, content_y, "Appearance & Interface", theme.text_primary, theme.font_title());
                canvas.draw_text(content_x, content_y + theme.pt(36.0), "UI Scale Factor: 1.5x Dynamic HiDPI [Active]", theme.text_secondary, theme.font_body());
                canvas.draw_rect(content_x, content_y + theme.pt(68.0), theme.pt(150.0), theme.pt(36.0), theme.accent, theme.pt(8.0));
                canvas.draw_text(content_x + theme.pt(20.0), content_y + theme.pt(76.0), "Dark Mode [ON]", 0xFFFFFFFF, theme.font_body());
            }
            1 => {
                canvas.draw_text(content_x, content_y, "Display Preferences", theme.text_primary, theme.font_title());
                canvas.draw_text(content_x, content_y + theme.pt(36.0), "Resolution: 1920x1080 @ 60 FPS", theme.text_secondary, theme.font_body());
                canvas.draw_text(content_x, content_y + theme.pt(66.0), "Frame Buffer: 32-bit TrueColor ARGB8888", theme.accent_hover, theme.font_body());
            }
            2 => {
                canvas.draw_text(content_x, content_y, "System & Hardware Overview", theme.text_primary, theme.font_title());
                canvas.draw_text(content_x, content_y + theme.pt(36.0), "Architecture: x86_64 SMP (8 Cores)", 0xFF22C55E, theme.font_body());
                canvas.draw_text(content_x, content_y + theme.pt(66.0), "Ring Privilege: Ring-3 Musl Userspace", theme.text_secondary, theme.font_body());
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let sb_w = theme.pt(160.0);
        if pressed && mx < self.bounds.x + sb_w {
            let idx = (my.saturating_sub(self.bounds.y + theme.pt(24.0))) / theme.pt(38.0);
            if idx < 3 {
                self.selected_tab = idx;
                return true;
            }
        }
        true
    }
}
