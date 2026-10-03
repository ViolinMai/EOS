use crate::framework::*;

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
                            let clean_line = l.replace('\t', "    ");
                            self.history.push((clean_line.clone(), 0xFFE2E8F0));
                            println!("{}", clean_line);
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
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

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

    fn handle_mouse(&mut self, mx: usize, my: usize, _p: bool) -> bool {
        self.bounds.contains(mx, my)
    }

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
