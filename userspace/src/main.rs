mod syscall;
mod sdk;
mod framework;

use std::fs;
use std::time::Instant;
use framework::*;
use std::any::Any;

const SCREEN_W: usize = 1920;
const SCREEN_H: usize = 1080;

struct FinderWidget {
    bounds: Rect,
    current_path: String,
    sidebar_items: Vec<&'static str>,
    selected_sidebar: usize,
    files: Vec<(String, usize, bool)>,
    selected_file: Option<usize>,
    last_click_time: Instant,
    status_text: String,
}

impl FinderWidget {
    pub fn new() -> Self {
        let mut widget = Self {
            bounds: Rect { x: 0, y: 0, w: 0, h: 0 },
            current_path: ".".into(),
            sidebar_items: vec!["EOS SHARE", "fonts", "RootFS"],
            selected_sidebar: 0,
            files: Vec::new(),
            selected_file: None,
            last_click_time: Instant::now(),
            status_text: "Ready".into(),
        };
        widget.reload_directory();
        widget
    }

    fn reload_directory(&mut self) {
        self.files.clear();
        if let Ok(entries) = fs::read_dir(&self.current_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let size = entry.metadata().map(|m| m.len() as usize).unwrap_or(0);
                self.files.push((name, size, is_dir));
            }
        }
        if self.files.is_empty() {
            self.files.push(("fonts".into(), 0, true));
            self.files.push(("SFPRODISPLAYREGULAR.OTF".into(), 712000, false));
            self.files.push(("SFPRODISPLAYBOLD.OTF".into(), 724000, false));
            self.files.push(("SFPRODISPLAYMEDIUM.OTF".into(), 718000, false));
            self.files.push(("readme.txt".into(), 64, false));
        }
        self.status_text = format!("{} items in /{}", self.files.len(), self.current_path);
    }
}

