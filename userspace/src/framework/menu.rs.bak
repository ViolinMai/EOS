use super::canvas::Canvas;
use super::theme::get_theme;
use super::widget::Rect;

#[derive(Clone)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub action_id: usize,
    pub is_separator: bool,
}

#[derive(Clone)]
pub struct MenuCategory {
    pub title: String,
    pub items: Vec<MenuItem>,
}

pub struct MenuBar {
    pub categories: Vec<MenuCategory>,
    pub active_menu: Option<usize>,
    pub hovered_item: Option<usize>,
    pub bounds: Rect,
    category_rects: Vec<Rect>,
}

impl MenuBar {
    pub fn new() -> Self {
        Self {
            categories: vec![
                MenuCategory {
                    title: "".into(),
                    items: vec![
                        MenuItem { label: "About This Mac".into(), shortcut: None, action_id: 101, is_separator: false },
                        MenuItem { label: "System Settings...".into(), shortcut: Some("⌘,".into()), action_id: 102, is_separator: false },
                        MenuItem { label: "-".into(), shortcut: None, action_id: 0, is_separator: true },
                        MenuItem { label: "Restart...".into(), shortcut: None, action_id: 103, is_separator: false },
                    ],
                },
                MenuCategory {
                    title: "File".into(),
                    items: vec![
                        MenuItem { label: "New Window".into(), shortcut: Some("⌘N".into()), action_id: 201, is_separator: false },
                        MenuItem { label: "Open...".into(), shortcut: Some("⌘O".into()), action_id: 202, is_separator: false },
                        MenuItem { label: "Close Window".into(), shortcut: Some("⌘W".into()), action_id: 203, is_separator: false },
                    ],
                },
                MenuCategory {
                    title: "Edit".into(),
                    items: vec![
                        MenuItem { label: "Undo".into(), shortcut: Some("⌘Z".into()), action_id: 301, is_separator: false },
                        MenuItem { label: "Redo".into(), shortcut: Some("⇧⌘Z".into()), action_id: 302, is_separator: false },
                        MenuItem { label: "-".into(), shortcut: None, action_id: 0, is_separator: true },
                        MenuItem { label: "Cut".into(), shortcut: Some("⌘X".into()), action_id: 303, is_separator: false },
                        MenuItem { label: "Copy".into(), shortcut: Some("⌘C".into()), action_id: 304, is_separator: false },
                        MenuItem { label: "Paste".into(), shortcut: Some("⌘V".into()), action_id: 305, is_separator: false },
                    ],
                },
                MenuCategory {
                    title: "View".into(),
                    items: vec![
                        MenuItem { label: "Actual Size".into(), shortcut: Some("⌘0".into()), action_id: 401, is_separator: false },
                        MenuItem { label: "Zoom In".into(), shortcut: Some("⌘+".into()), action_id: 402, is_separator: false },
                        MenuItem { label: "Zoom Out".into(), shortcut: Some("⌘-".into()), action_id: 403, is_separator: false },
                    ],
                },
                MenuCategory {
                    title: "Window".into(),
                    items: vec![
                        MenuItem { label: "Minimize".into(), shortcut: Some("⌘M".into()), action_id: 501, is_separator: false },
                        MenuItem { label: "Zoom".into(), shortcut: None, action_id: 502, is_separator: false },
                    ],
                },
                MenuCategory {
                    title: "Help".into(),
                    items: vec![
                        MenuItem { label: "EOS Documentation".into(), shortcut: None, action_id: 601, is_separator: false },
                    ],
                },
            ],
            active_menu: None,
            hovered_item: None,
            bounds: Rect::default(),
            category_rects: Vec::new(),
        }
    }

