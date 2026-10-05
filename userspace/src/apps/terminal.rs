use crate::framework::*;

pub struct TerminalApp {
    bounds: Rect,
    history: Vec<String>,
    input: String,
    scroll_y: i32,
}

impl TerminalApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            history: vec![
                "EOS POSIX Ring 3 Terminal".into(),
                "Type 'help' or 'clear' to test shell execution.".into(),
            ],
            input: String::new(),
            scroll_y: 0,
        }
    }

    fn execute(&mut self) {
        let cmd = self.input.trim().to_string();
        self.history.push(format!("eos$ {}", cmd));
        self.input.clear();

        match cmd.as_str() {
            "help" => {
                self.history.push("Commands: help, clear, uname, ping, ipconfig".into());
            }
            "clear" => {
                self.history.clear();
            }
            "uname" => {
                self.history.push("EOS 0.1.0 x86_64 SMP Monolithic + Ring 3 UI".into());
            }
            "ipconfig" | "net" => {
                self.history.push("eth0: IP=10.0.2.15 Subnet=255.255.255.0 Gateway=10.0.2.2".into());
            }
            "" => {}
            _ => {
                self.history.push(format!("Command not found: {}", cmd));
            }
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
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF18181B, 0);

        let line_h = theme.pt(20.0);
        let mut cy = self.bounds.y + theme.pt(12.0) - self.scroll_y;

        for line in &self.history {
            if cy + line_h > self.bounds.y && cy < self.bounds.y + self.bounds.h - line_h {
                canvas.draw_text_clipped(self.bounds.x + theme.pt(14.0), cy, self.bounds.w - theme.pt(28.0), line, 0xFFE2E8F0, theme.font_body());
            }
            cy += line_h;
        }

        let prompt = format!("eos$ {}_", self.input);
        canvas.draw_text_clipped(self.bounds.x + theme.pt(14.0), cy, self.bounds.w - theme.pt(28.0), &prompt, 0xFF38BDF8, theme.font_body());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, _pressed: bool) -> bool {
        self.bounds.contains(mx, my)
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if dy < 0 {
            self.scroll_y = (self.scroll_y + 24).min(2000);
        } else {
            self.scroll_y = (self.scroll_y - 24).max(0);
        }
        true
    }

    fn handle_key(&mut self, keycode: u8, _mods: u8) -> bool {
        if keycode == 0x1C { // Enter
            self.execute();
            return true;
        }
        false
    }

    fn handle_char(&mut self, c: char) -> bool {
        if c == '\x08' {
            self.input.pop();
            return true;
        } else if c >= ' ' && c <= '~' {
            self.input.push(c);
            return true;
        }
        false
    }
}
