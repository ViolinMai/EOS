use crate::framework::*;
use crate::f_info;

pub struct TextEditApp {
    bounds: Rect,
    pub title: String,
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_y: usize,
}

impl TextEditApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            title: "Notes.txt".into(),
            lines: vec!["EOS TextEdit Editor".into(), "Type notes or code here...".into()],
            cursor_row: 1,
            cursor_col: 26,
            scroll_y: 0,
        }
    }

    pub fn with_content(title: String, content: String) -> Self {
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(String::from).collect()
        };
        f_info!("TEXTEDIT", "Loaded document '{}' with {} lines", title, lines.len());
        Self {
            bounds: Rect::default(),
            title,
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_y: 0,
        }
    }
}

impl Widget for TextEditApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let tb_h = theme.pt(36.0);
        let status_h = theme.pt(28.0);

        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);
        canvas.draw_text(self.bounds.x + theme.pt(18.0), self.bounds.y + theme.pt(8.0), &format!("💾 {}", self.title), theme.text_primary, theme.font_body());

        let editor_y = self.bounds.y + tb_h + 1;
        let editor_h = self.bounds.h.saturating_sub(tb_h + status_h + 1);
        canvas.draw_rect(self.bounds.x, editor_y, self.bounds.w, editor_h, theme.bg_window, 0);

        let gutter_w = theme.pt(54.0);
        canvas.draw_rect(self.bounds.x, editor_y, gutter_w, editor_h, 0xFF141E2E, 0);
        canvas.draw_line_v(self.bounds.x + gutter_w, editor_y, editor_h, theme.border_window);

        let line_h = theme.pt(26.0);
        let max_visible_lines = editor_h / line_h;
        let mut cy = editor_y + theme.pt(8.0);

        for i in 0..max_visible_lines {
            let line_idx = self.scroll_y + i;
            if line_idx >= self.lines.len() { break; }
            if cy + line_h > editor_y + editor_h { break; }

            // Line numbers in gutter
            canvas.draw_text(self.bounds.x + theme.pt(12.0), cy, &format!("{:3}", line_idx + 1), theme.text_muted, theme.font_caption());

            // Text line with horizontal bounds clipping
            let clean_line = self.lines[line_idx].replace('\t', "    ");
            let text_avail_w = self.bounds.w.saturating_sub(gutter_w + theme.pt(24.0));
            canvas.draw_text_clipped(self.bounds.x + gutter_w + theme.pt(16.0), cy, text_avail_w, &clean_line, theme.text_primary, theme.font_body());

            // Cursor
            if line_idx == self.cursor_row {
                let cursor_x = self.bounds.x + gutter_w + theme.pt(16.0) + (self.cursor_col * theme.pt(9.0));
                if cursor_x < self.bounds.x + self.bounds.w.saturating_sub(theme.pt(8.0)) {
                    canvas.draw_rect(cursor_x, cy, 3, theme.pt(18.0), theme.accent_hover, 0);
                }
            }
            cy += line_h;
        }

        // Scrollbar Track & Thumb
        if self.lines.len() > max_visible_lines && max_visible_lines > 0 {
            let track_x = self.bounds.x + self.bounds.w.saturating_sub(theme.pt(8.0));
            let thumb_h = ((max_visible_lines * editor_h) / self.lines.len()).max(theme.pt(24.0));
            let thumb_y = editor_y + ((self.scroll_y * (editor_h - thumb_h)) / (self.lines.len() - max_visible_lines));
            canvas.draw_rect(track_x, thumb_y, theme.pt(6.0), thumb_h, theme.text_muted, theme.pt(3.0));
        }

        // Status bar
        let sb_y = self.bounds.y + self.bounds.h.saturating_sub(status_h);
        canvas.draw_rect(self.bounds.x, sb_y, self.bounds.w, status_h, theme.bg_menubar, 0);
        canvas.draw_line_h(self.bounds.x, sb_y, self.bounds.w, theme.border_menubar);
        canvas.draw_text(self.bounds.x + theme.pt(16.0), sb_y + theme.pt(6.0), &format!("Ln {}, Col {} | Total Lines: {}", self.cursor_row + 1, self.cursor_col + 1, self.lines.len()), theme.text_secondary, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, _p: bool) -> bool {
        self.bounds.contains(mx, my)
    }

    fn handle_char(&mut self, c: char) -> bool {
        if c == '\n' {
            self.lines.insert(self.cursor_row + 1, String::new());
            self.cursor_row += 1;
            self.cursor_col = 0;
            let theme = get_theme();
            let visible_count = self.bounds.h.saturating_sub(theme.pt(64.0)) / theme.pt(26.0);
            if self.cursor_row >= self.scroll_y + visible_count {
                self.scroll_y = self.cursor_row.saturating_sub(visible_count.saturating_sub(1));
            }
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
        } else if c == '\t' {
            self.lines[self.cursor_row].insert_str(self.cursor_col, "    ");
            self.cursor_col += 4;
            return true;
        } else if c >= ' ' && c <= '~' {
            self.lines[self.cursor_row].insert(self.cursor_col, c);
            self.cursor_col += 1;
            return true;
        }
        false
    }
}