    pub fn paint(&mut self, canvas: &mut Canvas, current_app_title: &str) {
        let theme = get_theme();
        let h = self.bounds.h;
        canvas.draw_rect(0, 0, canvas.width, h, theme.bg_menubar, 0);
        canvas.draw_line_h(0, h.saturating_sub(1), canvas.width, theme.border_menubar);

        let mut cur_x = theme.pt(16.0);
        let font_sz = theme.font_body();
        self.category_rects.clear();

        // 1. Apple Logo
        let (apple_w, _) = canvas.measure_text("", font_sz + 4);
        let apple_rect = Rect::new(cur_x, 0, apple_w + theme.pt(14.0), h);
        if self.active_menu == Some(0) {
            canvas.draw_rect(apple_rect.x, theme.pt(3.0), apple_rect.w, h.saturating_sub(theme.pt(6.0)), theme.accent, theme.pt(4.0));
            canvas.draw_text(cur_x + theme.pt(7.0), theme.pt(7.0), "", 0xFFFFFFFF, font_sz + 4);
        } else {
            canvas.draw_text(cur_x + theme.pt(7.0), theme.pt(7.0), "", theme.text_primary, font_sz + 4);
        }
        self.category_rects.push(apple_rect);
        cur_x += apple_rect.w + theme.pt(8.0);

        // 2. Active Application Name (macOS bold title style)
        let (title_w, _) = canvas.measure_text(current_app_title, font_sz + 2);
        canvas.draw_text(cur_x, theme.pt(7.0), current_app_title, theme.text_primary, font_sz + 2);
        cur_x += title_w + theme.pt(20.0);

        // 3. Regular Menu Headers
        for (idx, cat) in self.categories.iter().enumerate().skip(1) {
            let (tw, _) = canvas.measure_text(&cat.title, font_sz);
            let item_w = tw + theme.pt(16.0);
            let cat_rect = Rect::new(cur_x, 0, item_w, h);
            
            if self.active_menu == Some(idx) {
                canvas.draw_rect(cat_rect.x, theme.pt(3.0), cat_rect.w, h.saturating_sub(theme.pt(6.0)), theme.accent, theme.pt(4.0));
                canvas.draw_text(cur_x + theme.pt(8.0), theme.pt(7.0), &cat.title, 0xFFFFFFFF, font_sz);
            } else {
                canvas.draw_text(cur_x + theme.pt(8.0), theme.pt(7.0), &cat.title, theme.text_secondary, font_sz);
            }
            self.category_rects.push(cat_rect);
            cur_x += item_w + theme.pt(4.0);
        }

        // 4. System Status Items
        let status = "Oct 2 | 8 Cores | 1.5x HiDPI";
        let (sw, _) = canvas.measure_text(status, font_sz);
        canvas.draw_text(canvas.width.saturating_sub(sw + theme.pt(16.0)), theme.pt(7.0), status, theme.accent_hover, font_sz);

        // 5. Active Dropdown Menu
        if let Some(active_idx) = self.active_menu {
            if let Some(cat) = self.categories.get(active_idx) {
                let drop_w = theme.pt(220.0);
                let item_h = theme.pt(30.0);
                let drop_h = cat.items.len() * item_h + theme.pt(10.0);
                let drop_x = self.category_rects.get(active_idx).map(|r| r.x).unwrap_or(theme.pt(16.0)).min(canvas.width.saturating_sub(drop_w));
                let drop_y = h + 1;

                canvas.draw_rect(drop_x, drop_y, drop_w, drop_h, theme.bg_titlebar, theme.pt(8.0));
                canvas.draw_rect_outline(drop_x, drop_y, drop_w, drop_h, theme.border_window, theme.pt(8.0));

                let mut item_y = drop_y + theme.pt(5.0);
                for (item_idx, item) in cat.items.iter().enumerate() {
                    if item.is_separator {
                        canvas.draw_line_h(drop_x + theme.pt(10.0), item_y + (item_h / 2), drop_w.saturating_sub(theme.pt(20.0)), theme.separator);
                    } else {
                        let is_hov = self.hovered_item == Some(item_idx);
                        if is_hov {
                            canvas.draw_rect(drop_x + theme.pt(6.0), item_y, drop_w.saturating_sub(theme.pt(12.0)), item_h, theme.accent, theme.pt(4.0));
                        }
                        let text_col = if is_hov { 0xFFFFFFFF } else { theme.text_primary };
                        canvas.draw_text(drop_x + theme.pt(14.0), item_y + theme.pt(5.0), &item.label, text_col, font_sz);

                        if let Some(ref sc) = item.shortcut {
                            let (sc_w, _) = canvas.measure_text(sc, font_sz);
                            let sc_col = if is_hov { 0xFFFFFFFF } else { theme.text_muted };
                            canvas.draw_text(drop_x + drop_w.saturating_sub(sc_w + theme.pt(14.0)), item_y + theme.pt(5.0), sc, sc_col, font_sz);
                        }
                    }
                    item_y += item_h;
                }
            }
        }
    }

    pub fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> Option<usize> {
        let theme = get_theme();
        let h = self.bounds.h;

        if my <= h {
            if pressed {
                for (idx, rect) in self.category_rects.iter().enumerate() {
                    if rect.contains(mx, my) {
                        self.active_menu = if self.active_menu == Some(idx) { None } else { Some(idx) };
                        return None;
                    }
                }
            }
            return None;
        }

        if let Some(active_idx) = self.active_menu {
            if let Some(cat) = self.categories.get(active_idx) {
                let drop_w = theme.pt(220.0);
                let item_h = theme.pt(30.0);
                let drop_h = cat.items.len() * item_h + theme.pt(10.0);
                let drop_x = self.category_rects.get(active_idx).map(|r| r.x).unwrap_or(theme.pt(16.0)).min(1920 - drop_w);
                let drop_y = h + 1;

                if mx >= drop_x && mx <= drop_x + drop_w && my >= drop_y && my <= drop_y + drop_h {
                    let rel_y = my.saturating_sub(drop_y + theme.pt(5.0));
                    let clicked_item = rel_y / item_h;
                    if clicked_item < cat.items.len() {
                        self.hovered_item = Some(clicked_item);
                        if pressed {
                            let act = cat.items[clicked_item].action_id;
                            self.active_menu = None;
                            self.hovered_item = None;
                            return Some(act);
                        }
                    }
                    return None;
                }
            }
            if pressed {
                self.active_menu = None;
                self.hovered_item = None;
            }
        }
        None
    }
}
