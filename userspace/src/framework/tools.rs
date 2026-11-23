use crate::framework::canvas::Canvas;
use crate::framework::widget::Rect;
use crate::framework::theme::get_theme;
use std::fs;

pub struct ColorPickerTool {
    pub bounds: Rect,
    pub selected_color: u32,
    pub active_hue: f32,
    is_dragging_hue: bool,
    is_dragging_sv: bool,
}

impl ColorPickerTool {
    pub fn new(x: i32, y: i32, w: i32, h: i32, initial_color: u32) -> Self {
        Self {
            bounds: Rect::new(x, y, w, h),
            selected_color: initial_color,
            active_hue: 210.0,
            is_dragging_hue: false,
            is_dragging_sv: false,
        }
    }

    pub fn draw(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let pad = theme.pt(8.0);
        let bar_h = theme.pt(16.0);
        let matrix_h = self.bounds.h - bar_h - (pad * 3);
        let matrix_w = self.bounds.w - (pad * 2);

        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF18181B, theme.pt(8.0) as usize);
        canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.border_window, theme.pt(8.0) as usize);

        let sv_x = self.bounds.x + pad;
        let sv_y = self.bounds.y + pad;
        let step_x = (matrix_w / 20).max(1);
        let step_y = (matrix_h / 12).max(1);

        for sy in 0..12 {
            for sx in 0..20 {
                let col = hsv_to_rgb(self.active_hue, (sx as f32) / 20.0, 1.0 - ((sy as f32) / 12.0));
                canvas.draw_rect(sv_x + (sx * step_x), sv_y + (sy * step_y), step_x, step_y, col, 0);
            }
        }

        let bar_y = sv_y + matrix_h + pad;
        let hue_step = (matrix_w / 24).max(1);
        for i in 0..24 {
            let h_val = (i as f32 / 24.0) * 360.0;
            let col = hsv_to_rgb(h_val, 1.0, 1.0);
            canvas.draw_rect(sv_x + (i * hue_step), bar_y, hue_step, bar_h, col, 0);
        }
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) {
            if !pressed {
                self.is_dragging_hue = false;
                self.is_dragging_sv = false;
            }
            return false;
        }

        let theme = get_theme();
        let pad = theme.pt(8.0);
        let bar_h = theme.pt(16.0);
        let matrix_h = self.bounds.h - bar_h - (pad * 3);
        let matrix_w = self.bounds.w - (pad * 2);

        let sv_x = self.bounds.x + pad;
        let sv_y = self.bounds.y + pad;
        let bar_y = sv_y + matrix_h + pad;

        if pressed {
            if mx >= sv_x && mx <= sv_x + matrix_w && my >= bar_y && my <= bar_y + bar_h {
                self.is_dragging_hue = true;
            } else if mx >= sv_x && mx <= sv_x + matrix_w && my >= sv_y && my <= sv_y + matrix_h {
                self.is_dragging_sv = true;
            }
        } else {
            self.is_dragging_hue = false;
            self.is_dragging_sv = false;
        }

        if self.is_dragging_hue {
            let rel_x = (mx - sv_x).clamp(0, matrix_w) as f32;
            self.active_hue = (rel_x / matrix_w as f32) * 360.0;
            self.selected_color = hsv_to_rgb(self.active_hue, 1.0, 1.0);
            return true;
        }

        if self.is_dragging_sv {
            let s = (mx - sv_x).clamp(0, matrix_w) as f32 / matrix_w as f32;
            let v = 1.0 - ((my - sv_y).clamp(0, matrix_h) as f32 / matrix_h as f32);
            self.selected_color = hsv_to_rgb(self.active_hue, s, v);
            return true;
        }

        false
    }
}

