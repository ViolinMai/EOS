use crate::framework::canvas::Canvas;
use crate::framework::widget::Rect;
use crate::framework::theme::get_theme;
use crate::framework::vector::draw_vector_icon;

#[derive(Clone, Debug)]
pub struct StartMenuItem {
    pub label: String,
    pub app_target: String,
    pub icon: String,
}

pub struct Win7StartMenu {
    pub bounds: Rect,
    pub is_open: bool,
    pub search_query: String,
    pub left_items: Vec<StartMenuItem>,
    pub right_items: Vec<(&'static str, &'static str)>,
    pub hovered_left: Option<usize>,
    pub hovered_right: Option<usize>,
}

impl Win7StartMenu {
    pub fn new() -> Self {
        Self {
            bounds: Rect::new(0, 560, 420, 480),
            is_open: false,
            search_query: String::new(),
            left_items: vec![
                StartMenuItem { label: "Windows Explorer".into(), app_target: "Finder".into(), icon: "folder".into() },
                StartMenuItem { label: "Command Prompt".into(), app_target: "Terminal".into(), icon: "terminal".into() },
                StartMenuItem { label: "Notepad".into(), app_target: "TextEdit".into(), icon: "doc".into() },
                StartMenuItem { label: "WindowsTask Manager".into(), app_target: "Activity Monitor".into(), icon: "monitor".into() },
                StartMenuItem { label: "Internet Explorer".into(), app_target: "Browser".into(), icon: "browser".into() },
                StartMenuItem { label: "Control Panel".into(), app_target: "Settings".into(), icon: "gear".into() },
            ],
            right_items: vec![
                ("march", "Finder"),
                ("Documents", "Finder"),
                ("Pictures", "Finder"),
                ("Computer", "Finder"),
                ("Control Panel", "Settings"),
                ("Devices and Printers", "Settings"),
                ("Default Programs", "Settings"),
                ("Help and Support", "Browser"),
            ],
            hovered_left: None,
            hovered_right: None,
        }
    }

    pub fn layout_above_taskbar(&mut self, screen_h: i32, taskbar_h: i32) {
        let theme = get_theme();
        let w = theme.pt(400.0);
        let h = theme.pt(460.0);
        self.bounds = Rect::new(0, (screen_h - taskbar_h - h).max(0), w, h);
    }

