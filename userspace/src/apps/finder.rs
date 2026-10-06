use crate::framework::*;
use std::fs;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct FinderEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: usize,
}

pub struct FinderApp {
    pub bounds: Rect,
    pub current_path: String,
    pub entries: Vec<FinderEntry>,
    pub selected_idx: Option<usize>,
    pub scroll_y: i32,
    pub status_label: String,
    pub pending_open_image: Option<(String, String)>,
    pub pending_open_text: Option<String>,
    pub thumb_cache: HashMap<String, (Vec<u32>, usize, usize)>,
}

impl FinderApp {
    pub fn new() -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            current_path: String::from("EOS SHARE"),
            entries: Vec::new(),
            selected_idx: None,
            scroll_y: 0,
            status_label: String::from("Ready"),
            pending_open_image: None,
            pending_open_text: None,
            thumb_cache: HashMap::new(),
        };
        app.load_directory("EOS SHARE");
        app
    }

    pub fn load_directory(&mut self, path: &str) {
        let clean = path.trim().trim_matches('/');
        let target_path = if clean.is_empty() { String::from("EOS SHARE") } else { clean.to_string() };

        self.entries.clear();
        self.selected_idx = None;
        self.scroll_y = 0;

        let mut read_success = false;
        let candidates = [
            target_path.clone(),
            format!("/{}", target_path),
            format!("{}/", target_path),
        ];

        for p in &candidates {
            if let Ok(read_dir) = fs::read_dir(p) {
                for entry in read_dir.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name == "." || name == ".." { continue; }
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let size = if is_dir {
                        0
                    } else {
                        let full_file_path = format!("{}/{}", p.trim_end_matches('/'), name);
                        fs::metadata(&full_file_path).map(|m| m.len() as usize).unwrap_or(0)
                    };
                    self.entries.push(FinderEntry { name, is_dir, size });
                }
                read_success = true;
                break;
            }
        }

        self.current_path = target_path;

        if self.current_path.contains('/') || (self.current_path != "EOS SHARE" && self.current_path != "RootFS" && self.current_path != "Initrd") {
            self.entries.insert(0, FinderEntry { name: String::from(".."), is_dir: true, size: 0 });
        }

        self.entries.sort_by(|a, b| {
            if a.name == ".." { std::cmp::Ordering::Less }
            else if b.name == ".." { std::cmp::Ordering::Greater }
            else { b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())) }
        });

        self.status_label = if read_success {
            format!("{} items in /{}", self.entries.len(), self.current_path)
        } else {
            format!("Empty or unavailable folder (/{}).", self.current_path)
        };
    }

    pub fn open_entry(&mut self, idx: usize) {
        if idx >= self.entries.len() { return; }
        let entry = self.entries[idx].clone();

        if entry.is_dir {
            if entry.name == ".." {
                if let Some(pos) = self.current_path.rfind('/') {
                    let parent = self.current_path[..pos].to_string();
                    self.load_directory(&parent);
                } else {
                    self.load_directory("EOS SHARE");
                }
            } else {
                let next_path = format!("{}/{}", self.current_path.trim_end_matches('/'), entry.name);
                self.load_directory(&next_path);
            }
        } else {
            let lower = entry.name.to_lowercase();
            let full_path = format!("{}/{}", self.current_path.trim_end_matches('/'), entry.name);
            if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                self.pending_open_image = Some((entry.name.clone(), full_path));
            } else {
                self.pending_open_text = Some(full_path);
            }
        }
    }
}

