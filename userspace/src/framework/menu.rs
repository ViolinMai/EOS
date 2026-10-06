use crate::framework::canvas::Canvas;
use crate::framework::widget::Rect;
use crate::framework::theme::get_theme;
use crate::framework::vector::draw_vector_icon;

pub struct MenuBarWidget {
    pub bounds: Rect,
    pub active_app_title: String,
    pub active_app_menus: Vec<String>,
}

impl MenuBarWidget {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            active_app_title: "Finder".to_string(),
            active_app_menus: vec!["File".into(), "Edit".into(), "View".into(), "Help".into()],
        }
    }

    pub fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    pub fn set_active_app(&mut self, title: &str) {
        let clean = title.split('-').next().unwrap_or(title).trim();
        self.active_app_title = clean.to_string();
        self.active_app_menus = match clean {
            "Finder" => vec!["File".into(), "Edit".into(), "View".into(), "Go".into(), "Window".into()],
            "Settings" => vec!["Preferences".into(), "Display".into(), "Theme".into(), "Help".into()],
            "Terminal" => vec!["Shell".into(), "Edit".into(), "View".into(), "Window".into()],
            "Activity Monitor" => vec!["Process".into(), "Inspect".into(), "Diagnostic".into(), "Help".into()],
            "Browser" => vec!["Navigate".into(), "Bookmarks".into(), "Network".into(), "Window".into()],
            "TextEdit" => vec!["File".into(), "Edit".into(), "Format".into(), "View".into(), "Help".into()],
            _ => vec!["File".into(), "Edit".into(), "Window".into()],
        };
    }

    pub fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let h = self.bounds.h;

        canvas.draw_frosted_glass_rect(self.bounds.x, self.bounds.y, self.bounds.w, h, 0xEE18181B, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + h - 1, self.bounds.w, theme.border_window);

        let logo_x = self.bounds.x + theme.pt(14.0);
        let logo_y = self.bounds.y + (h - theme.pt(16.0)) / 2;
        draw_vector_icon(canvas, "gear", logo_x, logo_y, theme.pt(16.0) as i32, theme.pt(16.0) as i32, Some(theme.accent_hover));

        let mut cur_x = logo_x + theme.pt(24.0);
        let app_name = &self.active_app_title;
        canvas.draw_text(cur_x, self.bounds.y + theme.pt(6.0), app_name, 0xFFFFFFFF, theme.font_body());
        let (aw, _) = canvas.measure_text(app_name, theme.font_body());
        cur_x += aw as i32 + theme.pt(22.0);

        for menu in &self.active_app_menus {
            canvas.draw_text(cur_x, self.bounds.y + theme.pt(6.0), menu, theme.text_secondary, theme.font_caption());
            let (mw, _) = canvas.measure_text(menu, theme.font_caption());
            cur_x += mw as i32 + theme.pt(18.0);
        }

        let clock_str = get_system_clock_string();
        let (cw, _) = canvas.measure_text(&clock_str, theme.font_caption());
        let clock_x = self.bounds.x + self.bounds.w - (cw as i32) - theme.pt(18.0);
        canvas.draw_text(clock_x, self.bounds.y + theme.pt(6.0), &clock_str, 0xFFF4F4F5, theme.font_caption());
    }
}

pub fn get_system_clock_string() -> String {
    let mut tv = [0u64; 2];
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 96,
            in("rdi") tv.as_mut_ptr() as u64,
            in("rsi") 0,
            out("rcx") _, out("r11") _
        );
    }
    let total_secs = tv[0];
    let hours = (total_secs / 3600) % 24;
    let mins = (total_secs / 60) % 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}