    pub fn filtered_left(&self) -> Vec<StartMenuItem> {
        if self.search_query.trim().is_empty() {
            return self.left_items.clone();
        }
        let q = self.search_query.to_lowercase();
        self.left_items
            .iter()
            .filter(|i| i.label.to_lowercase().contains(&q) || i.app_target.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    pub fn paint(&self, canvas: &mut Canvas) {
        if !self.is_open { return; }
        let theme = get_theme();
        let x = self.bounds.x;
        let y = self.bounds.y;
        let w = self.bounds.w;
        let h = self.bounds.h;

        // Outer Aero Glass Frame
        canvas.draw_frosted_glass_rect(x, y, w, h, theme.start_menu_right_bg, theme.pt(8.0) as usize);
        canvas.draw_rect_outline(x, y, w, h, theme.border_window, theme.pt(8.0) as usize);
        canvas.draw_rect_outline(x + 1, y + 1, w - 2, h - 2, theme.border_frost_highlight, theme.pt(7.0) as usize);

        let pad = theme.pt(8.0);
        let left_w = (w * 58) / 100;
        let left_h = h - pad * 2;

        // Left White Pane (Pinned/Recent Programs + Search Box)
        canvas.draw_rect(x + pad, y + pad, left_w, left_h, theme.start_menu_left_bg, theme.pt(5.0) as usize);
        canvas.draw_rect_outline(x + pad, y + pad, left_w, left_h, 0xFFCBD5E1, theme.pt(5.0) as usize);

        let items = self.filtered_left();
        let row_h = theme.pt(36.0);
        let mut iy = y + pad + theme.pt(8.0);
        for (idx, item) in items.iter().enumerate() {
            if iy + row_h > y + h - theme.pt(56.0) { break; }
            let is_hov = self.hovered_left == Some(idx);
            if is_hov {
                canvas.draw_rect(x + pad + 4, iy, left_w - 8, row_h, 0xFFD8EAFD, 4);
                canvas.draw_rect_outline(x + pad + 4, iy, left_w - 8, row_h, 0xFF7DA2CE, 4);
            }
            draw_vector_icon(canvas, &item.icon, x + pad + theme.pt(12.0), iy + theme.pt(8.0), theme.pt(20.0), theme.pt(20.0), Some(0xFF0284C7));
            canvas.draw_text_clipped(
                x + pad + theme.pt(40.0),
                iy + theme.pt(8.0),
                left_w - theme.pt(48.0),
                &item.label,
                0xFF0F172A,
                theme.font_body(),
            );
            iy += row_h + 2;
        }

        // Bottom Left Search Input Box ("Search programs and files")
        let sb_h = theme.pt(30.0);
        let sb_y = y + h - pad - sb_h - theme.pt(6.0);
        canvas.draw_line_h(x + pad + 6, sb_y - theme.pt(6.0), left_w - 12, 0xFFE2E8F0);
        canvas.draw_rect(x + pad + 8, sb_y, left_w - 16, sb_h, 0xFFFFFFFF, 3);
        canvas.draw_rect_outline(x + pad + 8, sb_y, left_w - 16, sb_h, 0xFF3B82F6, 3);
        let search_txt = if self.search_query.is_empty() {
            "Search programs and files"
        } else {
            &self.search_query
        };
        let search_col = if self.search_query.is_empty() { 0xFF64748B } else { 0xFF0F172A };
        canvas.draw_text_clipped(
            x + pad + 16,
            sb_y + theme.pt(6.0),
            left_w - 32,
            search_txt,
            search_col,
            theme.font_caption(),
        );

        // Right Dark-Glass System Column (Computer, Documents, Control Panel, Shut Down)
        let rx = x + pad + left_w + theme.pt(8.0);
        let rw = w - (left_w + pad * 2 + theme.pt(8.0));
        let mut ry = y + pad + theme.pt(14.0);
        let r_row_h = theme.pt(30.0);

        for (idx, (label, _)) in self.right_items.iter().enumerate() {
            let is_hov = self.hovered_right == Some(idx);
            if is_hov {
                canvas.draw_rect(rx, ry, rw, r_row_h, 0x5538BDF8, 4);
                canvas.draw_rect_outline(rx, ry, rw, r_row_h, 0x99E0F2FE, 4);
            }
            canvas.draw_text_clipped(rx + theme.pt(10.0), ry + theme.pt(6.0), rw - theme.pt(16.0), label, 0xFFF8FAFC, theme.font_caption());
            ry += r_row_h + 2;
            if idx == 2 || idx == 5 {
                canvas.draw_line_h(rx + 6, ry, rw - 12, 0x4494A3B8);
                ry += 6;
            }
        }

        // Bottom Right "Shut down" Glass Button
        let sd_w = rw - theme.pt(12.0);
        let sd_h = theme.pt(28.0);
        let sd_y = y + h - pad - sd_h - theme.pt(6.0);
        canvas.draw_rect(rx + 4, sd_y, sd_w, sd_h, 0xCC1E3A8A, 4);
        canvas.draw_rect_outline(rx + 4, sd_y, sd_w, sd_h, 0xCCE0F2FE, 4);
        canvas.draw_text(rx + theme.pt(16.0), sd_y + theme.pt(6.0), "Shut down  ▸", 0xFFFFFFFF, theme.font_caption());
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> Option<String> {
        if !self.is_open { return None; }
        let theme = get_theme();
        if !self.bounds.contains(mx, my) {
            if pressed {
                self.is_open = false;
            }
            return None;
        }

        let pad = theme.pt(8.0);
        let left_w = (self.bounds.w * 58) / 100;
        let row_h = theme.pt(36.0);
        let left_x = self.bounds.x + pad;
        let top_y = self.bounds.y + pad + theme.pt(8.0);

        self.hovered_left = None;
        self.hovered_right = None;

        if mx >= left_x && mx <= left_x + left_w {
            if my >= top_y && my < self.bounds.y + self.bounds.h - theme.pt(56.0) {
                let idx = ((my - top_y) / (row_h + 2)) as usize;
                let items = self.filtered_left();
                if idx < items.len() {
                    self.hovered_left = Some(idx);
                    if pressed {
                        self.is_open = false;
                        return Some(items[idx].app_target.clone());
                    }
                }
            }
        } else {
            let r_top = self.bounds.y + pad + theme.pt(14.0);
            let r_row_h = theme.pt(30.0);
            if my >= r_top {
                let approx_idx = ((my - r_top) / (r_row_h + 3)) as usize;
                if approx_idx < self.right_items.len() {
                    self.hovered_right = Some(approx_idx);
                    if pressed {
                        self.is_open = false;
                        return Some(self.right_items[approx_idx].1.to_string());
                    }
                }
            }
        }

        None
    }
}

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

pub fn get_system_date_string() -> String {
    "10/23/2026".to_string()
}