impl Widget for FinderApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_window, 0);

        let tb_h = theme.pt(40.0);
        let sb_h = theme.pt(26.0);

        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);

        let favorites = [("Root", "EOS SHARE"), ("RootFS", "RootFS"), ("Initrd", "Initrd")];
        let mut fav_x = self.bounds.x + theme.pt(12.0);
        for &(label, target) in &favorites {
            let btn_w = theme.pt(58.0);
            let is_cur = self.current_path == target || (target == "EOS SHARE" && self.current_path.starts_with("EOS SHARE/"));
            let bg_c = if is_cur { theme.accent } else { 0xFF27272A };
            canvas.draw_rect(fav_x, self.bounds.y + theme.pt(7.0), btn_w, theme.pt(26.0), bg_c, theme.pt(5.0) as usize);
            let (tw, th) = canvas.measure_text(label, theme.font_caption());
            canvas.draw_text(fav_x + (btn_w - tw as i32) / 2, self.bounds.y + theme.pt(7.0) + (theme.pt(26.0) - th as i32) / 2, label, 0xFFFFFFFF, theme.font_caption());
            fav_x += btn_w + theme.pt(8.0);
        }

        let path_text = format!("Location: /{}", self.current_path);
        canvas.draw_text_clipped(fav_x + theme.pt(8.0), self.bounds.y + theme.pt(11.0), self.bounds.w - (fav_x - self.bounds.x) - theme.pt(16.0), &path_text, theme.text_primary, theme.font_body());

        let list_y = self.bounds.y + tb_h + 1;
        let list_h = (self.bounds.h - tb_h - sb_h - 1).max(0);
        canvas.push_clip(Rect::new(self.bounds.x, list_y, self.bounds.w, list_h));

        let row_h = theme.pt(34.0);
        let mut cur_y = list_y + theme.pt(4.0) - self.scroll_y;

        for (idx, entry) in self.entries.iter().enumerate() {
            if cur_y + row_h >= list_y && cur_y <= list_y + list_h {
                let is_sel = self.selected_idx == Some(idx);
                let bg = if is_sel { theme.accent } else if idx % 2 == 0 { theme.bg_window } else { 0xFF18181B };

                canvas.draw_rect(self.bounds.x + theme.pt(4.0), cur_y, self.bounds.w - theme.pt(8.0), row_h, bg, theme.pt(4.0) as usize);

                let (tag, tag_col) = if entry.name == ".." {
                    ("[DIR]", 0xFF38BDF8)
                } else if entry.is_dir {
                    ("[DIR]", 0xFF38BDF8)
                } else {
                    let l = entry.name.to_lowercase();
                    if l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg") {
                        ("[IMG]", 0xFFA78BFA)
                    } else if l.ends_with(".elf") {
                        ("[BIN]", 0xFF34D399)
                    } else {
                        ("[TXT]", 0xFFFCD34D)
                    }
                };

                let label = if entry.is_dir {
                    format!("{} {}", tag, entry.name)
                } else {
                    let sz_str = if entry.size >= 1024 * 1024 {
                        format!("{:.2} MB", (entry.size as f64) / (1024.0 * 1024.0))
                    } else if entry.size >= 1024 {
                        format!("{} KB", entry.size / 1024)
                    } else {
                        format!("{} B", entry.size)
                    };
                    format!("{} {} ({})", tag, entry.name, sz_str)
                };

                let txt_c = if is_sel { 0xFFFFFFFF } else { theme.text_primary };
                canvas.draw_text(self.bounds.x + theme.pt(14.0), cur_y + theme.pt(8.0), tag, tag_col, theme.font_body());
                canvas.draw_text(self.bounds.x + theme.pt(54.0), cur_y + theme.pt(8.0), &label[5..], txt_c, theme.font_body());
            }
            cur_y += row_h;
        }

        canvas.pop_clip();

        let sb_y = self.bounds.y + self.bounds.h - sb_h;
        canvas.draw_rect(self.bounds.x, sb_y, self.bounds.w, sb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, sb_y, self.bounds.w, theme.border_window);
        canvas.draw_text_clipped(self.bounds.x + theme.pt(14.0), sb_y + theme.pt(5.0), self.bounds.w - theme.pt(28.0), &self.status_label, theme.text_muted, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let tb_h = theme.pt(40.0);
        let sb_h = theme.pt(26.0);

        if pressed {
            if my >= self.bounds.y && my <= self.bounds.y + tb_h {
                let favorites = ["EOS SHARE", "RootFS", "Initrd"];
                let mut fav_x = self.bounds.x + theme.pt(12.0);
                for target in favorites {
                    let btn_w = theme.pt(58.0);
                    if mx >= fav_x && mx <= fav_x + btn_w {
                        self.load_directory(target);
                        return true;
                    }
                    fav_x += btn_w + theme.pt(8.0);
                }
                return true;
            }

            let list_y = self.bounds.y + tb_h + 1;
            let list_h = self.bounds.h - tb_h - sb_h;
            if my >= list_y && my <= list_y + list_h {
                let row_h = theme.pt(34.0);
                let rel_y = (my - (list_y + theme.pt(4.0))) + self.scroll_y;
                if rel_y >= 0 {
                    let clicked_idx = (rel_y / row_h) as usize;
                    if clicked_idx < self.entries.len() {
                        if self.selected_idx == Some(clicked_idx) {
                            self.open_entry(clicked_idx);
                        } else {
                            self.selected_idx = Some(clicked_idx);
                        }
                        return true;
                    }
                }
            }
        }
        true
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let row_h = theme.pt(34.0);
        let total_h = (self.entries.len() as i32) * row_h;
        let view_h = self.bounds.h - theme.pt(66.0);
        let max_scroll = (total_h - view_h).max(0);

        if dy > 0 {
            self.scroll_y = (self.scroll_y + theme.pt(32.0)).min(max_scroll);
        } else {
            self.scroll_y = (self.scroll_y - theme.pt(32.0)).max(0);
        }
        true
    }
}
