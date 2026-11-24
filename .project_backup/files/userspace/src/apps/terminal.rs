use std::fs;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use crate::framework::*;

pub static CLIPBOARD: Mutex<String> = Mutex::new(String::new());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextPos {
    line: usize,
    col: usize,
}

#[derive(Clone, Debug)]
struct StyledLine {
    spans: Vec<(String, u32)>,
    grid_cols: usize,
}

impl StyledLine {
    fn plain(text: impl Into<String>, color: u32) -> Self {
        Self { spans: vec![(text.into(), color)], grid_cols: 0 }
    }

    fn spans(spans: Vec<(String, u32)>) -> Self {
        Self { spans, grid_cols: 0 }
    }

    fn grid(spans: Vec<(String, u32)>, grid_cols: usize) -> Self {
        Self { spans, grid_cols }
    }

    fn full_text(&self) -> String {
        let mut out = String::new();
        for (i, (s, _)) in self.spans.iter().enumerate() {
            if self.grid_cols > 0 && i > 0 {
                out.push_str("  ");
            }
            out.push_str(s);
        }
        out
    }
}

pub struct TerminalApp {
    bounds: Rect,
    history: Vec<StyledLine>,
    input: String,
    cursor_idx: usize,
    cmd_history: Vec<String>,
    history_idx: Option<usize>,
    scroll_y: i32,
    current_dir: String,
    // Mouse text selection (enabled by default in Terminal)
    selectable: bool,
    is_selecting: bool,
    sel_start: Option<TextPos>,
    sel_end: Option<TextPos>,
    // Kitty beam cursor blink state
    cursor_visible: bool,
    last_blink: Instant,
}

