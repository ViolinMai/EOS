use crate::framework::canvas::Canvas;
use crate::framework::theme::get_theme;
use crate::framework::widget::Rect;

pub struct MenuBar {
    pub bounds: Rect,
    pub is_apple_open: bool,
}

impl MenuBar {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            is_apple_open: false,
        }
    }

    pub fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let h = self.bounds.h;
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, h, theme.bg_menubar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + h - 1, self.bounds.w, theme.border_menubar);

        let apple_w = theme.pt(28.0);
        let cur_x = self.bounds.x + theme.pt(12.0);
        canvas.draw_text(cur_x, self.bounds.y + (h - (theme.font_title() as i32)) / 2, "", theme.text_primary, theme.font_title());

        let menus = ["File", "Edit", "View", "Window", "Help"];
        let mut mx = cur_x + apple_w;
        for m in menus {
            let (tw, th) = canvas.measure_text(m, theme.font_body());
            canvas.draw_text(mx, self.bounds.y + (h - (th as i32)) / 2, m, theme.text_secondary, theme.font_body());
            mx += tw as i32 + theme.pt(16.0);
        }

        let sys_info = "EOS 64-bit | Core 2 Userspace";
        let (sw, sh) = canvas.measure_text(sys_info, theme.font_caption());
        let rx = self.bounds.x + self.bounds.w - sw as i32 - theme.pt(16.0);
        canvas.draw_text(rx, self.bounds.y + (h - (sh as i32)) / 2, sys_info, theme.text_muted, theme.font_caption());
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) {
            return false;
        }
        if pressed {
            let theme = get_theme();
            if mx < self.bounds.x + theme.pt(40.0) {
                self.is_apple_open = !self.is_apple_open;
            }
        }
        true
    }
}
