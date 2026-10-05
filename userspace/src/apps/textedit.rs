use crate::framework::*;

pub struct TextEditApp {
    bounds: Rect,
    pub title: String,
    pub content: String,
    cursor_pos: usize,
}

impl TextEditApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            title: "Untitled.txt".into(),
            content: "Welcome to TextEdit on EOS!\nEdit your notes here.".into(),
            cursor_pos: 0,
        }
    }

    pub fn with_content(title: String, content: String) -> Self {
        let len = content.len();
        Self {
            bounds: Rect::default(),
            title,
            content,
            cursor_pos: len,
        }
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
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFFFFFFFF, 0);

        let gutter_w = theme.pt(42.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, gutter_w, self.bounds.h, 0xFFF1F5F9, 0);
        canvas.draw_line_v(self.bounds.x + gutter_w, self.bounds.y, self.bounds.h, 0xFFE2E8F0);

        let line_h = theme.pt(22.0);
        let mut cy = self.bounds.y + theme.pt(12.0);

        for (idx, line) in self.content.lines().enumerate() {
            canvas.draw_text(self.bounds.x + theme.pt(8.0), cy, &format!("{}", idx + 1), 0xFF94A3B8, theme.font_caption());
            canvas.draw_text_clipped(self.bounds.x + gutter_w + theme.pt(12.0), cy, self.bounds.w - gutter_w - theme.pt(24.0), line, 0xFF0F172A, theme.font_body());
            cy += line_h;
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, _pressed: bool) -> bool {
        self.bounds.contains(mx, my)
    }

    fn handle_key(&mut self, keycode: u8, _mods: u8) -> bool {
        if keycode == 0x1C { // Enter
            self.content.push('\n');
            return true;
        }
        false
    }

    fn handle_char(&mut self, c: char) -> bool {
        if c == '\x08' {
            self.content.pop();
            return true;
        } else if c >= ' ' && c <= '~' {
            self.content.push(c);
            return true;
        }
        false
    }
}