impl TerminalApp {
    pub fn new() -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            history: vec![
                StyledLine::spans(vec![
                    ("EOS 0.1.0".into(), 0xFF38BDF8),
                    ("  •  ".into(), 0xFF475569),
                    ("root@eos".into(), 0xFF34D399),
                    ("  •  Ctrl+Shift+C: Copy  |  Ctrl+C: Cancel".into(), 0xFF64748B),
                ]),
                StyledLine::plain("", 0xFFFFFFFF),
            ],
            input: String::new(),
            cursor_idx: 0,
            cmd_history: Vec::new(),
            history_idx: None,
            scroll_y: 0,
            current_dir: "/".into(),
            selectable: true,
            is_selecting: false,
            sel_start: None,
            sel_end: None,
            cursor_visible: true,
            last_blink: Instant::now(),
        };
        app.load_history_from_disk();
        app
    }

    pub fn tick_cursor(&mut self) -> bool {
        if self.last_blink.elapsed() >= Duration::from_millis(530) {
            self.cursor_visible = !self.cursor_visible;
            self.last_blink = Instant::now();
            return true;
        }
        false
    }

    fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.last_blink = Instant::now();
    }

    fn font_px(&self) -> usize {
        get_theme().font_large()
    }

    fn line_h(&self) -> i32 {
        (self.font_px() as i32) + get_theme().pt(8.0)
    }

    fn prompt_prefix(&self) -> String {
        let disp_dir = if self.current_dir == "/" {
            "~".to_string()
        } else {
            self.current_dir.clone()
        };
        format!("root@eos {}> ", disp_dir)
    }

    fn load_history_from_disk(&mut self) {
        for p in ["/home/march/.bash_history", "home/march/.bash_history", ".bash_history"] {
            if let Ok(content) = fs::read_to_string(p) {
                for line in content.lines() {
                    let tr = line.trim();
                    if !tr.is_empty() && !self.cmd_history.contains(&tr.to_string()) {
                        self.cmd_history.push(tr.to_string());
                    }
                }
                break;
            }
        }
    }

    fn save_history_to_disk(&self) {
        let payload = self.cmd_history.join("\n") + "\n";
        for p in ["/home/march/.bash_history", "home/march/.bash_history", ".bash_history"] {
            let _ = fs::write(p, payload.as_bytes());
        }
    }

    fn scroll_to_bottom(&mut self) {
        let lh = self.line_h();
        let total_h = ((self.history.len() + 1) as i32) * lh;
        let visible_h = (self.bounds.h - get_theme().pt(36.0)).max(lh);
        self.scroll_y = (total_h - visible_h).max(0);
    }

    /// Fish/Kitty-style live inline ghost autosuggestion
    fn ghost_suggestion(&self) -> Option<String> {
        if self.input.is_empty() || self.cursor_idx != self.input.len() {
            return None;
        }
        // 1. Check command history (most recent first)
        for cmd in self.cmd_history.iter().rev() {
            if cmd.len() > self.input.len() && cmd.starts_with(&self.input) {
                return Some(cmd[self.input.len()..].to_string());
            }
        }
        // 2. Check builtin commands
        if !self.input.contains(' ') {
            let builtins = [
                "help", "whoami", "pwd", "history", "ls", "cd", "cat", "rm", "clear",
                "uname", "echo", "ps", "kill", "free", "lspci", "reboot", "shutdown", "python",
            ];
            for b in builtins {
                if b.len() > self.input.len() && b.starts_with(&self.input) {
                    return Some(b[self.input.len()..].to_string());
                }
            }
        }
        None
    }

    fn execute(&mut self) {
        let raw_cmd = self.input.trim().to_string();
        self.history.push(StyledLine::spans(vec![
            (self.prompt_prefix(), 0xFF38BDF8),
            (self.input.clone(), 0xFFF8FAFC),
        ]));

        if !raw_cmd.is_empty() {
            self.cmd_history.retain(|x| x != &raw_cmd);
            self.cmd_history.push(raw_cmd.clone());
            self.save_history_to_disk();
        }
        self.history_idx = None;
        self.input.clear();
        self.cursor_idx = 0;
        self.sel_start = None;
        self.sel_end = None;

        if raw_cmd.is_empty() {
            self.scroll_to_bottom();
            return;
        }

        let parts = self.parse_command_args(&raw_cmd);
        if parts.is_empty() {
            self.scroll_to_bottom();
            return;
        }

        let mut redir_file: Option<(String, bool)> = None;
        let mut cmd_parts: Vec<String> = Vec::new();
        let mut pi = 0;
        while pi < parts.len() {
            if parts[pi] == ">" || parts[pi] == ">>" {
                let append = parts[pi] == ">>";
                if let Some(target) = parts.get(pi + 1) {
                    let full = if target.starts_with('/') || self.current_dir == "/" {
                        target.clone()
                    } else {
                        format!("{}/{}", self.current_dir.trim_matches('/'), target)
                    };
                    redir_file = Some((full, append));
                }
                break;
            }
            cmd_parts.push(parts[pi].clone());
            pi += 1;
        }
        if cmd_parts.is_empty() {
            self.scroll_to_bottom();
            return;
        }

        let program = cmd_parts[0].as_str();
        let args = &cmd_parts[1..];
        let hist_before = self.history.len();

        match program {
            "help" => {
                self.history.push(StyledLine::plain("ls [path]  cd <dir>  cat <file>  rm <file>  echo <txt>  clear", 0xFFE2E8F0));
                self.history.push(StyledLine::plain("ps  kill <pid>  free  lspci  uname  whoami  pwd  history", 0xFF34D399));
                self.history.push(StyledLine::plain("reboot  shutdown  |  Tab / Right: complete  |  Ctrl+Shift+C/V: Copy/Paste", 0xFF94A3B8));
            }
            "echo" => self.history.push(StyledLine::plain(args.join(" "), 0xFFE2E8F0)),
            "whoami" => self.history.push(StyledLine::plain("root", 0xFF34D399)),
            "pwd" => self.history.push(StyledLine::plain(self.current_dir.clone(), 0xFFE2E8F0)),
            "clear" => {
                self.history.clear();
                self.scroll_y = 0;
            }
            "uname" => self.history.push(StyledLine::plain("EOS 0.1.0 x86_64 SMP (musl)", 0xFFE2E8F0)),
            "history" => {
                for (i, h) in self.cmd_history.iter().enumerate() {
                    self.history.push(StyledLine::plain(format!("{:3}  {}", i + 1, h), 0xFFE2E8F0));
                }
            }
            "ps" => {
                let procs = crate::sdk::sys::query_processes();
                self.history.push(StyledLine::plain("PID   CORE   MEM(KB)   SYSCALLS   NAME", 0xFFFCD34D));
                for p in procs {
                    self.history.push(StyledLine::plain(
                        format!("{:<5} {:<6} {:<9} {:<10} {}", p.pid, p.core_id, p.mem_kb, p.syscalls, p.name),
                        0xFFE2E8F0,
                    ));
                }
            }
            "kill" => {
                if let Some(pid_str) = args.first() {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        if crate::syscall::sys_kill_proc(pid) {
                            self.history.push(StyledLine::plain(format!("Terminated PID {}", pid), 0xFF10B981));
                        } else {
                            self.history.push(StyledLine::plain(format!("kill: failed or protected PID {}", pid), 0xFFEF4444));
                        }
                    } else {
                        self.history.push(StyledLine::plain("kill: invalid PID", 0xFFEF4444));
                    }
                } else {
                    self.history.push(StyledLine::plain("usage: kill <pid>", 0xFFEF4444));
                }
            }
            "free" => {
                let used_kb: usize = crate::sdk::agent::query_kernel_heap_used() / 1024;
                let total_kb: usize = 256 * 1024;
                let free_kb: usize = total_kb.saturating_sub(used_kb);
                self.history.push(StyledLine::plain("              total        used        free", 0xFFFCD34D));
                self.history.push(StyledLine::plain(
                    format!("KernelHeap: {:>7} KB  {:>7} KB  {:>7} KB", total_kb, used_kb, free_kb),
                    0xFFE2E8F0,
                ));
            }
            "lspci" => {
                let gpu = crate::sdk::gpu::query_info();
                let aud = if crate::sdk::audio::is_available() { "Active" } else { "Offline" };
                for d in [
                    "00:00.0 Host bridge: Intel Q35 / ICH9",
                    &format!("00:02.0 Display: {} ({}x{})", gpu.backend_name(), gpu.width, gpu.height),
                    "00:03.0 Ethernet: Intel E1000 Gigabit",
                    &format!("00:04.0 Audio: Intel AC'97 ({})", aud),
                    "00:1f.2 Storage: PIIX3 IDE/SATA",
                ] {
                    self.history.push(StyledLine::plain(d, 0xFFE2E8F0));
                }
            }
            "python" | "python3" => {
                let dyn_py = "/EOS SHARE/bin/python3";
                let has_dyn_py = fs::metadata(dyn_py).is_ok() || fs::metadata("EOS SHARE/bin/python3").is_ok();
                if has_dyn_py {
                    if args.is_empty() {
                        self.history.push(StyledLine::plain(
                            "Python 3.12.7 Dynamic (musl + dlopen) — usage: python -c '<code>' | python <script.py>",
                            0xFF34D399,
                        ));
                    } else {
                        let mut quoted_args: Vec<String> = Vec::new();
                        for a in args {
                            let resolved = if !a.starts_with('-') && self.current_dir != "/" && !a.starts_with('/')
                                && (a.ends_with(".py") || fs::metadata(format!("{}/{}", self.current_dir.trim_matches('/'), a)).is_ok())
                            {
                                format!("{}/{}", self.current_dir.trim_matches('/'), a)
                            } else {
                                a.clone()
                            };
                            if resolved.contains(' ') {
                                quoted_args.push(format!("\"{}\"", resolved));
                            } else {
                                quoted_args.push(resolved);
                            }
                        }
                        let arg_str = quoted_args.join(" ");
                        let ret = crate::syscall::sys_spawn_elf("EOS SHARE/bin/python3", &arg_str);
                        if ret >= 0 {
                            self.history.push(StyledLine::plain(
                                format!("[Dynamic CPython 3.12 + dlopen] Spawned on Core {}", ret),
                                0xFF34D399,
                            ));
                        } else {
                            self.history.push(StyledLine::plain("Failed to spawn dynamic python3", 0xFFEF4444));
                        }
                    }
                } else if args.is_empty() {
                    self.history.push(StyledLine::plain("Python 3.12 — usage: python -c '<code>' | python <script.py>", 0xFF34D399));
                } else if args[0] == "-c" && args.len() >= 2 {
                    let code = args[1..].join(" ");
                    match crate::ffi_python::run_code(&code) {
                        Ok(out) => {
                            for line in out.lines() {
                                self.history.push(StyledLine::plain(line.to_string(), 0xFFE2E8F0));
                            }
                        }
                        Err(e) => {
                            for line in e.lines() {
                                self.history.push(StyledLine::plain(line.to_string(), 0xFFEF4444));
                            }
                        }
                    }
                } else {
                    let target = args.join(" ");
                    let full = if target.starts_with('/') || self.current_dir == "/" {
                        target.clone()
                    } else {
                        format!("{}/{}", self.current_dir.trim_matches('/'), target)
                    };
                    match crate::ffi_python::run_file(&full) {
                        Ok(out) => {
                            for line in out.lines() {
                                self.history.push(StyledLine::plain(line.to_string(), 0xFFE2E8F0));
                            }
                        }
                        Err(e) => {
                            for line in e.lines() {
                                self.history.push(StyledLine::plain(line.to_string(), 0xFFEF4444));
                            }
                        }
                    }
                }
            }
            "reboot" => crate::syscall::sys_power(1),
            "shutdown" => crate::syscall::sys_power(2),
            "rm" => {
                if !args.is_empty() {
                    let target = args.join(" ");
                    let full = if target.starts_with('/') {
                        target.clone()
                    } else if self.current_dir == "/" {
                        format!("/{}", target)
                    } else {
                        format!("{}/{}", self.current_dir, target)
                    };
                    match fs::remove_file(&full) {
                        Ok(_) => self.history.push(StyledLine::plain(format!("Removed '{}'", target), 0xFF10B981)),
                        Err(e) => self.history.push(StyledLine::plain(format!("rm: '{}': {:?}", target, e), 0xFFEF4444)),
                    }
                } else {
                    self.history.push(StyledLine::plain("rm: missing operand", 0xFFEF4444));
                }
            }
            "ls" => {
                let target = if args.is_empty() { self.current_dir.clone() } else { args.join(" ") };
                let clean = target.trim_matches('/').to_string();
                let cands = [clean.clone(), format!("/{}", clean), if clean.is_empty() { ".".into() } else { clean.clone() }];
                let mut found = false;
                for p in &cands {
                    if let Ok(entries) = fs::read_dir(p) {
                        let mut items: Vec<(String, u32)> = Vec::new();
                        for e in entries.flatten() {
                            let name = e.file_name().to_string_lossy().to_string();
                            if name == "." || name == ".." { continue; }
                            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                            let lower = name.to_ascii_lowercase();
                            if is_dir {
                                items.push((format!("{}/", name), 0xFF38BDF8)); // Cyan-Blue for folders
                            } else if lower.ends_with(".elf") || lower.ends_with(".sh") || lower.ends_with(".py") {
                                items.push((format!("{}*", name), 0xFF34D399)); // Green for executables/scripts
                            } else {
                                items.push((name, 0xFFE2E8F0)); // Crisp light gray for files
                            }
                        }
                        items.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                        if items.is_empty() {
                            self.history.push(StyledLine::plain("(empty)", 0xFF64748B));
                        } else {
                            // Pixel-aligned multi-column grid (adapts to long filenames)
                            let max_len = items.iter().map(|(n, _)| n.chars().count()).max().unwrap_or(0);
                            let cols = if max_len > 26 { 2usize } else { 3usize };
                            for chunk in items.chunks(cols) {
                                let spans: Vec<(String, u32)> = chunk.iter().cloned().collect();
                                self.history.push(StyledLine::grid(spans, cols));
                            }
                        }
                        found = true;
                        break;
                    }
                }
                if !found {
                    self.history.push(StyledLine::plain(format!("ls: '{}': No such directory", target), 0xFFEF4444));
                }
            }
            "cd" => {
                let target = args.join(" ");
                if target.is_empty() || target == "~" || target == "/" {
                    self.current_dir = "/".into();
                } else if target == ".." {
                    if let Some(pos) = self.current_dir.trim_end_matches('/').rfind('/') {
                        let p = &self.current_dir[..pos];
                        self.current_dir = if p.is_empty() { "/".into() } else { p.into() };
                    } else {
                        self.current_dir = "/".into();
                    }
                } else {
                    let next = if target.starts_with('/') {
                        target.clone()
                    } else if self.current_dir == "/" {
                        format!("/{}", target)
                    } else {
                        format!("{}/{}", self.current_dir, target)
                    };
                    let clean_chk = next.trim_matches('/');
                    if clean_chk.is_empty() || fs::read_dir(clean_chk).is_ok() || fs::read_dir(&format!("/{}", clean_chk)).is_ok() {
                        self.current_dir = next;
                    } else {
                        self.history.push(StyledLine::plain(format!("cd: no such directory: {}", target), 0xFFEF4444));
                    }
                }
            }
            "cat" => {
                if !args.is_empty() {
                    let a0 = args.join(" ");
                    let fpath = if a0.starts_with('/') || self.current_dir == "/" {
                        a0.clone()
                    } else {
                        format!("{}/{}", self.current_dir.trim_matches('/'), a0)
                    };
                    let mut ok = false;
                    for p in [fpath.clone(), a0.clone(), format!("/{}", fpath)] {
                        if let Ok(content) = fs::read_to_string(&p) {
                            for line in content.lines() {
                                self.history.push(StyledLine::plain(line.to_string(), 0xFFE2E8F0));
                            }
                            ok = true;
                            break;
                        }
                    }
                    if !ok {
                        self.history.push(StyledLine::plain(format!("cat: {}: No such file", a0), 0xFFEF4444));
                    }
                } else {
                    self.history.push(StyledLine::plain("cat: missing operand", 0xFFEF4444));
                }
            }
            _ => self.execute_elf(program, args),
        }

        if let Some((target_path, append)) = redir_file {
            if self.history.len() > hist_before {
                let captured: Vec<String> = self.history.drain(hist_before..).map(|l| l.full_text()).collect();
                let mut payload = captured.join("\n") + "\n";
                if append {
                    if let Ok(existing) = fs::read_to_string(&target_path) {
                        payload = existing + &payload;
                    }
                }
                match fs::write(&target_path, payload.as_bytes()) {
                    Ok(_) => self.history.push(StyledLine::plain(format!("-> {}", target_path), 0xFF10B981)),
                    Err(e) => self.history.push(StyledLine::plain(format!("Redirection error: {:?}", e), 0xFFEF4444)),
                }
            }
        }
        self.scroll_to_bottom();
    }

    fn execute_elf(&mut self, prog: &str, args: &[String]) {
        let clean_prog = prog.trim().trim_start_matches('/');
        let arg_str = args.join(" ");
        let ret = crate::syscall::sys_spawn_elf(clean_prog, &arg_str);
        if ret >= 0 {
            self.history.push(StyledLine::plain(format!("Spawned '{}' on Core {}", clean_prog, ret), 0xFF10B981));
        } else {
            self.history.push(StyledLine::plain(format!("command not found: {}", prog), 0xFFEF4444));
        }
    }

    fn tab_autocomplete(&mut self) {
        if let Some(ghost) = self.ghost_suggestion() {
            self.input.push_str(&ghost);
            self.cursor_idx = self.input.len();
            return;
        }
        if let Some(last_sp) = self.input.rfind(' ') {
            let prefix = &self.input[last_sp + 1..];
            let (dir_part, name_part) = match prefix.rfind('/') {
                Some(sl) => (&prefix[..=sl], &prefix[sl + 1..]),
                None => ("", prefix),
            };
            let scan_dir = if dir_part.is_empty() {
                self.current_dir.clone()
            } else if dir_part.starts_with('/') {
                dir_part.to_string()
            } else if self.current_dir == "/" {
                format!("/{}", dir_part)
            } else {
                format!("{}/{}", self.current_dir.trim_end_matches('/'), dir_part)
            };
            let clean = scan_dir.trim_matches('/');
            let cand = if clean.is_empty() { ".".to_string() } else { clean.to_string() };
            if let Ok(rd) = fs::read_dir(&cand).or_else(|_| fs::read_dir(format!("/{}", clean))) {
                for e in rd.flatten() {
                    let fname = e.file_name().to_string_lossy().to_string();
                    if fname.to_ascii_lowercase().starts_with(&name_part.to_ascii_lowercase()) {
                        let suffix = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { "/" } else { "" };
                        self.input = format!("{}{}{}{}", &self.input[..=last_sp], dir_part, fname, suffix);
                        self.cursor_idx = self.input.len();
                        break;
                    }
                }
            }
        }
    }

    fn parse_command_args(&self, line: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut cur = String::new();
        let mut in_quotes = false;
        let mut quote_char = ' ';
        for c in line.chars() {
            if in_quotes {
                if c == quote_char { in_quotes = false; } else { cur.push(c); }
            } else if c == '"' || c == '\'' {
                in_quotes = true;
                quote_char = c;
            } else if c.is_whitespace() {
                if !cur.is_empty() {
                    tokens.push(core::mem::take(&mut cur));
                }
            } else {
                cur.push(c);
            }
        }
        if !cur.is_empty() { tokens.push(cur); }
        tokens
    }

    fn copy_selection(&self) -> bool {
        if !self.selectable { return false; }
        let (s, e) = match (self.sel_start, self.sel_end) {
            (Some(a), Some(b)) => {
                if a.line < b.line || (a.line == b.line && a.col <= b.col) { (a, b) } else { (b, a) }
            }
            _ => return false,
        };
        if s == e { return false; }
        let mut lines = Vec::new();
        for l_idx in s.line..=e.line {
            let line_str = if l_idx < self.history.len() {
                self.history[l_idx].full_text()
            } else if l_idx == self.history.len() {
                format!("{}{}", self.prompt_prefix(), self.input)
            } else {
                continue;
            };
            let chars: Vec<char> = line_str.chars().collect();
            let sc = if l_idx == s.line { s.col.min(chars.len()) } else { 0 };
            let ec = if l_idx == e.line { e.col.min(chars.len()) } else { chars.len() };
            if sc < ec {
                lines.push(chars[sc..ec].iter().collect::<String>());
            }
        }
        if lines.is_empty() { return false; }
        let joined = lines.join("\n");
        let _ = crate::syscall::sys_clipboard_set(&joined);
        if let Ok(mut clip) = CLIPBOARD.lock() { *clip = joined; }
        true
    }

    fn paste_clipboard(&mut self) {
        let mut clip = crate::syscall::sys_clipboard_get();
        if clip.is_empty() {
            if let Ok(local) = CLIPBOARD.lock() { clip = local.clone(); }
        }
        if !clip.is_empty() {
            self.input.insert_str(self.cursor_idx, &clip);
            self.cursor_idx += clip.len();
            self.sel_start = None;
            self.sel_end = None;
        }
    }
}

