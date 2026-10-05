use crate::framework::*;
use std::fs;

#[derive(Clone)]
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
    pub root_layout: ContainerWidget,
    pub scroll_view: ScrollViewWidget,
    pub status_label: String,
}

impl FinderApp {
    pub fn new() -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            current_path: String::from("/EOS SHARE"),
            entries: Vec::new(),
            selected_idx: None,
            root_layout: ContainerWidget::vbox().with_spacing(4).with_padding(6),
            scroll_view: ScrollViewWidget::new(Box::new(ContainerWidget::vbox().with_spacing(2).with_padding(4)), 0),
            status_label: String::from("Ready"),
        };
        app.load_directory("/EOS SHARE");
        app
    }

    pub fn load_directory(&mut self, path: &str) {
        self.current_path = path.to_string();
        self.entries.clear();
        self.selected_idx = None;

        let candidate_paths = [
            path.to_string(),
            path.trim_start_matches('/').to_string(),
            format!("/{}", path.trim_start_matches('/')),
            String::from("EOS SHARE"),
            String::from("/EOS SHARE"),
        ];

        let mut read_success = false;
        for cpath in &candidate_paths {
            if let Ok(read_dir) = fs::read_dir(cpath) {
                for entry in read_dir.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let size = entry.metadata().map(|m| m.len() as usize).unwrap_or(0);
                    self.entries.push(FinderEntry { name, is_dir, size });
                }
                read_success = true;
                break;
            }
        }

        if !read_success || self.entries.is_empty() {
            if let Ok(entries) = fs::read_dir(".") {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let size = entry.metadata().map(|m| m.len() as usize).unwrap_or(0);
                    self.entries.push(FinderEntry { name, is_dir, size });
                }
            }
        }

        self.status_label = format!("{} items in {}", self.entries.len(), self.current_path);
        self.rebuild_ui();
    }

    fn rebuild_ui(&mut self) {
        let mut list_box = ContainerWidget::vbox().with_spacing(2).with_padding(2);

        for entry in &self.entries {
            let is_dir = entry.is_dir;
            let entry_name = entry.name.clone();
            let label_text = if is_dir {
                format!("📁 {}", entry_name)
            } else {
                format!("📄 {} ({} B)", entry_name, entry.size)
            };

            let mut row = ContainerWidget::hbox().with_spacing(6).with_padding(4);
            row.add_child(Box::new(LabelWidget::new(label_text)), 1, (160, 24));
            list_box.add_child(Box::new(row), 0, (200, 32));
        }

        let content_h = (self.entries.len() as i32) * 34;
        self.scroll_view = ScrollViewWidget::new(Box::new(list_box), content_h);
    }
}

impl Widget for FinderApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        let list_h = (h - 76).max(0);
        self.scroll_view.layout(x + 8, y + 42, (w - 16).max(0), list_h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_window, 0);

        let tb_h = theme.pt(36.0) as i32;
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);

        let path_text = format!("Path: {}", self.current_path);
        canvas.draw_text(self.bounds.x + (theme.pt(14.0) as i32), self.bounds.y + (theme.pt(9.0) as i32), &path_text, theme.text_primary, theme.font_body());

        self.scroll_view.paint(canvas);

        let sb_h = theme.pt(24.0) as i32;
        let sb_y = self.bounds.y + self.bounds.h - sb_h;
        canvas.draw_rect(self.bounds.x, sb_y, self.bounds.w, sb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, sb_y, self.bounds.w, theme.border_window);
        canvas.draw_text(self.bounds.x + (theme.pt(14.0) as i32), sb_y + (theme.pt(4.0) as i32), &self.status_label, theme.text_muted, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if pressed && my >= self.bounds.y + 42 && my <= self.bounds.y + self.bounds.h - 24 {
            let rel_y = (my - (self.bounds.y + 42)) + self.scroll_view.scroll_y;
            let clicked_idx = (rel_y / 34) as usize;
            if clicked_idx < self.entries.len() {
                self.selected_idx = Some(clicked_idx);
                let entry = self.entries[clicked_idx].clone();
                if entry.is_dir {
                    let next_path = format!("{}/{}", self.current_path.trim_end_matches('/'), entry.name);
                    self.load_directory(&next_path);
                }
                return true;
            }
        }
        self.scroll_view.handle_mouse(mx, my, pressed)
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        self.scroll_view.handle_scroll(mx, my, dy)
    }
}