#[derive(Clone, Debug)]
pub struct FileDialogItem {
    pub name: String,
    pub is_dir: bool,
    pub size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileFilterMode {
    AllowedOnly,
    AllFiles,
}

pub struct FileDialog {
    pub bounds: Rect,
    pub is_save_mode: bool,
    pub current_dir: String,
    pub selected_idx: Option<usize>,
    pub input_filename: String,
    pub is_open: bool,
    pub entries: Vec<FileDialogItem>,
    pub scroll_y: i32,
    pub filter_mode: FileFilterMode,
    pub allowed_exts: Vec<String>,
    pub item_context_menu: crate::framework::menu::ContextMenu,
}

impl FileDialog {
    pub fn new_with_filter(is_save_mode: bool, exts_spec: &str) -> Self {
        let allowed_exts: Vec<String> = exts_spec
            .split(',')
            .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|s| !s.is_empty() && s != "*" && s != "all")
            .collect();
        let filter_mode = if allowed_exts.is_empty() {
            FileFilterMode::AllFiles
        } else {
            FileFilterMode::AllowedOnly
        };
        let mut d = Self {
            bounds: Rect::new(400, 220, 880, 560),
            is_save_mode,
            current_dir: String::from("EOS SHARE"),
            selected_idx: None,
            input_filename: if is_save_mode { String::from("document.txt") } else { String::new() },
            is_open: true,
            entries: Vec::new(),
            scroll_y: 0,
            filter_mode,
            allowed_exts,
            item_context_menu: crate::framework::menu::ContextMenu::new(),
        };
        d.refresh_entries();
        d
    }

    pub fn new(is_save_mode: bool) -> Self {
        Self::new_with_filter(is_save_mode, "")
    }

    pub fn refresh_entries(&mut self) {
        self.entries.clear();
        self.selected_idx = None;
        self.scroll_y = 0;

        let clean = self.current_dir.trim().trim_matches('/').to_string();
        let candidates = if clean.is_empty() || clean == "." {
            vec![".".to_string(), "/".to_string()]
        } else {
            vec![clean.clone(), format!("/{}", clean)]
        };

        for cand in &candidates {
            if let Ok(rd) = fs::read_dir(cand) {
                let mut found = false;
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name == "." || name == ".." || name.eq_ignore_ascii_case("QEMUVFAT") {
                        continue;
                    }
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    if !is_dir && !self.matches_filter(&name) {
                        continue;
                    }
                    let full = format!("{}/{}", cand.trim_end_matches('/'), name);
                    let size = if is_dir { 0 } else { fs::metadata(&full).map(|m| m.len() as usize).unwrap_or(0) };
                    self.entries.push(FileDialogItem { name, is_dir, size });
                    found = true;
                }
                if found { break; }
            }
        }

        if self.entries.is_empty() && (clean.is_empty() || clean == ".") {
            self.entries.push(FileDialogItem { name: "EOS SHARE".into(), is_dir: true, size: 0 });
            self.entries.push(FileDialogItem { name: "RootFS".into(), is_dir: true, size: 0 });
            self.entries.push(FileDialogItem { name: "Initrd".into(), is_dir: true, size: 0 });
        }

