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
    pub right_items: Vec<(&'static str, &'static str, &'static str, &'static str)>,
    pub hovered_left: Option<usize>,
    pub hovered_right: Option<usize>,
}

impl Win7StartMenu {
    pub fn new() -> Self {
        Self {
            bounds: Rect::new(0, 560, 440, 450),
            is_open: false,
            search_query: String::new(),
            left_items: vec![
                StartMenuItem { label: "File Explorer (Finder)".into(), app_target: "Finder".into(), icon: "lib".into() },
                StartMenuItem { label: "Media Player (FFmpeg)".into(), app_target: "Video".into(), icon: "video".into() },
                StartMenuItem { label: "Terminal Shell".into(), app_target: "Terminal".into(), icon: "console".into() },
                StartMenuItem { label: "Text Editor".into(), app_target: "TextEdit".into(), icon: "document".into() },
                StartMenuItem { label: "System Monitor".into(), app_target: "Activity Monitor".into(), icon: "lighthouse".into() },
                StartMenuItem { label: "Web Browser".into(), app_target: "Browser".into(), icon: "chrome".into() },
                StartMenuItem { label: "System Settings".into(), app_target: "Settings".into(), icon: "settings".into() },
                StartMenuItem { label: "Framework Sandbox".into(), app_target: "Test".into(), icon: "tune".into() },
            ],
            right_items: vec![
                ("EOS SHARE", "Finder", "EOS SHARE", "lib"),
                ("TEST_SUBJECT", "Finder", "EOS SHARE/TEST_SUBJECT", "tune"),
                ("RootFS (EXT2)", "Finder", "RootFS", "virtual"),
                ("Home (/home/march)", "Finder", "home/march", "lib"),
                ("Initrd Ramdisk", "Finder", "Initrd", "disc"),
                ("System Settings", "Settings", "", "settings"),
            ],
            hovered_left: None,
            hovered_right: None,
        }
    }

