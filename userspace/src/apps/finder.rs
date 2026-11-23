use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;
use crate::framework::*;
use crate::sdk;

static THUMB_CACHE: Mutex<Option<HashMap<String, ImageWidget>>> = Mutex::new(None);

#[derive(Clone)]
struct FinderItem {
    name: String,
    is_dir: bool,
    size: usize,
    thumb: Option<ImageWidget>,
}

pub struct FinderApp {
    bounds: Rect,
    pub current_dir: String,
    history: Vec<String>,
    items: Vec<FinderItem>,
    selected_idx: Option<usize>,
    grid_mode: bool,
    search_query: String,
    search_focused: bool,
    scrollbar: SmoothScrollBar,
    scroll_y: i32,
    ctx_menu: crate::framework::menu::ContextMenu,
    pub pending_open_text: Option<String>,
    pub pending_open_image: Option<(String, String)>,
}

impl FinderApp {
    pub fn new() -> Self {
        let mut init_dir = "EOS SHARE".to_string();
        if let Ok(nav) = fs::read_to_string("RootFS/finder_nav.ini") {
            let trimmed = nav.trim();
            if !trimmed.is_empty() {
                init_dir = trimmed.to_string();
            }
            let _ = fs::remove_file("RootFS/finder_nav.ini");
        }
        let mut app = Self {
            bounds: Rect::default(),
            current_dir: init_dir,
            history: Vec::new(),
            items: Vec::new(),
            selected_idx: None,
            grid_mode: true,
            search_query: String::new(),
            search_focused: false,
            scrollbar: SmoothScrollBar::new(),
            scroll_y: 0,
            ctx_menu: crate::framework::menu::ContextMenu::new(),
            pending_open_text: None,
            pending_open_image: None,
        };
        app.refresh();
        app
    }

    pub fn navigate_to(&mut self, path: &str) {
        if !self.current_dir.is_empty() && self.current_dir != path {
            self.history.push(self.current_dir.clone());
        }
        self.current_dir = path.trim_matches('/').to_string();
        self.refresh();
    }

    pub fn navigate_up(&mut self) {
        let clean = self.current_dir.trim_matches('/').to_string();
        if let Some(pos) = clean.rfind('/') {
            let parent = clean[..pos].to_string();
            self.navigate_to(&parent);
        } else if !clean.is_empty() && clean != "." {
            self.navigate_to(".");
        }
    }

    pub fn refresh(&mut self) {
        self.items.clear();
        self.selected_idx = None;
        self.scroll_y = 0;
        self.scrollbar.value = 0.0;
        self.scrollbar.target = 0.0;

        let clean = self.current_dir.trim().trim_matches('/').to_string();
        let candidates = if clean.is_empty() || clean == "." {
            vec![".".to_string(), "/".to_string()]
        } else {
            vec![clean.clone(), format!("/{}", clean)]
        };

        for cand in &candidates {
            if let Ok(rd) = fs::read_dir(cand) {
                let mut found = false;
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name == "." || name == ".." || name.eq_ignore_ascii_case("QEMUVFAT") {
                        continue;
                    }
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let full = format!("{}/{}", cand.trim_end_matches('/'), name);
                    let size = if is_dir { 0 } else { fs::metadata(&full).map(|m| m.len() as usize).unwrap_or(0) };

                    let lower = name.to_ascii_lowercase();
                    let mut thumb = None;
                    if !is_dir && size > 0 && size <= 8 * 1024 * 1024
                        && (lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg"))
                    {
                        let cache_key = format!("{}:{}", full, size);
                        let mut lock = THUMB_CACHE.lock().unwrap();
                        let map = lock.get_or_insert_with(HashMap::new);
                        if let Some(cached) = map.get(&cache_key) {
                            thumb = Some(cached.clone());
                        } else if let Ok(bytes) = fs::read(&full) {
                            if let Ok((px, w, h, _)) = sdk::media::decode_image(&bytes) {
                                let scale = (140.0 / (w.max(h) as f32)).min(1.0);
                                let tw = ((w as f32 * scale).round() as usize).max(16);
                                let th = ((h as f32 * scale).round() as usize).max(16);
                                let mut tpx = vec![0xFF0F172A; tw * th];
                                for ty in 0..th {
                                    let sy = (ty * h) / th.max(1);
                                    for tx in 0..tw {
                                        let sx = (tx * w) / tw.max(1);
                                        tpx[ty * tw + tx] = px[sy.min(h - 1) * w + sx.min(w - 1)];
                                    }
                                }
                                let iw = ImageWidget::new_cover(tpx, tw, th);
                                map.insert(cache_key, iw.clone());
                                thumb = Some(iw);
                            }
                        }
                    }

                    self.items.push(FinderItem { name, is_dir, size, thumb });
                    found = true;
                }
                if found { break; }
            }
        }

        if self.items.is_empty() && (clean.is_empty() || clean == ".") {
            for root in ["EOS SHARE", "RootFS", "Initrd"] {
                self.items.push(FinderItem { name: root.into(), is_dir: true, size: 0, thumb: None });
            }
        }

        self.items.sort_by(|a, b| {
            if a.is_dir == b.is_dir {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            } else if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });
    }