        self.entries.sort_by(|a, b| {
            if a.is_dir == b.is_dir {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            } else if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });
    }

    fn matches_filter(&self, name: &str) -> bool {
        if self.filter_mode == FileFilterMode::AllFiles || self.allowed_exts.is_empty() {
            return true;
        }
        let lower = name.to_ascii_lowercase();
        self.allowed_exts.iter().any(|ext| lower.ends_with(&format!(".{}", ext)))
    }

    pub fn navigate_up(&mut self) {
        let clean = self.current_dir.trim_matches('/');
        if let Some(pos) = clean.rfind('/') {
            self.current_dir = clean[..pos].to_string();
        } else if !clean.is_empty() && clean != "." {
            self.current_dir = ".".to_string();
        }
        self.refresh_entries();
    }

    pub fn draw(&self, canvas: &mut Canvas) {
        if !self.is_open { return; }
        let theme = get_theme();
        let x = self.bounds.x;
        let y = self.bounds.y;
        let w = self.bounds.w;
        let h = self.bounds.h;
        let pad = theme.pt(12.0);

        // Aero Glass Modal Frame
        canvas.draw_frosted_glass_rect(x, y, w, h, 0xF0111827, theme.pt(8.0) as usize);
        canvas.draw_rect_outline(x, y, w, h, theme.border_window, theme.pt(8.0) as usize);
        canvas.draw_rect_outline(x + 1, y + 1, w - 2, h - 2, theme.border_frost_highlight, theme.pt(7.0) as usize);

        // Titlebar + Close X
        let tb_h = theme.pt(36.0);
        canvas.draw_rect(x + 1, y + 1, w - 2, tb_h, 0xFF1E293B, theme.pt(7.0) as usize);
        let title = if self.is_save_mode { "Save File  (EOS FilePicker Portal)" } else { "Open File  (EOS FilePicker Portal)" };
        canvas.draw_text(x + pad, y + theme.pt(9.0), title, 0xFFFFFFFF, theme.font_body());

        let close_w = theme.pt(32.0);
        canvas.draw_rect(x + w - close_w - 4, y + 4, close_w, tb_h - 8, 0xFFDC2626, 4);
        canvas.draw_text(x + w - close_w + theme.pt(7.0), y + theme.pt(8.0), "X", 0xFFFFFFFF, theme.font_caption());

        // Navigation + Path Bar + Filter Chips
        let nav_y = y + tb_h + theme.pt(8.0);
        let nav_h = theme.pt(28.0);
        let up_w = theme.pt(44.0);
        canvas.draw_rect(x + pad, nav_y, up_w, nav_h, 0xFF334155, 4);
        canvas.draw_text(x + pad + theme.pt(12.0), nav_y + theme.pt(6.0), "Up", 0xFFFFFFFF, theme.font_caption());

        let filter_w = theme.pt(195.0);
        let path_x = x + pad + up_w + theme.pt(8.0);
        let path_w = (w - pad * 2 - up_w - filter_w - theme.pt(16.0)).max(120);
        canvas.draw_rect(path_x, nav_y, path_w, nav_h, 0xFF0F172A, 4);
        canvas.draw_rect_outline(path_x, nav_y, path_w, nav_h, 0xFF334155, 4);
        let path_disp = format!("/{}", self.current_dir.trim_matches('/'));
        canvas.draw_text_clipped(path_x + theme.pt(10.0), nav_y + theme.pt(6.0), path_w - theme.pt(16.0), &path_disp, 0xFF38BDF8, theme.font_caption());

        // Filter Chips: [Ext Filter] | [All]
        let fx0 = path_x + path_w + theme.pt(8.0);
        let chip_w = theme.pt(92.0);
        let ext_lbl = if self.allowed_exts.is_empty() {
            "Filtered".to_string()
        } else {
            format!("*.{}", self.allowed_exts.first().unwrap())
        };
        let filters = [
            (ext_lbl.as_str(), FileFilterMode::AllowedOnly),
            ("All", FileFilterMode::AllFiles),
        ];
        for (i, (lbl, mode)) in filters.iter().enumerate() {
            let cx = fx0 + (i as i32 * (chip_w + 4));
            let col = if self.filter_mode == *mode { theme.accent } else { 0xFF1E293B };
            canvas.draw_rect(cx, nav_y, chip_w, nav_h, col, 4);
            canvas.draw_text_clipped(cx + theme.pt(8.0), nav_y + theme.pt(6.0), chip_w - theme.pt(12.0), lbl, 0xFFFFFFFF, theme.font_caption());
        }

        // Body Area: Left Sidebar + Right Scrollable File List
        let body_y = nav_y + nav_h + theme.pt(8.0);
        let footer_h = theme.pt(44.0);
        let body_h = (h - (body_y - y) - footer_h - pad).max(80);
        let side_w = theme.pt(145.0);

        // Sidebar
        canvas.draw_rect(x + pad, body_y, side_w, body_h, 0xFF0F172A, 6);
        canvas.draw_rect_outline(x + pad, body_y, side_w, body_h, 0xFF1E293B, 6);
        let places = [
            ("EOS SHARE", "EOS SHARE", "lib"),
            ("TEST_SUBJECT", "EOS SHARE/TEST_SUBJECT", "tune"),
            ("RootFS", "RootFS", "virtual"),
            ("Home", "home/march", "lib"),
            ("Initrd", "Initrd", "disc"),
        ];
        let p_row_h = theme.pt(28.0);
        for (i, (lbl, pth, ic)) in places.iter().enumerate() {
            let py = body_y + theme.pt(8.0) + (i as i32 * (p_row_h + 4));
            let active = self.current_dir.eq_ignore_ascii_case(pth);
            if active {
                canvas.draw_rect(x + pad + 4, py, side_w - 8, p_row_h, theme.accent, 4);
            }
            let ic_sz = theme.pt(16.0);
            crate::framework::icons::draw_svg_icon(canvas, ic, x + pad + theme.pt(10.0), py + (p_row_h - ic_sz) / 2, ic_sz, ic_sz);
            canvas.draw_text_clipped(x + pad + theme.pt(32.0), py + theme.pt(6.0), side_w - theme.pt(38.0), lbl, 0xFFF8FAFC, theme.font_caption());
        }

        // File List Box
        let list_x = x + pad + side_w + theme.pt(8.0);
        let list_w = (w - pad * 2 - side_w - theme.pt(8.0)).max(160);
        canvas.draw_rect(list_x, body_y, list_w, body_h, 0xFF0B1120, 6);
        canvas.draw_rect_outline(list_x, body_y, list_w, body_h, 0xFF1E293B, 6);

        let row_h = theme.pt(28.0);
        let sb_w = theme.pt(8.0);
        canvas.push_clip(Rect::new(list_x + 2, body_y + 2, list_w - sb_w - 6, body_h - 4));

        if self.entries.is_empty() {
            canvas.draw_text(list_x + theme.pt(16.0), body_y + theme.pt(16.0), "No matching files in this folder.", 0xFF64748B, theme.font_caption());
        } else {
            let mut cur_y = body_y + theme.pt(4.0) - self.scroll_y;
            for (idx, item) in self.entries.iter().enumerate() {
                if cur_y + row_h >= body_y && cur_y <= body_y + body_h {
                    let is_sel = self.selected_idx == Some(idx);
                    let row_bg = if is_sel {
                        0xFF1D4ED8
                    } else if idx % 2 == 1 {
                        0xFF0F172A
                    } else {
                        0xFF0B1120
                    };
                    canvas.draw_rect(list_x + 4, cur_y, list_w - sb_w - 10, row_h, row_bg, 4);

                    let ic_name = crate::framework::icons::icon_for_filename(&item.name, item.is_dir);
                    let ic_sz = theme.pt(18.0);
                    crate::framework::icons::draw_svg_icon(
                        canvas,
                        ic_name,
                        list_x + theme.pt(10.0),
                        cur_y + (row_h - ic_sz) / 2,
                        ic_sz,
                        ic_sz,
                    );

                    let name_w = (list_w - theme.pt(130.0)).max(80);
                    canvas.draw_text_clipped(
                        list_x + theme.pt(34.0),
                        cur_y + theme.pt(6.0),
                        name_w,
                        &item.name,
                        0xFFF8FAFC,
                        theme.font_caption(),
                    );

                    let meta_str = if item.is_dir {
                        "Folder".to_string()
                    } else if item.size >= 1024 * 1024 {
                        format!("{:.1} MB", item.size as f32 / (1024.0 * 1024.0))
                    } else if item.size >= 1024 {
                        format!("{:.1} KB", item.size as f32 / 1024.0)
                    } else {
                        format!("{} B", item.size)
                    };
                    canvas.draw_text(
                        list_x + list_w - theme.pt(88.0),
                        cur_y + theme.pt(6.0),
                        &meta_str,
                        0xFF94A3B8,
                        theme.font_caption(),
                    );
                }
                cur_y += row_h;
            }
        }
        canvas.pop_clip();

        // Scrollbar Thumb
        let total_h = (self.entries.len() as i32) * row_h;
        if total_h > body_h && body_h > 0 {
            let track_x = list_x + list_w - sb_w - 3;
            let thumb_h = ((body_h as f32 / total_h as f32) * body_h as f32).max(theme.pt(24.0) as f32) as i32;
            let max_s = (total_h - body_h).max(1);
            let thumb_y = body_y + ((self.scroll_y as f32 / max_s as f32) * (body_h - thumb_h) as f32) as i32;
            canvas.draw_rect(track_x, body_y + 2, sb_w, body_h - 4, 0x331E293B, 3);
            canvas.draw_rect(track_x, thumb_y, sb_w, thumb_h, theme.accent, 3);
        }

        // Footer: Filename Input + Cancel + Open/Save Button
        let foot_y = y + h - footer_h;
        let btn_w = theme.pt(86.0);
        let btn_h = theme.pt(30.0);
        let cancel_x = x + w - pad - btn_w;
        let open_x = cancel_x - btn_w - theme.pt(8.0);
        let in_w = (open_x - (x + pad) - theme.pt(10.0)).max(100);

        canvas.draw_rect(x + pad, foot_y, in_w, btn_h, 0xFF0F172A, 4);
        canvas.draw_rect_outline(x + pad, foot_y, in_w, btn_h, theme.border_window, 4);
        let disp_in = if self.input_filename.is_empty() { "Select a file from the list..." } else { &self.input_filename };
        let in_col = if self.input_filename.is_empty() { 0xFF64748B } else { 0xFFFFFFFF };
        canvas.draw_text_clipped(x + pad + theme.pt(10.0), foot_y + theme.pt(7.0), in_w - theme.pt(16.0), disp_in, in_col, theme.font_caption());

        let act_lbl = if self.is_save_mode { "Save" } else { "Open" };
        canvas.draw_rect(open_x, foot_y, btn_w, btn_h, 0xFF059669, 4);
        canvas.draw_text(open_x + theme.pt(24.0), foot_y + theme.pt(7.0), act_lbl, 0xFFFFFFFF, theme.font_caption());

        canvas.draw_rect(cancel_x, foot_y, btn_w, btn_h, 0xFF334155, 4);
        canvas.draw_text(cancel_x + theme.pt(18.0), foot_y + theme.pt(7.0), "Cancel", 0xFFFFFFFF, theme.font_caption());

        self.item_context_menu.draw(canvas);
    }

    pub fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.is_open || !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let tb_h = theme.pt(36.0);
        let nav_h = theme.pt(28.0);
        let footer_h = theme.pt(44.0);
        let pad = theme.pt(12.0);
        let body_h = (self.bounds.h - tb_h - nav_h - footer_h - pad * 2 - theme.pt(16.0)).max(80);
        let row_h = theme.pt(28.0);
        let total_h = (self.entries.len() as i32) * row_h + theme.pt(8.0);
        let max_scroll = (total_h - body_h).max(0);
        let step = row_h * 2;
        if dy > 0 {
            self.scroll_y = (self.scroll_y + step).min(max_scroll);
        } else if dy < 0 {
            self.scroll_y = (self.scroll_y - step).max(0);
        }
        true
    }

    pub fn handle_right_click(&mut self, mx: i32, my: i32) -> bool {
        if !self.is_open || !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let pad = theme.pt(12.0);
        let tb_h = theme.pt(36.0);
        let nav_y = self.bounds.y + tb_h + theme.pt(8.0);
        let nav_h = theme.pt(28.0);
        let body_y = nav_y + nav_h + theme.pt(8.0);
        let side_w = theme.pt(145.0);
        let list_x = self.bounds.x + pad + side_w + theme.pt(8.0);

        if mx >= list_x && my >= body_y {
            let row_h = theme.pt(28.0);
            let rel_y = my - (body_y + theme.pt(4.0)) + self.scroll_y;
            if rel_y >= 0 {
                let idx = (rel_y / row_h) as usize;
                if idx < self.entries.len() {
                    self.selected_idx = Some(idx);
                    let item = &self.entries[idx];
                    if !item.is_dir {
                        self.input_filename = item.name.clone();
                    }
                    let items = if item.is_dir {
                        vec![
                            crate::framework::menu::ContextMenuItem::action("open_item", "Open Folder", Some("lib")).with_sep(),
                            crate::framework::menu::ContextMenuItem::action("refresh", "Refresh List", Some("tune")),
                        ]
                    } else {
                        vec![
                            crate::framework::menu::ContextMenuItem::action("open_item", "Select & Open File", Some("video")).with_sep(),
                            crate::framework::menu::ContextMenuItem::action("toggle_all", "Show All Files (*.*)", Some("document")),
                        ]
                    };
                    self.item_context_menu.open_custom(mx, my, self.bounds.x + self.bounds.w, self.bounds.y + self.bounds.h, Some(item.name.clone()), items);
                    return true;
                }
            }
        }
        false
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> Option<String> {
        if !self.is_open { return None; }
        if !self.bounds.contains(mx, my) {
            if pressed { self.is_open = false; }
            return None;
        }
        if let Some(act) = self.item_context_menu.handle_mouse(mx, my, pressed) {
            if act == "toggle_all" {
                self.filter_mode = FileFilterMode::AllFiles;
                self.refresh_entries();
            } else if act == "refresh" {
                self.refresh_entries();
            } else if act == "open_item" {
                if let Some(idx) = self.selected_idx {
                    if idx < self.entries.len() {
                        let item = self.entries[idx].clone();
                        if item.is_dir {
                            let clean = self.current_dir.trim_matches('/');
                            self.current_dir = if clean.is_empty() || clean == "." { item.name } else { format!("{}/{}", clean, item.name) };
                            self.refresh_entries();
                        } else {
                            self.is_open = false;
                            let clean = self.current_dir.trim_matches('/');
                            return Some(if clean.is_empty() || clean == "." { item.name } else { format!("{}/{}", clean, item.name) });
                        }
                    }
                }
            }
            return None;
        }
        if !pressed { return None; }

        let theme = get_theme();
        let x = self.bounds.x;
        let y = self.bounds.y;
        let w = self.bounds.w;
        let h = self.bounds.h;
        let pad = theme.pt(12.0);
        let tb_h = theme.pt(36.0);

        // Close X
        let close_w = theme.pt(32.0);
        if my >= y && my <= y + tb_h && mx >= x + w - close_w - 8 {
            self.is_open = false;
            return None;
        }

        // Nav bar: Up + Filter Chips
        let nav_y = y + tb_h + theme.pt(8.0);
        let nav_h = theme.pt(28.0);
        let up_w = theme.pt(44.0);
        if my >= nav_y && my <= nav_y + nav_h {
            if mx >= x + pad && mx <= x + pad + up_w {
                self.navigate_up();
                return None;
            }
            let filter_w = theme.pt(195.0);
            let fx0 = x + w - pad - filter_w;
            let chip_w = theme.pt(92.0);
            let modes = [FileFilterMode::AllowedOnly, FileFilterMode::AllFiles];
            for (i, m) in modes.iter().enumerate() {
                let cx = fx0 + (i as i32 * (chip_w + 4));
                if mx >= cx && mx <= cx + chip_w {
                    self.filter_mode = *m;
                    self.refresh_entries();
                    return None;
                }
            }
        }

        // Sidebar Quick Access
        let body_y = nav_y + nav_h + theme.pt(8.0);
        let footer_h = theme.pt(44.0);
        let body_h = (h - (body_y - y) - footer_h - pad).max(80);
        let side_w = theme.pt(145.0);

        if mx >= x + pad && mx <= x + pad + side_w && my >= body_y && my <= body_y + body_h {
            let p_row_h = theme.pt(28.0);
            let idx = ((my - (body_y + theme.pt(8.0))) / (p_row_h + 4)) as usize;
            let targets = ["EOS SHARE", "EOS SHARE/TEST_SUBJECT", "RootFS", "home/march", "Initrd"];
            if idx < targets.len() {
                self.current_dir = targets[idx].to_string();
                self.refresh_entries();
            }
            return None;
        }

        // File List Click / Scrollbar Click / Double-Click
        let list_x = x + pad + side_w + theme.pt(8.0);
        let list_w = (w - pad * 2 - side_w - theme.pt(8.0)).max(160);
        let sb_w = theme.pt(8.0);
        if mx >= list_x && my >= body_y && my <= body_y + body_h {
            let row_h = theme.pt(28.0);
            if mx >= list_x + list_w - sb_w - 8 {
                let total_h = (self.entries.len() as i32) * row_h + theme.pt(8.0);
                let max_scroll = (total_h - body_h).max(0);
                let ratio = ((my - body_y) as f32 / body_h.max(1) as f32).clamp(0.0, 1.0);
                self.scroll_y = (ratio * max_scroll as f32) as i32;
                return None;
            }
            let rel_y = my - (body_y + theme.pt(4.0)) + self.scroll_y;
            if rel_y >= 0 {
                let idx = (rel_y / row_h) as usize;
                if idx < self.entries.len() {
                    let item = self.entries[idx].clone();
                    if self.selected_idx == Some(idx) {
                        // Double click
                        if item.is_dir {
                            let clean = self.current_dir.trim_matches('/');
                            self.current_dir = if clean.is_empty() || clean == "." {
                                item.name
                            } else {
                                format!("{}/{}", clean, item.name)
                            };
                            self.refresh_entries();
                            return None;
                        } else {
                            self.is_open = false;
                            let clean = self.current_dir.trim_matches('/');
                            return Some(if clean.is_empty() || clean == "." {
                                item.name
                            } else {
                                format!("{}/{}", clean, item.name)
                            });
                        }
                    } else {
                        self.selected_idx = Some(idx);
                        if !item.is_dir {
                            self.input_filename = item.name;
                        }
                    }
                }
            }
            return None;
        }

        // Footer Buttons
        let foot_y = y + h - footer_h;
        let btn_w = theme.pt(86.0);
        let btn_h = theme.pt(30.0);
        let cancel_x = x + w - pad - btn_w;
        let open_x = cancel_x - btn_w - theme.pt(8.0);

        if my >= foot_y && my <= foot_y + btn_h {
            if mx >= cancel_x && mx <= cancel_x + btn_w {
                self.is_open = false;
                return None;
            }
            if mx >= open_x && mx <= open_x + btn_w {
                if let Some(idx) = self.selected_idx {
                    if idx < self.entries.len() && self.entries[idx].is_dir {
                        let clean = self.current_dir.trim_matches('/');
                        let dname = self.entries[idx].name.clone();
                        self.current_dir = if clean.is_empty() || clean == "." { dname } else { format!("{}/{}", clean, dname) };
                        self.refresh_entries();
                        return None;
                    }
                }
                if !self.input_filename.is_empty() {
                    self.is_open = false;
                    let clean = self.current_dir.trim_matches('/');
                    return Some(if clean.is_empty() || clean == "." {
                        self.input_filename.clone()
                    } else {
                        format!("{}/{}", clean, self.input_filename)
                    });
                }
            }
        }

        None
    }
}