impl Widget for TerminalApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        // Kitty translucent obsidian surface (alpha = 0xCC)
        let bg_col: u32 = 0xCC0B0E14;
        for py in self.bounds.y.max(0)..(self.bounds.y + self.bounds.h).min(canvas.height as i32) {
            let row = (py as usize) * canvas.width;
            let x1 = self.bounds.x.max(0) as usize;
            let x2 = (self.bounds.x + self.bounds.w).min(canvas.width as i32) as usize;
            if x2 > x1 {
                canvas.buffer[row + x1..row + x2].fill(bg_col);
            }
        }

        let font_size = self.font_px();
        let line_h = self.line_h();
        let pad_x = theme.pt(20.0);
        let pad_y = theme.pt(18.0);
        let sb_w = theme.pt(5.0);
        let max_text_w = (self.bounds.w - (pad_x * 2) - sb_w).max(60);

        let (sel_a, sel_b) = match (self.sel_start, self.sel_end) {
            (Some(a), Some(b)) if a != b => {
                if a.line < b.line || (a.line == b.line && a.col <= b.col) { (Some(a), Some(b)) } else { (Some(b), Some(a)) }
            }
            _ => (None, None),
        };

        let mut cy = self.bounds.y + pad_y - self.scroll_y;
        for (l_idx, styled) in self.history.iter().enumerate() {
            if cy + line_h > self.bounds.y && cy < self.bounds.y + self.bounds.h {
                // Draw mouse text selection highlight
                if let (Some(sa), Some(sb)) = (sel_a, sel_b) {
                    if l_idx >= sa.line && l_idx <= sb.line {
                        let full = styled.full_text();
                        let chars: Vec<char> = full.chars().collect();
                        let sc = if l_idx == sa.line { sa.col.min(chars.len()) } else { 0 };
                        let ec = if l_idx == sb.line { sb.col.min(chars.len()) } else { chars.len() };
                        if sc < ec {
                            let pre: String = chars[..sc].iter().collect();
                            let sel: String = chars[sc..ec].iter().collect();
                            let (px_off, _) = canvas.measure_text_mono(&pre, font_size);
                            let (sw, _) = canvas.measure_text_mono(&sel, font_size);
                            if sw > 0 {
                                canvas.draw_rect(self.bounds.x + pad_x + px_off as i32, cy, sw as i32, line_h - 2, 0x6638BDF8, 3);
                            }
                        }
                    }
                }

                // Draw colored spans (pixel-aligned grid for `ls`, sequential for normal lines)
                if styled.grid_cols > 0 {
                    let col_w = (max_text_w / styled.grid_cols as i32).max(80);
                    let cell_pad = theme.pt(14.0);
                    for (idx, (span_txt, col)) in styled.spans.iter().enumerate() {
                        let gx = self.bounds.x + pad_x + (idx as i32) * col_w;
                        canvas.draw_text_mono_clipped(gx, cy, (col_w - cell_pad).max(20), span_txt, *col, font_size);
                    }
                } else {
                    let mut sx = self.bounds.x + pad_x;
                    let max_x = self.bounds.x + pad_x + max_text_w;
                    for (span_txt, col) in &styled.spans {
                        if sx >= max_x { break; }
                        canvas.draw_text_mono_clipped(sx, cy, max_x - sx, span_txt, *col, font_size);
                        let (sw, _) = canvas.measure_text_mono(span_txt, font_size);
                        sx += sw as i32;
                    }
                }
            }
            cy += line_h;
        }

        // Draw selection highlight on active prompt/input line as well
        let prompt = self.prompt_prefix();
        let prompt_line_idx = self.history.len();
        if let (Some(sa), Some(sb)) = (sel_a, sel_b) {
            if prompt_line_idx >= sa.line && prompt_line_idx <= sb.line {
                let full = format!("{}{}", prompt, self.input);
                let chars: Vec<char> = full.chars().collect();
                let sc = if prompt_line_idx == sa.line { sa.col.min(chars.len()) } else { 0 };
                let ec = if prompt_line_idx == sb.line { sb.col.min(chars.len()) } else { chars.len() };
                if sc < ec {
                    let pre: String = chars[..sc].iter().collect();
                    let sel: String = chars[sc..ec].iter().collect();
                    let (px_off, _) = canvas.measure_text_mono(&pre, font_size);
                    let (sw, _) = canvas.measure_text_mono(&sel, font_size);
                    if sw > 0 {
                        canvas.draw_rect(self.bounds.x + pad_x + px_off as i32, cy, sw as i32, line_h - 2, 0x6638BDF8, 3);
                    }
                }
            }
        }

        // Kitty Prompt + Input + Fish Ghost Autosuggestion + Animated Beam Cursor
        let (pw, _) = canvas.measure_text_mono(&prompt, font_size);
        canvas.draw_text_mono(self.bounds.x + pad_x, cy, &prompt, theme.accent_hover, font_size);

        let in_x = self.bounds.x + pad_x + pw as i32;
        let in_max_w = (max_text_w - pw as i32).max(20);
        canvas.draw_text_mono_clipped(in_x, cy, in_max_w, &self.input, 0xFFF8FAFC, font_size);

        let before_cur = &self.input[..self.cursor_idx.min(self.input.len())];
        let (cx_off, _) = canvas.measure_text_mono(before_cur, font_size);

        if let Some(ghost) = self.ghost_suggestion() {
            let (in_w, _) = canvas.measure_text_mono(&self.input, font_size);
            canvas.draw_text_mono_clipped(in_x + in_w as i32, cy, (in_max_w - in_w as i32).max(0), &ghost, 0xFF64748B, font_size);
        }

        if self.cursor_visible {
            let cur_x = in_x + cx_off as i32;
            canvas.draw_rect(cur_x, cy + 2, 3, (font_size as i32).max(16), 0xFFE2E8F0, 1);
        }

        // Minimal thin scrollbar
        let total_h = ((self.history.len() + 1) as i32) * line_h + pad_y * 2;
        if total_h > self.bounds.h && self.bounds.h > 20 {
            let sb_x = self.bounds.x + self.bounds.w - sb_w - 4;
            let thumb_h = ((self.bounds.h as f32 / total_h as f32) * self.bounds.h as f32).max(24.0) as i32;
            let max_s = (total_h - self.bounds.h).max(1);
            let thumb_y = self.bounds.y + ((self.scroll_y as f32 / max_s as f32) * (self.bounds.h - thumb_h) as f32) as i32;
            canvas.draw_rect(sb_x, thumb_y, sb_w, thumb_h, 0x6694A3B8, (sb_w / 2) as usize);
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) {
            self.is_selecting = false;
            return false;
        }
        if !self.selectable { return true; }

        let theme = get_theme();
        let line_h = self.line_h();
        let pad_x = theme.pt(20.0);
        let pad_y = theme.pt(18.0);
        let rel_y = my - (self.bounds.y + pad_y) + self.scroll_y;
        let line_idx = if rel_y >= 0 { (rel_y / line_h) as usize } else { 0 };
        let target_x = (mx - (self.bounds.x + pad_x)).max(0);

        // Exact pixel-to-column hit-testing using the active font's real glyph advance_width
        let line_str = if line_idx < self.history.len() {
            self.history[line_idx].full_text()
        } else if line_idx == self.history.len() {
            format!("{}{}", self.prompt_prefix(), self.input)
        } else {
            String::new()
        };

        let col_idx = if let Some(font) = crate::framework::fonts::get_font_for_role(crate::framework::fonts::FontRole::Mono) {
            let scale = rusttype::Scale::uniform(self.font_px() as f32);
            let mut acc_x = 0i32;
            let mut found_col = 0usize;
            for (i, c) in line_str.chars().enumerate() {
                let adv = (font.glyph(c).scaled(scale).h_metrics().advance_width.round() as i32).max(1);
                if target_x < acc_x + adv / 2 {
                    found_col = i;
                    break;
                }
                acc_x += adv;
                found_col = i + 1;
            }
            found_col
        } else {
            let fallback_w = ((self.font_px() as f32 * 0.48).round() as i32).max(6);
            (target_x / fallback_w) as usize
        };
        let pos = TextPos { line: line_idx, col: col_idx };

        if pressed {
            if !self.is_selecting {
                self.is_selecting = true;
                self.sel_start = Some(pos);
                self.sel_end = Some(pos);
            } else {
                self.sel_end = Some(pos);
            }
        } else if self.is_selecting {
            self.sel_end = Some(pos);
            self.is_selecting = false;
        }
        true
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let line_h = self.line_h();
        let total_h = ((self.history.len() + 1) as i32) * line_h + get_theme().pt(36.0);
        let max_s = (total_h - self.bounds.h).max(0);
        let step = line_h * 2;
        if dy < 0 {
            self.scroll_y = (self.scroll_y + step).min(max_s);
        } else if dy > 0 {
            self.scroll_y = (self.scroll_y - step).max(0);
        }
        true
    }

    fn handle_key(&mut self, keycode: u8, mods: u8) -> bool {
        self.reset_cursor_blink();
        let is_shift = (mods & 0x01) != 0;
        let is_ctrl = (mods & 0x02) != 0;

        match keycode {
            0x0F => { self.tab_autocomplete(); true }
            0x1C => { self.execute(); true }
            // Ctrl+Shift+C -> Copy Selection (Kitty standard)
            0x2E if is_ctrl && is_shift => {
                self.copy_selection();
                true
            }
            // Ctrl+C -> Interrupt / Cancel current command ONLY
            0x2E if is_ctrl => {
                self.history.push(StyledLine::spans(vec![
                    (self.prompt_prefix(), 0xFF38BDF8),
                    (format!("{}^C", self.input), 0xFFF87171),
                ]));
                self.input.clear();
                self.cursor_idx = 0;
                self.sel_start = None;
                self.sel_end = None;
                self.scroll_to_bottom();
                true
            }
            // Ctrl+Shift+V or Ctrl+V -> Paste
            0x2F if is_ctrl => {
                self.paste_clipboard();
                true
            }
            0x48 => {
                if !self.cmd_history.is_empty() {
                    let next = match self.history_idx {
                        Some(i) => i.saturating_sub(1),
                        None => self.cmd_history.len() - 1,
                    };
                    self.history_idx = Some(next);
                    self.input = self.cmd_history[next].clone();
                    self.cursor_idx = self.input.len();
                    return true;
                }
                false
            }
            0x50 => {
                if let Some(i) = self.history_idx {
                    if i + 1 < self.cmd_history.len() {
                        self.history_idx = Some(i + 1);
                        self.input = self.cmd_history[i + 1].clone();
                        self.cursor_idx = self.input.len();
                    } else {
                        self.history_idx = None;
                        self.input.clear();
                        self.cursor_idx = 0;
                    }
                    return true;
                }
                false
            }
            0x4B => { if self.cursor_idx > 0 { self.cursor_idx -= 1; } true }
            0x4D => {
                if self.cursor_idx < self.input.len() {
                    self.cursor_idx += 1;
                } else if let Some(ghost) = self.ghost_suggestion() {
                    self.input.push_str(&ghost);
                    self.cursor_idx = self.input.len();
                }
                true
            }
            0x47 => { self.cursor_idx = 0; true }
            0x4F => { self.cursor_idx = self.input.len(); true }
            _ => false,
        }
    }

    fn handle_char(&mut self, c: char) -> bool {
        self.reset_cursor_blink();
        if c == '\x08' {
            if self.cursor_idx > 0 {
                self.cursor_idx -= 1;
                self.input.remove(self.cursor_idx);
                return true;
            }
            return false;
        }
        if c >= ' ' && c <= '~' {
            self.input.insert(self.cursor_idx, c);
            self.cursor_idx += 1;
            self.sel_start = None;
            self.sel_end = None;
            return true;
        }
        false
    }
}