    fn filtered_indices(&self) -> Vec<usize> {
        let q = self.search_query.trim().to_ascii_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| q.is_empty() || it.name.to_ascii_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    fn activate_item(&mut self, idx: usize) {
        if idx >= self.items.len() { return; }
        let item = self.items[idx].clone();
        let clean = self.current_dir.trim_matches('/');
        let full_path = if clean.is_empty() || clean == "." {
            item.name.clone()
        } else {
            format!("{}/{}", clean, item.name)
        };

        if item.is_dir {
            self.navigate_to(&full_path);
            return;
        }

        let lower = item.name.to_ascii_lowercase();
        let _ = fs::write("RootFS/open_file.ini", full_path.as_bytes());
        if lower.ends_with(".mp4") || lower.ends_with(".mkv") || lower.ends_with(".avi") || lower.ends_with(".webm") || lower.ends_with(".mov") {
            let _ = crate::syscall::sys_spawn_elf("user_app.elf", "eos_video.elf --app Video");
        } else if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            self.pending_open_image = Some((item.name, full_path.clone()));
            let _ = crate::syscall::sys_spawn_elf("user_app.elf", "eos_preview.elf --app Preview");
        } else {
            self.pending_open_text = Some(full_path.clone());
            let _ = crate::syscall::sys_spawn_elf("user_app.elf", "eos_textedit.elf --app TextEdit");
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
        let x = self.bounds.x;
        let y = self.bounds.y;
        let w = self.bounds.w;
        let h = self.bounds.h;
        let pad = theme.pt(10.0);

        canvas.draw_rect(x, y, w, h, theme.bg_app, 0);

        // 1. Modern Toolbar (Back | Up | Refresh | Path Breadcrumb | Grid/List | Search)
        let tb_h = theme.pt(38.0);
        canvas.draw_rect(x, y, w, tb_h, theme.bg_sidebar, 0);
        canvas.draw_line_h(x, y + tb_h, w, theme.border_window);

        let btn_y = y + theme.pt(5.0);
        let btn_h = tb_h - theme.pt(10.0);
        let nav_btns = [("Back", theme.pt(44.0)), ("Up", theme.pt(34.0)), ("Refresh", theme.pt(56.0))];
        let mut bx = x + pad;
        for (lbl, bw) in nav_btns {
            canvas.draw_rect(bx, btn_y, bw, btn_h, theme.bg_panel, 5);
            canvas.draw_rect_outline(bx, btn_y, bw, btn_h, theme.border_window, 5);
            canvas.draw_text(bx + theme.pt(7.0), btn_y + theme.pt(5.0), lbl, theme.text_primary, theme.font_caption());
            bx += bw + theme.pt(5.0);
        }

        let mode_w = theme.pt(58.0);
        let search_w = theme.pt(150.0);
        let path_w = (x + w - pad - search_w - mode_w - theme.pt(12.0) - bx).max(100);
        canvas.draw_rect(bx, btn_y, path_w, btn_h, theme.bg_app, 5);
        canvas.draw_rect_outline(bx, btn_y, path_w, btn_h, theme.border_window, 5);
        let path_lbl = format!("/{}", self.current_dir.trim_matches('/'));
        canvas.draw_text_clipped(bx + theme.pt(10.0), btn_y + theme.pt(5.0), path_w - theme.pt(16.0), &path_lbl, theme.accent_hover, theme.font_caption());

        let mx_btn = bx + path_w + theme.pt(6.0);
        canvas.draw_rect(mx_btn, btn_y, mode_w, btn_h, theme.accent, 5);
        canvas.draw_text(mx_btn + theme.pt(12.0), btn_y + theme.pt(5.0), if self.grid_mode { "Grid" } else { "List" }, 0xFFFFFFFF, theme.font_caption());

        let sx = mx_btn + mode_w + theme.pt(6.0);
        canvas.draw_rect(sx, btn_y, search_w, btn_h, theme.bg_app, 5);
        canvas.draw_rect_outline(sx, btn_y, search_w, btn_h, if self.search_focused { theme.accent_hover } else { theme.border_window }, 5);
        let s_txt = if self.search_query.is_empty() { "Search..." } else { &self.search_query };
        let s_col = if self.search_query.is_empty() { theme.text_muted } else { theme.text_primary };
        canvas.draw_text_clipped(sx + theme.pt(8.0), btn_y + theme.pt(5.0), search_w - theme.pt(14.0), s_txt, s_col, theme.font_caption());

        // 2. Left Places Sidebar
        let body_y = y + tb_h + 1;
        let status_h = theme.pt(24.0);
        let body_h = (h - tb_h - status_h - 1).max(80);
        let side_w = theme.pt(150.0);

        canvas.draw_rect(x, body_y, side_w, body_h, theme.bg_sidebar, 0);
        canvas.draw_line_v(x + side_w, body_y, body_h, theme.border_window);

        let places = [
            ("EOS SHARE", "EOS SHARE", "lib"),
            ("TEST_SUBJECT", "EOS SHARE/TEST_SUBJECT", "tune"),
            ("RootFS", "RootFS", "virtual"),
            ("Home", "home/march", "lib"),
            ("Initrd", "Initrd", "disc"),
        ];
        let p_row_h = theme.pt(30.0);
        let p_ic = theme.pt(16.0);
        for (i, (lbl, target, ic)) in places.iter().enumerate() {
            let py = body_y + theme.pt(8.0) + (i as i32 * (p_row_h + 4));
            let active = self.current_dir.eq_ignore_ascii_case(target);
            if active {
                canvas.draw_rect(x + 6, py, side_w - 12, p_row_h, theme.accent, 5);
            }
            crate::framework::icons::draw_svg_icon(canvas, ic, x + theme.pt(12.0), py + (p_row_h - p_ic) / 2, p_ic, p_ic);
            let col = if active { 0xFFFFFFFF } else { theme.text_primary };
            canvas.draw_text_clipped(x + theme.pt(34.0), py + theme.pt(7.0), side_w - theme.pt(40.0), lbl, col, theme.font_caption());
        }

        // 3. Main Content Viewport (Grid with Real Thumbnails OR Detailed List)
        let main_x = x + side_w + 1;
        let main_w = (w - side_w - 1).max(160);
        let visible = self.filtered_indices();

        canvas.push_clip(Rect::new(main_x, body_y, main_w, body_h));
        if visible.is_empty() {
            canvas.draw_text(main_x + theme.pt(20.0), body_y + theme.pt(20.0), "Folder is empty.", theme.text_muted, theme.font_body());
        } else if self.grid_mode {
            let card_w = theme.pt(118.0);
            let card_h = theme.pt(108.0);
            let gap = theme.pt(10.0);
            let cols = ((main_w - pad * 2) / (card_w + gap)).max(1) as usize;

            for (vis_i, &real_idx) in visible.iter().enumerate() {
                let col = vis_i % cols;
                let row = vis_i / cols;
                let cx = main_x + pad + (col as i32 * (card_w + gap));
                let cy = body_y + pad + (row as i32 * (card_h + gap)) - self.scroll_y;
                if cy + card_h < body_y || cy > body_y + body_h { continue; }

                let item = &self.items[real_idx];
                let is_sel = self.selected_idx == Some(real_idx);
                let bg = if is_sel { 0x5538BDF8 } else { theme.bg_panel };
                canvas.draw_rect(cx, cy, card_w, card_h, bg, 6);
                canvas.draw_rect_outline(cx, cy, card_w, card_h, if is_sel { theme.accent_hover } else { theme.border_window }, 6);

                let thumb_box_w = card_w - theme.pt(16.0);
                let thumb_box_h = theme.pt(64.0);
                let tx = cx + theme.pt(8.0);
                let ty = cy + theme.pt(8.0);

                if let Some(ref th) = item.thumb {
                    let mut preview = ImageWidget::new(th.pixels.clone(), th.img_w, th.img_h);
                    preview.layout(tx, ty, thumb_box_w, thumb_box_h);
                    preview.paint(canvas);
                } else {
                    let ic_name = crate::framework::icons::icon_for_filename(&item.name, item.is_dir);
                    let ic_sz = theme.pt(40.0);
                    crate::framework::icons::draw_svg_icon(
                        canvas,
                        ic_name,
                        tx + (thumb_box_w - ic_sz) / 2,
                        ty + (thumb_box_h - ic_sz) / 2,
                        ic_sz,
                        ic_sz,
                    );
                }

                let (tw, _) = canvas.measure_text(&item.name, theme.font_caption());
                let lx = cx + ((card_w - (tw as i32).min(card_w - 8)) / 2);
                canvas.draw_text_clipped(lx, ty + thumb_box_h + theme.pt(6.0), card_w - 8, &item.name, theme.text_primary, theme.font_caption());
            }
        } else {
            let row_h = theme.pt(30.0);
            let ic_sz = theme.pt(18.0);
            for (vis_i, &real_idx) in visible.iter().enumerate() {
                let ry = body_y + theme.pt(6.0) + (vis_i as i32 * (row_h + 2)) - self.scroll_y;
                if ry + row_h < body_y || ry > body_y + body_h { continue; }
                let item = &self.items[real_idx];
                let is_sel = self.selected_idx == Some(real_idx);
                let bg = if is_sel { theme.accent } else if vis_i % 2 == 1 { theme.bg_panel } else { theme.bg_app };
                canvas.draw_rect(main_x + pad, ry, main_w - pad * 2 - theme.pt(12.0), row_h, bg, 4);

                let ic_name = crate::framework::icons::icon_for_filename(&item.name, item.is_dir);
                crate::framework::icons::draw_svg_icon(canvas, ic_name, main_x + pad + theme.pt(8.0), ry + (row_h - ic_sz) / 2, ic_sz, ic_sz);
                canvas.draw_text_clipped(main_x + pad + theme.pt(34.0), ry + theme.pt(7.0), main_w - theme.pt(150.0), &item.name, if is_sel { 0xFFFFFFFF } else { theme.text_primary }, theme.font_caption());

                let sz_str = if item.is_dir {
                    "Folder".to_string()
                } else if item.size >= 1024 * 1024 {
                    format!("{:.1} MB", item.size as f32 / (1024.0 * 1024.0))
                } else {
                    format!("{:.1} KB", item.size as f32 / 1024.0)
                };
                canvas.draw_text(main_x + main_w - theme.pt(90.0), ry + theme.pt(7.0), &sz_str, theme.text_secondary, theme.font_caption());
            }
        }
        canvas.pop_clip();

        self.scrollbar.paint(canvas, x + w - theme.pt(10.0), body_y + 4, body_h - 8);

        // 4. Bottom Status Bar
        let st_y = y + h - status_h;
        canvas.draw_rect(x, st_y, w, status_h, theme.bg_sidebar, 0);
        canvas.draw_line_h(x, st_y, w, theme.border_window);
        canvas.draw_text(x + pad, st_y + theme.pt(4.0), &format!("{} items  |  Double-click to open", visible.len()), theme.text_secondary, theme.font_caption());

        self.ctx_menu.draw(canvas);
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let pad = theme.pt(10.0);
        let tb_h = theme.pt(38.0);

        if let Some(act) = self.ctx_menu.handle_mouse(mx, my, pressed) {
            match act.as_str() {
                "open" => if let Some(idx) = self.selected_idx { self.activate_item(idx); },
                "toggle_view" => self.grid_mode = !self.grid_mode,
                "refresh" => self.refresh(),
                _ => {}
            }
            return true;
        }
        if !pressed { return true; }

        // Toolbar clicks
        if my >= self.bounds.y && my <= self.bounds.y + tb_h {
            let mut bx = self.bounds.x + pad;
            let widths = [theme.pt(44.0), theme.pt(34.0), theme.pt(56.0)];
            for (i, bw) in widths.iter().enumerate() {
                if mx >= bx && mx <= bx + *bw {
                    match i {
                        0 => if let Some(prev) = self.history.pop() { self.current_dir = prev; self.refresh(); },
                        1 => self.navigate_up(),
                        2 => self.refresh(),
                        _ => {}
                    }
                    return true;
                }
                bx += *bw + theme.pt(5.0);
            }

            let mode_w = theme.pt(58.0);
            let search_w = theme.pt(150.0);
            let sx = self.bounds.x + self.bounds.w - pad - search_w;
            let mx_btn = sx - mode_w - theme.pt(6.0);
            if mx >= mx_btn && mx <= mx_btn + mode_w {
                self.grid_mode = !self.grid_mode;
                return true;
            }
            self.search_focused = mx >= sx;
            return true;
        }
        self.search_focused = false;

        // Sidebar clicks
        let body_y = self.bounds.y + tb_h + 1;
        let side_w = theme.pt(150.0);
        if mx >= self.bounds.x && mx <= self.bounds.x + side_w && my >= body_y {
            let p_row_h = theme.pt(30.0);
            let idx = ((my - (body_y + theme.pt(8.0))) / (p_row_h + 4)) as usize;
            let targets = ["EOS SHARE", "EOS SHARE/TEST_SUBJECT", "RootFS", "home/march", "Initrd"];
            if idx < targets.len() {
                self.navigate_to(targets[idx]);
            }
            return true;
        }

        // Item selection / double-click
        let main_x = self.bounds.x + side_w + 1;
        let main_w = (self.bounds.w - side_w - 1).max(160);
        let visible = self.filtered_indices();

        if self.grid_mode {
            let card_w = theme.pt(118.0);
            let card_h = theme.pt(108.0);
            let gap = theme.pt(10.0);
            let cols = ((main_w - pad * 2) / (card_w + gap)).max(1) as usize;
            let rel_x = mx - (main_x + pad);
            let rel_y = my - (body_y + pad) + self.scroll_y;
            if rel_x >= 0 && rel_y >= 0 {
                let col = (rel_x / (card_w + gap)) as usize;
                let row = (rel_y / (card_h + gap)) as usize;
                let vis_i = row * cols + col;
                if col < cols && vis_i < visible.len() {
                    let real_idx = visible[vis_i];
                    if self.selected_idx == Some(real_idx) {
                        self.activate_item(real_idx);
                    } else {
                        self.selected_idx = Some(real_idx);
                    }
                }
            }
        } else {
            let row_h = theme.pt(30.0);
            let rel_y = my - (body_y + theme.pt(6.0)) + self.scroll_y;
            if rel_y >= 0 {
                let vis_i = (rel_y / (row_h + 2)) as usize;
                if vis_i < visible.len() {
                    let real_idx = visible[vis_i];
                    if self.selected_idx == Some(real_idx) {
                        self.activate_item(real_idx);
                    } else {
                        self.selected_idx = Some(real_idx);
                    }
                }
            }
        }
        true
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let step = theme.pt(56.0);
        let rows = if self.grid_mode { ((self.items.len() + 3) / 4) as i32 } else { self.items.len() as i32 };
        let row_h = if self.grid_mode { theme.pt(118.0) } else { theme.pt(32.0) };
        let max_s = (rows * row_h - (self.bounds.h - theme.pt(80.0))).max(0);
        // اتجاه التمرير الطبيعي: dy < 0 للأسفل، dy > 0 للأعلى
        if dy < 0 {
            self.scroll_y = (self.scroll_y + step).min(max_s);
        } else if dy > 0 {
            self.scroll_y = (self.scroll_y - step).max(0);
        }
        self.scrollbar.target = if max_s > 0 { self.scroll_y as f32 / max_s as f32 } else { 0.0 };
        self.scrollbar.value = self.scrollbar.target;
        true
    }

    fn handle_char(&mut self, c: char) -> bool {
        if !self.search_focused { return false; }
        if c == '\x08' {
            self.search_query.pop();
            return true;
        } else if c >= ' ' && c <= '~' {
            self.search_query.push(c);
            return true;
        }
        false
    }
}