pub struct ContextMenu {
    pub bounds: Rect,
    pub items: Vec<String>,
    pub is_visible: bool,
    pub target_item: Option<String>,
}

impl ContextMenu {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            items: Vec::new(),
            is_visible: false,
            target_item: None,
        }
    }

    pub fn show(&mut self, x: i32, y: i32, items: Vec<String>, target: Option<String>) {
        self.bounds = Rect::new(x, y, 160, (items.len() as i32 * 28) + 8);
        self.items = items;
        self.is_visible = true;
        self.target_item = target;
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
        self.target_item = None;
    }

    pub fn draw(&self, canvas: &mut Canvas) {
        if !self.is_visible { return; }
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF18181B, 6);
        canvas.draw_rect_outline(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.border_window, 6);

        for (i, item) in self.items.iter().enumerate() {
            let ry = self.bounds.y + 4 + (i as i32 * 28);
            canvas.draw_text(self.bounds.x + 14, ry + 6, item, 0xFFF4F4F5, theme.font_caption());
        }
    }

    pub fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> Option<String> {
        if !self.is_visible { return None; }
        if pressed {
            if self.bounds.contains(mx, my) {
                let idx = ((my - self.bounds.y - 4) / 28) as usize;
                if idx < self.items.len() {
                    let chosen = self.items[idx].clone();
                    self.hide();
                    return Some(chosen);
                }
            } else {
                self.hide();
            }
        }
        None
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let ri = ((r + m) * 255.0) as u32;
    let gi = ((g + m) * 255.0) as u32;
    let bi = ((b + m) * 255.0) as u32;
    0xFF000000 | (ri << 16) | (gi << 8) | bi
}
