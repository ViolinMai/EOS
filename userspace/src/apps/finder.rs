use std::fs;
use crate::framework::*;
use crate::f_info;

#[derive(Clone)]
pub struct FileEntry {
    pub name: String,
    pub size: usize,
    pub is_dir: bool,
    pub kind_str: String,
}

pub struct FinderApp {
    bounds: Rect,
    sidebar_items: Vec<&'static str>,
    selected_sidebar: usize,
    columns: Vec<(Vec<FileEntry>, Option<usize>)>,
    active_path: Vec<String>,
    last_click_tick: u64,
    last_clicked_file: Option<String>,
    pub pending_open_image: Option<(String, Vec<u8>)>,
    pub pending_open_text: Option<(String, String)>,
}

impl FinderApp {
    pub fn new() -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            sidebar_items: vec!["EOS SHARE", "RootFS", "Initrd"],
            selected_sidebar: 0,
            columns: Vec::new(),
            active_path: vec!["/EOS SHARE".into()],
            last_click_tick: 0,
            last_clicked_file: None,
            pending_open_image: None,
            pending_open_text: None,
        };
        app.load_root();
        app
    }

    fn detect_kind(name: &str, is_dir: bool) -> String {
        if is_dir { return "Folder".into(); }
        let l = name.to_ascii_lowercase();
        if l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg") {
            "Image".into()
        } else if l.ends_with(".elf") {
            "Executable".into()
        } else if l.ends_with(".otf") || l.ends_with(".ttf") {
            "Font".into()
        } else if l.ends_with(".txt") || l.ends_with(".rs") || l.ends_with(".toml") || l.ends_with(".ini") || l.ends_with(".conf") || l.ends_with(".log") {
            "Document".into()
        } else {
            "File".into()
        }
    }

    fn read_entries(path: &str) -> Vec<FileEntry> {
        let mut list = Vec::new();
        let clean_path = if path == "/" { "." } else { path.trim_start_matches('/') };

        if let Ok(entries) = fs::read_dir(path).or_else(|_| fs::read_dir(clean_path)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "." || name == ".." { continue; }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let full_subpath = format!("{}/{}", path.trim_end_matches('/'), name);
                let size = fs::metadata(&full_subpath).or_else(|_| entry.metadata()).map(|m| m.len() as usize).unwrap_or(0);
                let kind_str = Self::detect_kind(&name, is_dir);
                list.push(FileEntry { name, size, is_dir, kind_str });
            }
        }

        list.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        list
    }

    fn load_root(&mut self) {
        self.columns.clear();
        let target = match self.selected_sidebar {
            0 => "/EOS SHARE",
            1 => "/RootFS",
            2 => "/Initrd",
            _ => "/EOS SHARE",
        };
        self.active_path = vec![target.to_string()];
        let root_files = Self::read_entries(target);
        self.columns.push((root_files, None));
    }
}