impl Widget for FinderWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, origin_x: usize, origin_y: usize, max_w: usize, max_h: usize) -> Rect {
        self.bounds = Rect { x: origin_x, y: origin_y, w: max_w, h: max_h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let sidebar_w = 160usize;
        canvas.draw_rect(self.bounds.x, self.bounds.y, sidebar_w, self.bounds.h, 0xFF18181B, 0);
        canvas.draw_line_v(self.bounds.x + sidebar_w, self.bounds.y, self.bounds.h, 0xFF3F3F46);

        canvas.draw_text(self.bounds.x + 14, self.bounds.y + 12, "LOCATIONS", 0xFF94A3B8, 12);
        for (i, label) in self.sidebar_items.iter().enumerate() {
            let row_y = self.bounds.y + 36 + (i * 32);
            let is_sel = self.selected_sidebar == i;
            if is_sel {
                canvas.draw_rect(self.bounds.x + 8, row_y, sidebar_w - 16, 28, 0xFF0284C7, 6);
                canvas.draw_text(self.bounds.x + 16, row_y + 6, label, 0xFFFFFFFF, 14);
            } else {
                canvas.draw_text(self.bounds.x + 16, row_y + 6, label, 0xFFE2E8F0, 14);
            }
        }

        let content_x = self.bounds.x + sidebar_w + 1;
        let content_w = self.bounds.w.saturating_sub(sidebar_w + 1);
        canvas.draw_rect(content_x, self.bounds.y, content_w, self.bounds.h, 0xFF0F172A, 0);

        canvas.draw_rect(content_x, self.bounds.y, content_w, 30, 0xFF1E293B, 0);
        canvas.draw_line_h(content_x, self.bounds.y + 30, content_w, 0xFF334155);
        canvas.draw_text(content_x + 18, self.bounds.y + 8, "Name", 0xFF94A3B8, 13);
        canvas.draw_text(content_x + content_w / 2, self.bounds.y + 8, "Size", 0xFF94A3B8, 13);
        canvas.draw_text(content_x + content_w - 90, self.bounds.y + 8, "Kind", 0xFF94A3B8, 13);

        let row_h = 32usize;
        let mut cur_y = self.bounds.y + 32;
        for (idx, (name, size, is_dir)) in self.files.iter().enumerate() {
            if cur_y + row_h > self.bounds.y + self.bounds.h - 26 { break; }
            let is_sel = self.selected_file == Some(idx);
            if is_sel {
                canvas.draw_rect(content_x, cur_y, content_w, row_h, 0xFF0369A1, 0);
            }

            let icon_str = if *is_dir { "DIR " } else if name.ends_with(".OTF") || name.ends_with(".ttf") { "FNT " } else { "DOC " };
            let display_name = format!("{}{}", icon_str, name);
            canvas.draw_text(content_x + 18, cur_y + 7, &display_name, 0xFFFFFFFF, 14);

            let size_str = if *is_dir { "--".into() } else { format!("{} B", size) };
            canvas.draw_text(content_x + content_w / 2, cur_y + 7, &size_str, 0xFF94A3B8, 13);

            let kind_str = if *is_dir { "Folder" } else if name.ends_with(".OTF") { "OpenType Font" } else { "Document" };
            canvas.draw_text(content_x + content_w - 90, cur_y + 7, kind_str, 0xFF94A3B8, 13);

            cur_y += row_h;
        }

        let status_y = self.bounds.y + self.bounds.h - 26;
        canvas.draw_rect(content_x, status_y, content_w, 26, 0xFF18181B, 0);
        canvas.draw_line_h(content_x, status_y, content_w, 0xFF27272A);
        canvas.draw_text(content_x + 14, status_y + 6, &self.status_text, 0xFF94A3B8, 12);
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if !pressed { return true; }
        let sidebar_w = 160usize;

        if mx < self.bounds.x + sidebar_w {
            let row_idx = (my.saturating_sub(self.bounds.y + 36)) / 32;
            if row_idx < self.sidebar_items.len() {
                self.selected_sidebar = row_idx;
                self.current_path = match row_idx {
                    0 => ".".into(),
                    1 => "fonts".into(),
                    2 => "RootFS".into(),
                    _ => ".".into(),
                };
                self.reload_directory();
                return true;
            }
        }

        let content_y = self.bounds.y + 32;
        if my >= content_y {
            let row_idx = (my - content_y) / 32;
            if row_idx < self.files.len() {
                let now = Instant::now();
                if self.selected_file == Some(row_idx) && now.duration_since(self.last_click_time).as_millis() < 400 {
                    let (name, _, is_dir) = &self.files[row_idx];
                    if *is_dir {
                        self.current_path = if self.current_path == "." { name.clone() } else { format!("{}/{}", self.current_path, name) };
                        self.reload_directory();
                    } else {
                        self.status_text = format!("Selected: {}", name);
                    }
                } else {
                    self.selected_file = Some(row_idx);
                }
                self.last_click_time = now;
                return true;
            }
        }
        true
    }
}

struct TerminalWidget {
    bounds: Rect,
    lines: Vec<String>,
    input_buf: String,
}

impl TerminalWidget {
    pub fn new() -> Self {
        Self {
            bounds: Rect { x: 0, y: 0, w: 0, h: 0 },
            lines: vec![
                "EOS POSIX Musl Environment [Ring 3 Isolated]".into(),
                "Kernel: 0.1.0-SMP | x86_64 Architecture | Sugoi Vector Engine".into(),
                "Type 'help', 'clear', 'about', 'ls' to execute:".into(),
                "".into(),
            ],
            input_buf: String::new(),
        }
    }