    pub fn layout_above_taskbar(&mut self, screen_h: i32, taskbar_h: i32) {
        let theme = get_theme();
        let w = theme.pt(440.0);
        let h = theme.pt(450.0);
        self.bounds = Rect::new(theme.pt(6.0), (screen_h - taskbar_h - h - theme.pt(6.0)).max(0), w, h);
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
            let ic_sz = theme.pt(20.0);
            crate::framework::icons::draw_app_icon(canvas, &item.app_target, x + pad + theme.pt(12.0), iy + theme.pt(8.0), ic_sz, ic_sz);
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

        // Right Glass Places Column (100% Real Paths & Apps + Custom icon.png Header)
        let rx = x + pad + left_w + theme.pt(8.0);
        let rw = w - (left_w + pad * 2 + theme.pt(8.0));

        let hdr_h = theme.pt(44.0);
        canvas.draw_rect(rx, y + pad, rw, hdr_h, 0x440F172A, theme.r_ctrl());
        let logo_sz = theme.pt(28.0);
        crate::framework::icons::draw_eos_logo(canvas, rx + theme.pt(8.0), y + pad + (hdr_h - logo_sz) / 2, logo_sz, logo_sz);
        canvas.draw_text(rx + theme.pt(44.0), y + pad + theme.pt(7.0), "EOS Desktop", 0xFFFFFFFF, theme.font_body());
        canvas.draw_text(rx + theme.pt(44.0), y + pad + theme.pt(24.0), "Isolated SMP", 0xFF38BDF8, theme.font_caption());

        let mut ry = y + pad + hdr_h + theme.pt(10.0);
        let r_row_h = theme.pt(32.0);
        let r_ic = theme.pt(16.0);

        for (idx, (label, _, _, ic)) in self.right_items.iter().enumerate() {
            let is_hov = self.hovered_right == Some(idx);
            if is_hov {
                canvas.draw_rect(rx, ry, rw, r_row_h, 0x6638BDF8, theme.r_ctrl());
                canvas.draw_rect_outline(rx, ry, rw, r_row_h, 0x99E0F2FE, theme.r_ctrl());
            }
            crate::framework::icons::draw_svg_icon(canvas, ic, rx + theme.pt(8.0), ry + (r_row_h - r_ic) / 2, r_ic, r_ic);
            canvas.draw_text_clipped(rx + theme.pt(30.0), ry + theme.pt(7.0), rw - theme.pt(36.0), label, 0xFFF8FAFC, theme.font_caption());
            ry += r_row_h + 4;
            if idx == 4 {
                canvas.draw_line_h(rx + 6, ry, rw - 12, 0x4494A3B8);
                ry += 6;
            }
        }
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> Option<(String, String)> {
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
                        return Some((items[idx].app_target.clone(), String::new()));
                    }
                }
            }
        } else {
            let r_top = self.bounds.y + pad + theme.pt(54.0);
            let r_row_h = theme.pt(36.0);
            if my >= r_top {
                let approx_idx = ((my - r_top) / r_row_h) as usize;
                if approx_idx < self.right_items.len() {
                    self.hovered_right = Some(approx_idx);
                    if pressed {
                        self.is_open = false;
                        let item = self.right_items[approx_idx];
                        return Some((item.1.to_string(), item.2.to_string()));
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
            "Finder" => vec!["File".into(), "View".into(), "Go".into(), "Window".into()],
            "Video" => vec!["Media".into(), "Playback".into(), "Audio".into(), "Subtitles".into(), "Window".into()],
            "Settings" => vec!["Appearance".into(), "Wallpapers".into(), "Hardware".into()],
            "Terminal" => vec!["Shell".into(), "Edit".into(), "Window".into()],
            "Activity Monitor" => vec!["Processes".into(), "Performance".into()],
            "Browser" => vec!["Navigate".into(), "Network".into()],
            "TextEdit" => vec!["File".into(), "Edit".into(), "Find".into()],
            "Test" => vec!["GPU".into(), "Animations".into(), "Portals".into()],
            _ => vec!["File".into(), "Window".into()],
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

#[derive(Clone, Debug)]
pub struct ContextMenuItem {
    pub id: String,
    pub label: String,
    pub icon: Option<&'static str>,
    pub enabled: bool,
    pub separator_after: bool,
}

impl ContextMenuItem {
    pub fn action(id: &str, label: &str, icon: Option<&'static str>) -> Self {
        Self { id: id.into(), label: label.into(), icon, enabled: true, separator_after: false }
    }
    pub fn with_sep(mut self) -> Self {
        self.separator_after = true;
        self
    }
}

pub struct ContextMenu {
    pub bounds: Rect,
    pub context_title: Option<String>,
    pub items: Vec<ContextMenuItem>,
    pub is_visible: bool,
    pub hover_idx: Option<usize>,
    pub anim_progress: f32,
}

impl ContextMenu {
    pub fn new() -> Self {
        Self {
            bounds: Rect::new(0, 0, 220, 160),
            context_title: None,
            items: Self::desktop_default_items(),
            is_visible: false,
            hover_idx: None,
            anim_progress: 0.0,
        }
    }

    pub fn desktop_default_items() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::action("refresh", "Refresh Desktop", Some("tune")).with_sep(),
            ContextMenuItem::action("new_folder", "New Folder", Some("lib")),
            ContextMenuItem::action("open_terminal", "Open Terminal Here", Some("console")).with_sep(),
            ContextMenuItem::action("personalize", "Personalize & Theme", Some("settings")),
        ]
    }

    pub fn tick_anim(&mut self) -> bool {
        if self.is_visible && self.anim_progress < 1.0 {
            self.anim_progress = (self.anim_progress + 0.25).min(1.0);
            return true;
        }
        false
    }

    pub fn open_custom(&mut self, x: i32, y: i32, screen_w: i32, screen_h: i32, title: Option<String>, items: Vec<ContextMenuItem>) {
        if items.is_empty() { return; }
        self.context_title = title;
        self.items = items;
        let theme = get_theme();
        let w = theme.pt(230.0);
        let hdr_h = if self.context_title.is_some() { theme.pt(24.0) } else { 0 };
        let h = hdr_h + (self.items.len() as i32 * theme.pt(28.0)) + theme.pt(12.0);
        let rx = if x + w > screen_w { (screen_w - w - 4).max(0) } else { x };
        let ry = if y + h > screen_h - theme.pt(40.0) { (y - h).max(0) } else { y };
        self.bounds = Rect::new(rx, ry, w, h);
        self.is_visible = true;
        self.hover_idx = None;
        self.anim_progress = if crate::framework::features::get_features().smooth_animations { 0.15 } else { 1.0 };
    }

    pub fn open_at(&mut self, x: i32, y: i32, screen_w: i32, screen_h: i32) {
        self.open_custom(x, y, screen_w, screen_h, Some("EOS Desktop".into()), Self::desktop_default_items());
    }

    pub fn draw(&self, canvas: &mut Canvas) {
        if !self.is_visible { return; }
        let theme = get_theme();
        let x = self.bounds.x;
        let y = self.bounds.y;
        let w = self.bounds.w;
        let t = self.anim_progress.clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - t).powi(3);
        let h = ((self.bounds.h as f32) * ease).max(16.0) as i32;

        canvas.push_clip(Rect::new(x, y, w + 4, h + 4));
        canvas.draw_rect(x + 3, y + 3, w, h, 0x77000000, 6);
        canvas.draw_frosted_glass_rect(x, y, w, h, 0xF2141E30, 6);
        canvas.draw_rect_outline(x, y, w, h, theme.border_frost_highlight, 6);

        let mut cur_y = y + theme.pt(6.0);
        if let Some(ref title) = self.context_title {
            let hdr_h = theme.pt(22.0);
            canvas.draw_text_clipped(x + theme.pt(12.0), cur_y + theme.pt(2.0), w - theme.pt(24.0), title, 0xFF38BDF8, theme.font_caption());
            canvas.draw_line_h(x + 8, cur_y + hdr_h, w - 16, 0x4438BDF8);
            cur_y += hdr_h + 2;
        }

        let item_h = theme.pt(28.0);
        let ic_sz = theme.pt(15.0);
        for (i, item) in self.items.iter().enumerate() {
            let iy = cur_y + (i as i32 * item_h);
            if self.hover_idx == Some(i) && item.enabled {
                canvas.draw_rect(x + 4, iy + 1, w - 8, item_h - 2, theme.accent, 4);
            }
            if let Some(ic) = item.icon {
                crate::framework::icons::draw_svg_icon(canvas, ic, x + theme.pt(10.0), iy + (item_h - ic_sz) / 2, ic_sz, ic_sz);
            }
            let tx = x + if item.icon.is_some() { theme.pt(32.0) } else { theme.pt(12.0) };
            let col = if item.enabled { 0xFFFFFFFF } else { 0xFF64748B };
            canvas.draw_text_clipped(tx, iy + theme.pt(6.0), w - theme.pt(40.0), &item.label, col, theme.font_caption());
            if item.separator_after {
                canvas.draw_line_h(x + 8, iy + item_h - 1, w - 16, 0x4494A3B8);
            }
        }
        canvas.pop_clip();
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> Option<String> {
        if !self.is_visible { return None; }
        let theme = get_theme();
        if !self.bounds.contains(mx, my) {
            if pressed { self.is_visible = false; }
            self.hover_idx = None;
            return None;
        }
        let hdr_h = if self.context_title.is_some() { theme.pt(24.0) } else { 0 };
        let item_h = theme.pt(28.0);
        let rel_y = my - (self.bounds.y + theme.pt(6.0) + hdr_h);
        if rel_y >= 0 {
            let idx = (rel_y / item_h) as usize;
            if idx < self.items.len() {
                self.hover_idx = Some(idx);
                if pressed && self.items[idx].enabled {
                    self.is_visible = false;
                    return Some(self.items[idx].id.clone());
                }
            }
        }
        None
    }
}