impl Widget for FinderApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let tb_h = theme.pt(40.0);
        let sb_w = theme.pt(160.0);
        let col_w = theme.pt(220.0);
        let row_h = theme.pt(30.0);

        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);
        let current_full_path = self.active_path.join("/");
        canvas.draw_text_clipped(self.bounds.x + theme.pt(18.0), self.bounds.y + theme.pt(10.0), self.bounds.w.saturating_sub(theme.pt(40.0)), &format!("📁 {}", current_full_path), theme.text_primary, theme.font_body());

        let content_y = self.bounds.y + tb_h + 1;
        let content_h = self.bounds.h.saturating_sub(tb_h + 1);
        canvas.draw_rect(self.bounds.x, content_y, sb_w, content_h, theme.bg_window, 0);
        canvas.draw_line_v(self.bounds.x + sb_w, content_y, content_h, theme.border_window);

        canvas.draw_text(self.bounds.x + theme.pt(16.0), content_y + theme.pt(14.0), "FAVORITES", theme.text_muted, theme.font_caption());
        for (i, item) in self.sidebar_items.iter().enumerate() {
            let ry = content_y + theme.pt(38.0) + (i * theme.pt(36.0));
            if self.selected_sidebar == i {
                canvas.draw_rect(self.bounds.x + theme.pt(10.0), ry, sb_w - theme.pt(20.0), theme.pt(30.0), theme.accent, theme.pt(6.0));
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ry + theme.pt(6.0), item, 0xFFFFFFFF, theme.font_body());
            } else {
                canvas.draw_text(self.bounds.x + theme.pt(20.0), ry + theme.pt(6.0), item, theme.text_secondary, theme.font_body());
            }
        }

        let mut cur_x = self.bounds.x + sb_w + 1;
        for (col_idx, (files, selected)) in self.columns.iter().enumerate() {
            if cur_x + col_w > self.bounds.x + self.bounds.w { break; }
            canvas.draw_rect(cur_x, content_y, col_w, content_h, if col_idx % 2 == 0 { 0xFF141E2E } else { theme.bg_window }, 0);
            canvas.draw_line_v(cur_x + col_w, content_y, content_h, theme.border_window);

            let mut ry = content_y + theme.pt(8.0);
            for (idx, f) in files.iter().enumerate() {
                if ry + row_h > content_y + content_h { break; }
                let is_sel = *selected == Some(idx);
                if is_sel {
                    canvas.draw_rect(cur_x + theme.pt(6.0), ry, col_w - theme.pt(12.0), row_h.saturating_sub(4), theme.accent, theme.pt(6.0));
                }
                let icon = if f.is_dir { "📁" } else if f.kind_str == "Image" { "🖼" } else if f.kind_str == "Executable" { "⚙" } else if f.kind_str == "Font" { "🔤" } else { "📄" };
                let txt_color = if is_sel { 0xFFFFFFFF } else { theme.text_primary };
                canvas.draw_text_clipped(cur_x + theme.pt(10.0), ry + theme.pt(6.0), col_w - theme.pt(32.0), &format!("{} {}", icon, f.name), txt_color, theme.font_body());
                if f.is_dir {
                    canvas.draw_text(cur_x + col_w - theme.pt(20.0), ry + theme.pt(6.0), "›", theme.text_muted, theme.font_title());
                }
                ry += row_h;
            }
            cur_x += col_w + 1;
        }

        if cur_x < self.bounds.x + self.bounds.w {
            let insp_w = (self.bounds.x + self.bounds.w).saturating_sub(cur_x);
            canvas.draw_rect(cur_x, content_y, insp_w, content_h, theme.bg_window, 0);
            if let Some((files, Some(sel))) = self.columns.last() {
                if let Some(target) = files.get(*sel) {
                    let icon_sz = theme.pt(72.0);
                    canvas.draw_rect(cur_x + theme.pt(24.0), content_y + theme.pt(24.0), icon_sz, icon_sz, theme.bg_titlebar, theme.pt(14.0));
                    canvas.draw_rect_outline(cur_x + theme.pt(24.0), content_y + theme.pt(24.0), icon_sz, icon_sz, theme.border_window, theme.pt(14.0));
                    let tag = if target.is_dir { "DIR" } else if target.kind_str == "Image" { "IMG" } else if target.kind_str == "Font" { "FNT" } else { "DOC" };
                    canvas.draw_text(cur_x + theme.pt(40.0), content_y + theme.pt(48.0), tag, theme.accent_hover, theme.font_title());
                    canvas.draw_text_clipped(cur_x + theme.pt(24.0), content_y + theme.pt(110.0), insp_w.saturating_sub(theme.pt(32.0)), &target.name, theme.text_primary, theme.font_title());
                    canvas.draw_text(cur_x + theme.pt(24.0), content_y + theme.pt(140.0), &format!("Size: {} Bytes", target.size), theme.text_secondary, theme.font_body());
                    canvas.draw_text(cur_x + theme.pt(24.0), content_y + theme.pt(168.0), &format!("Kind: {}", target.kind_str), theme.accent_hover, theme.font_body());
                }
            } else {
                canvas.draw_text(cur_x + theme.pt(24.0), content_y + theme.pt(48.0), "No item selected", theme.text_muted, theme.font_body());
            }
        }
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if !pressed { return true; }

        let theme = get_theme();
        let tb_h = theme.pt(40.0);
        let sb_w = theme.pt(160.0);
        let content_y = self.bounds.y + tb_h + 1;

        if mx < self.bounds.x + sb_w && my >= content_y {
            let idx = (my.saturating_sub(content_y + theme.pt(38.0))) / theme.pt(36.0);
            if idx < self.sidebar_items.len() {
                self.selected_sidebar = idx;
                self.load_root();
                return true;
            }
        }

        let col_w = theme.pt(220.0) + 1;
        let row_h = theme.pt(30.0);
        let mut cur_x = self.bounds.x + sb_w + 1;

        for col_idx in 0..self.columns.len() {
            if mx >= cur_x && mx < cur_x + col_w {
                let row_idx = (my.saturating_sub(content_y + theme.pt(8.0))) / row_h;
                if row_idx < self.columns[col_idx].0.len() {
                    self.columns[col_idx].1 = Some(row_idx);
                    let item = self.columns[col_idx].0[row_idx].clone();

                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                    let is_double_click = self.last_clicked_file == Some(item.name.clone()) && now.saturating_sub(self.last_click_tick) < 500;
                    self.last_click_tick = now;
                    self.last_clicked_file = Some(item.name.clone());

                    self.columns.truncate(col_idx + 1);
                    self.active_path.truncate(col_idx + 1);

                    if item.is_dir {
                        self.active_path.push(item.name.clone());
                        let next_path = self.active_path.join("/");
                        let sub_entries = Self::read_entries(&next_path);
                        self.columns.push((sub_entries, None));
                    } else if is_double_click {
                        let full_path = format!("{}/{}", self.active_path.join("/"), item.name);
                        let clean_path = full_path.trim_start_matches('/');
                        if item.kind_str == "Image" {
                            if let Ok(bytes) = fs::read(&full_path).or_else(|_| fs::read(clean_path)) {
                                f_info!("FINDER", "Opening image '{}' ({} bytes)", item.name, bytes.len());
                                self.pending_open_image = Some((item.name.clone(), bytes));
                            }
                        } else if item.kind_str == "Document" {
                            if let Ok(bytes) = fs::read(&full_path).or_else(|_| fs::read(clean_path)) {
                                let max_preview = 64 * 1024;
                                let slice = if bytes.len() > max_preview { &bytes[..max_preview] } else { &bytes[..] };
                                let text_content = String::from_utf8_lossy(slice).to_string();
                                f_info!("FINDER", "Opening document '{}' ({} chars)", item.name, text_content.len());
                                self.pending_open_text = Some((item.name.clone(), text_content));
                            }
                        }
                    }
                    return true;
                }
            }
            cur_x += col_w;
        }
        true
    }
}
