
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::input::InputEvent;
use super::App;
use crate::gui::draw::{self, FrameBuffer};
use crate::gui::theme::Theme;

pub struct TerminalApp { pub history: Vec<String>, pub input: String, pub needs_redraw: bool }

impl TerminalApp {
    pub fn new() -> Self {
        let mut t = Self { history: Vec::new(), input: String::new(), needs_redraw: true };
        t.history.push("EOS POSIX Shell v2.0 - Ring 3 Musl Compatible".to_string());
        t.history.push("Commands: selftest, ls [dir], run <file.elf>, clear, uptime, reboot".to_string());
        t
    }

    fn execute(&mut self) {
        let cmd = self.input.clone();
        self.history.push(alloc::format!("eos> {}", cmd));
        self.input.clear();
        let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
        if parts.is_empty() { return; }
        
        match parts[0] {
            "help" => { self.history.push("  run <file.elf> : Spawn isolated Ring 3 process".to_string()); self.history.push("  ls [dir]       : Directory enumeration".to_string()); },
            "run" => {
                if parts.len() > 1 {
                    let arg = if parts.len() > 2 { parts[2] } else { "" };
                    self.history.push(alloc::format!("Spawning '{}' with args '{}'...", parts[1], arg));
                    if let Err(e) = crate::task::request_elf_execution(parts[1], arg) { self.history.push(alloc::format!("Error: {}", e)); }
                } else { self.history.push("Usage: run <binary.elf> [arg]".to_string()); }
            },
            "clear" => self.history.clear(),
            "uptime" => self.history.push(alloc::format!("Uptime: {} seconds", crate::arch::x86_64::pit::get_uptime_seconds())),
            "reboot" => crate::arch::x86_64::power::reboot(),
            "ls" => {
                let folder = if parts.len() > 1 { parts[1] } else { "Storage" };
                let items = crate::fs::list_directory_contents(folder, 0);
                for item in items { match item { crate::fs::FsItem::Directory(d, _) => self.history.push(alloc::format!("[DIR]  {}", d)), crate::fs::FsItem::File(f, sz, _) => self.history.push(alloc::format!("[FILE] {:<20} ({} B)", f, sz)), } }
            },
            _ => self.history.push(alloc::format!("Command not found: '{}'. Type 'help'.", parts[0])),
        }
    }
}

impl App for TerminalApp {
    fn wants_redraw(&self) -> bool { self.needs_redraw }

    fn draw(&mut self, fb: &mut FrameBuffer, _theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        draw::draw_rect(fb, x, y, w, h, 0xFF18181B);
        let mut cy = y + 10;
        let line_h = 24;
        let max_lines = h.saturating_sub(40) / line_h;
        let start = if self.history.len() > max_lines { self.history.len() - max_lines } else { 0 };
        
        for line in &self.history[start..] {
            let col = if line.starts_with(" [PASS]") { 0xFF4ADE80 } else if line.starts_with(" [FAIL]") { 0xFFF87171 } else if line.starts_with(" [INFO]") { 0xFF60A5FA } else { 0xFFE2E8F0 };
            draw::draw_text(fb, x + 12, cy, line, col, 1);
            cy += line_h;
        }
        draw::draw_text(fb, x + 12, cy, &alloc::format!("eos> {}_", self.input), 0xFF38BDF8, 1);
        self.needs_redraw = false;
    }

    fn on_event(&mut self, event: &InputEvent, _mx: isize, _my: isize) {
        if let InputEvent::Char(c) = event {
            if *c == '\x08' { self.input.pop(); } else if *c == '\n' { self.execute(); } else if *c >= ' ' && *c <= '~' { self.input.push(*c); }
            self.needs_redraw = true;
        }
    }

    fn title(&self) -> &str { "Terminal" }
    fn icon(&self) -> &'static str { "TRM" }
}


