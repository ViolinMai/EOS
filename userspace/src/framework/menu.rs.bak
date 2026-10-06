use crate::framework::canvas::Canvas;
use crate::framework::theme::get_theme;
use crate::framework::widget::{Rect, Widget};
use std::any::Any;

pub struct MenuBarWidget {
    pub bounds: Rect,
}

impl MenuBarWidget {
    pub fn new() -> Self {
        Self { bounds: Rect::default() }
    }
}

impl Widget for MenuBarWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let h = self.bounds.h;

        // شريط MenuBar شفاف وضبابي بالكامل (Frosted Glass Blur)
        canvas.draw_frosted_glass_rect(self.bounds.x, self.bounds.y, self.bounds.w, h, 0xBB1E293B, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + h - 1, self.bounds.w, 0x44FFFFFF);

        let apple_w = theme.pt(28.0);
        let cur_x = self.bounds.x + theme.pt(14.0);
        canvas.draw_text(cur_x, self.bounds.y + (h - (theme.font_title() as i32)) / 2, "", theme.text_primary, theme.font_title());

        let menus = ["Finder", "File", "Edit", "View", "Window", "Help"];
        let mut mx = cur_x + apple_w;
        for m in menus {
            let (tw, th) = canvas.measure_text(m, theme.font_body());
            canvas.draw_text(mx, self.bounds.y + (h - (th as i32)) / 2, m, theme.text_secondary, theme.font_body());
            mx += tw as i32 + theme.pt(16.0);
        }

        let sys_info = "EOS 64-bit | Ring 3 Musl";
        let (sw, sh) = canvas.measure_text(sys_info, theme.font_caption());
        let rx = self.bounds.x + self.bounds.w - sw as i32 - theme.pt(16.0);
        canvas.draw_text(rx, self.bounds.y + (h - (sh as i32)) / 2, sys_info, theme.text_muted, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, _pressed: bool) -> bool {
        self.bounds.contains(mx, my)
    }
}
