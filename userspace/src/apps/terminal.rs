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
    shell: crate::shell::ShellSession,
    history: Vec<StyledLine>,
    input: String,
    cursor_idx: usize,
    history_idx: Option<usize>,
    scroll_y: i32,
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
        Self {
            bounds: Rect::default(),
            shell: crate::shell::ShellSession::new(),
            history: vec![
                StyledLine::spans(vec![
                    ("EOS 0.1.0".into(), 0xFF38BDF8),
                    ("  •  ".into(), 0xFF475569),
                    ("root@eos (Unified POSIX)".into(), 0xFF34D399),
                    ("  •  Ctrl+Shift+C: Copy  |  Ctrl+C: Cancel".into(), 0xFF64748B),
                ]),
                StyledLine::plain("", 0xFFFFFFFF),
            ],
            input: String::new(),
            cursor_idx: 0,
            history_idx: None,
            scroll_y: 0,
            selectable: true,
            is_selecting: false,
            sel_start: None,
            sel_end: None,
            cursor_visible: true,
            last_blink: Instant::now(),
        }
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
        self.shell.prompt_prefix(false)
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
        for cmd in self.shell.cmd_history.iter().rev() {
            if cmd.len() > self.input.len() && cmd.starts_with(&self.input) {
                return Some(cmd[self.input.len()..].to_string());
            }
        }
        if !self.input.contains(' ') {
            let builtins = [
                "help", "whoami", "pwd", "history", "ls", "cd", "cat", "head", "tail", "wc",
                "grep", "sort", "uniq", "cut", "tr", "find", "stat", "hexdump", "touch", "mkdir",
                "cp", "mv", "rm", "clear", "uname", "echo", "env", "export", "ps", "kill", "free",
                "lspci", "audit", "ifconfig", "dhcp", "arp", "ping", "ssh", "sshd", "reboot",
                "shutdown", "python",
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

        self.history_idx = None;
        self.input.clear();
        self.cursor_idx = 0;
        self.sel_start = None;
        self.sel_end = None;

        if raw_cmd.is_empty() {
            self.scroll_to_bottom();
            return;
        }

        let is_plain_ls = !raw_cmd.contains('|') && !raw_cmd.contains('>')
            && (raw_cmd == "ls" || (raw_cmd.starts_with("ls ") && !raw_cmd.contains("-l")));

        let res = self.shell.execute_line(&raw_cmd, true);
        if res.clear_screen {
            self.history.clear();
            self.scroll_y = 0;
            return;
        }

        if is_plain_ls && !res.lines.is_empty() && !res.lines[0].0.starts_with("ls:") {
            let max_len = res.lines.iter().map(|(n, _)| n.chars().count()).max().unwrap_or(0);
            let cols = if max_len > 26 { 2usize } else { 3usize };
            for chunk in res.lines.chunks(cols) {
                self.history.push(StyledLine::grid(chunk.to_vec(), cols));
            }
        } else {
            for (line, col) in res.lines {
                self.history.push(StyledLine::plain(line, col));
            }
        }
        self.scroll_to_bottom();
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
                self.shell.cwd.clone()
            } else if dir_part.starts_with('/') {
                dir_part.to_string()
            } else if self.shell.cwd == "/" {
                format!("/{}", dir_part)
            } else {
                format!("{}/{}", self.shell.cwd.trim_end_matches('/'), dir_part)
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
                self.shell.reload_history();
                if !self.shell.cmd_history.is_empty() {
                    let next = match self.history_idx {
                        Some(i) => i.saturating_sub(1),
                        None => self.shell.cmd_history.len() - 1,
                    };
                    self.history_idx = Some(next);
                    self.input = self.shell.cmd_history[next].clone();
                    self.cursor_idx = self.input.len();
                    return true;
                }
                false
            }
            0x50 => {
                if let Some(i) = self.history_idx {
                    if i + 1 < self.shell.cmd_history.len() {
                        self.history_idx = Some(i + 1);
                        self.input = self.shell.cmd_history[i + 1].clone();
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