    fn execute_command(&mut self) {
        let cmd_str = self.input_buf.trim().to_string();
        self.lines.push(format!("eos-sh> {}", cmd_str));
        self.input_buf.clear();

        let parts: Vec<&str> = cmd_str.split_whitespace().collect();
        if parts.is_empty() { return; }

        match parts[0] {
            "help" => {
                self.lines.push("  ls            : List directory contents".into());
                self.lines.push("  cat <file>    : Read and print file data".into());
                self.lines.push("  clear         : Clear screen history".into());
                self.lines.push("  about         : Display kernel & framework spec".into());
            }
            "clear" => {
                self.lines.clear();
            }
            "ls" => {
                if let Ok(entries) = fs::read_dir(".") {
                    let mut file_names = Vec::new();
                    for e in entries.flatten() {
                        file_names.push(e.file_name().to_string_lossy().to_string());
                    }
                    self.lines.push(file_names.join("  "));
                } else {
                    self.lines.push("fonts  user_app.elf  readme.txt  settings.ini".into());
                }
            }
            "cat" => {
                if parts.len() > 1 {
                    if let Ok(content) = fs::read_to_string(parts[1]) {
                        for l in content.lines() { self.lines.push(l.into()); }
                    } else {
                        self.lines.push(format!("cat: {}: No such file or directory", parts[1]));
                    }
                } else {
                    self.lines.push("Usage: cat <filename>".into());
                }
            }
            "about" => {
                self.lines.push("EOS Microkernel - High-DPI Vector GUI Framework".into());
                self.lines.push("Sugoi UI / Segoe UI Style Vector Fallback Engine Active.".into());
            }
            _ => {
                self.lines.push(format!("command not found: {}", parts[0]));
            }
        }
    }
}

impl Widget for TerminalWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, origin_x: usize, origin_y: usize, max_w: usize, max_h: usize) -> Rect {
        self.bounds = Rect { x: origin_x, y: origin_y, w: max_w, h: max_h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF111827, 0);

        let line_h = 22usize;
        let max_lines = self.bounds.h.saturating_sub(30) / line_h;
        let start_idx = if self.lines.len() > max_lines { self.lines.len() - max_lines } else { 0 };

        let mut cur_y = self.bounds.y + 10;
        for line in &self.lines[start_idx..] {
            let color = if line.starts_with("eos-sh>") {
                0xFF38BDF8
            } else if line.starts_with("command not found") {
                0xFFEF4444
            } else {
                0xFF4ADE80
            };
            canvas.draw_text(self.bounds.x + 12, cur_y, line, color, 14);
            cur_y += line_h;
        }

        let prompt = format!("eos-sh> {}_", self.input_buf);
        canvas.draw_text(self.bounds.x + 12, cur_y, &prompt, 0xFFF8FAFC, 14);
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, _pressed: bool) -> bool {
        self.bounds.contains(mx, my)
    }

    fn handle_char(&mut self, c: char) -> bool {
        if c == '\n' {
            self.execute_command();
            return true;
        } else if c == '\x08' {
            self.input_buf.pop();
            return true;
        } else if c >= ' ' && c <= '~' {
            self.input_buf.push(c);
            return true;
        }
        false
    }
}

fn main() {
    println!("🚀 Launching EOS Full Desktop Experience (Sugoi UI Fallback + SF Pro)...");

    let font_bytes = fs::read("SFPRODISPLAYREGULAR.OTF")
        .or_else(|_| fs::read("fonts/SFPRODISPLAYREGULAR.OTF"))
        .or_else(|_| fs::read("fonts/SFPRODISPLAYMEDIUM.OTF"))
        .or_else(|_| fs::read("fonts/SFPRODISPLAYBOLD.OTF"))
        .unwrap_or_default();

    if !font_bytes.is_empty() {
        println!("  [✓] SF Pro Display Vector Font loaded successfully ({} bytes)", font_bytes.len());
    } else {
        println!("  [✓] Using Sugoi UI / Segoe UI Style Vector Typography Engine");
    }

    let mut app = FrameworkApp::new(&font_bytes, SCREEN_W, SCREEN_H);

    let win_finder = WindowFrame::new(
        "Finder",
        120,
        70,
        760,
        480,
        Box::new(FinderWidget::new()),
    );

    let win_terminal = WindowFrame::new(
        "Terminal",
        720,
        140,
        640,
        420,
        Box::new(TerminalWidget::new()),
    );

    app.add_widget(Box::new(win_finder));
    app.add_widget(Box::new(win_terminal));

    app.set_dock_handler(|idx| {
        println!("Dock Icon Clicked: {}", idx);
    });

    app.run_loop();
}
