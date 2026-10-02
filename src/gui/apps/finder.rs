use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::input::InputEvent;
use super::{App, AppAction};
use crate::gui::draw::{self, FrameBuffer};
use crate::gui::theme::Theme;
use crate::fs::FsItem;

pub static ASYNC_IMAGE_BUSY: AtomicBool = AtomicBool::new(false);
pub static ASYNC_IMAGE_DONE: AtomicBool = AtomicBool::new(false);
pub static ASYNC_IMAGE_W: AtomicUsize = AtomicUsize::new(0);
pub static ASYNC_IMAGE_H: AtomicUsize = AtomicUsize::new(0);
pub static mut ASYNC_IMAGE_NAME: String = String::new();
pub static mut ASYNC_IMAGE_PIXELS: Vec<u32> = Vec::new();

pub fn core1_image_decoder_worker() {
    unsafe {
        let name_ptr = addr_of_mut!(ASYNC_IMAGE_NAME);
        let name = (*name_ptr).clone();
        if let Ok(bytes) = crate::fs::vfs_read_bytes(&name) {
            if let Ok((pixels, w, h)) = crate::fs::image::decode_image_to_raw(&bytes) {
                let pixels_ptr = addr_of_mut!(ASYNC_IMAGE_PIXELS);
                *pixels_ptr = pixels;
                ASYNC_IMAGE_W.store(w, Ordering::Release);
                ASYNC_IMAGE_H.store(h, Ordering::Release);
                ASYNC_IMAGE_DONE.store(true, Ordering::Release);
            }
        }
        ASYNC_IMAGE_BUSY.store(false, Ordering::Release);
    }
}

pub struct FinderApp {
    pub current_folder: String,
    pub path_stack: Vec<(String, u32)>,
    pub current_cluster: u32,
    pub items: Vec<FsItem>,
    pub selected_idx: Option<usize>,
    pub status: String,
    pub last_click_tick: u64,
    pub last_clicked_idx: Option<usize>,
    pub pending_action: Option<AppAction>,
    pub sidebar_width: usize,
    pub scroll_offset: usize,
    pub needs_redraw: bool,
    pub search_query: String,
    pub search_active: bool,
}

impl FinderApp {
    pub fn new() -> Self {
        let items = crate::fs::list_directory_contents("EOS SHARE", 0);
        let initial_sel = if !items.is_empty() { Some(0) } else { None };
        Self {
            current_folder: String::from("EOS SHARE"),
            path_stack: Vec::new(),
            current_cluster: 0,
            items,
            selected_idx: initial_sel,
            status: String::from("Ready - Double-click to open"),
            last_click_tick: 0,
            last_clicked_idx: None,
            pending_action: None,
            sidebar_width: 200,
            scroll_offset: 0,
            needs_redraw: true,
            search_query: String::new(),
            search_active: false,
        }
    }

    fn open_root_location(&mut self, loc: &str) {
        self.current_folder = String::from(loc);
        self.path_stack.clear();
        self.current_cluster = 0;
        self.items = crate::fs::list_directory_contents(loc, 0);
        self.selected_idx = if !self.items.is_empty() { Some(0) } else { None };
        self.scroll_offset = 0;
        self.status = alloc::format!("Location: /{} ({} items)", loc, self.items.len());
        self.needs_redraw = true;
    }

    fn drill_into_dir(&mut self, name: &str, cluster: u32) {
        if name == ".." {
            if let Some((prev_name, prev_cluster)) = self.path_stack.pop() {
                self.current_cluster = prev_cluster;
                let full_folder = if self.path_stack.is_empty() { self.current_folder.clone() } else { alloc::format!("{}/{}", self.current_folder, prev_name) };
                self.items = crate::fs::list_directory_contents(&full_folder, self.current_cluster);
            } else { self.open_root_location(&self.current_folder.clone()); }
        } else {
            self.path_stack.push((String::from(name), self.current_cluster));
            self.current_cluster = cluster;
            let full_folder = alloc::format!("{}/{}", self.current_folder, name);
            self.items = crate::fs::list_directory_contents(&full_folder, cluster);
        }
        self.selected_idx = if !self.items.is_empty() { Some(0) } else { None };
        self.scroll_offset = 0;
        self.status = alloc::format!("Navigated: {} items", self.items.len());
        self.needs_redraw = true;
    }

    fn get_filtered_items(&self) -> Vec<FsItem> {
        if self.search_query.is_empty() { return self.items.clone(); }
        let q = self.search_query.to_ascii_lowercase();
        self.items.iter().filter(|item| {
            match item { FsItem::Directory(name, _) => name.to_ascii_lowercase().contains(&q), FsItem::File(name, _, _) => name.to_ascii_lowercase().contains(&q), }
        }).cloned().collect()
    }

    fn execute_item(&mut self, item: FsItem) {
        match item {
            FsItem::Directory(dname, cluster) => { self.drill_into_dir(&dname, cluster); }
            FsItem::File(name, size, mtype) => {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || mtype == crate::drivers::ata::MediaType::Image {
                    let mut path_prefix = self.current_folder.clone();
                    for (seg, _) in &self.path_stack { path_prefix.push('/'); path_prefix.push_str(seg); }
                    let full_path = if path_prefix == "EOS SHARE" { name.clone() } else { alloc::format!("{}/{}", path_prefix.strip_prefix("EOS SHARE/").unwrap_or(&path_prefix), name) };
                    
                    if let Ok(bytes) = crate::fs::vfs_read_bytes(&full_path) {
                        if let Ok((pixels, w, h)) = crate::fs::image::decode_image_to_raw(&bytes) {
                            self.pending_action = Some(AppAction::OpenImage { name: name.clone(), width: w, height: h, pixels });
                            self.status = alloc::format!("Opened image '{}'", name);
                            self.needs_redraw = true;
                            return;
                        }
                    }
                    self.status = alloc::format!("Failed to decode image '{}'", name);
                    self.needs_redraw = true;
                } else if lower.ends_with(".txt") || lower.ends_with(".rs") || lower.ends_with(".otf") || mtype == crate::drivers::ata::MediaType::Text {
                    if let Ok(bytes) = crate::fs::vfs_read_bytes(&name) {
                        let text_content = core::str::from_utf8(&bytes).map(String::from).unwrap_or_else(|_| bytes.iter().map(|&b| if b.is_ascii() { b as char } else { '.' }).collect());
                        self.pending_action = Some(AppAction::OpenText { name: name.clone(), content: text_content });
                    }
                } else if name.ends_with(".elf") || mtype == crate::drivers::ata::MediaType::Executable {
                    self.status = alloc::format!("Spawning process '{}'...", name);
                    self.pending_action = Some(AppAction::LaunchElf { name, arg: String::new() });
                } else {
                    self.status = alloc::format!("File '{}' ({} B)", name, size);
                    self.needs_redraw = true;
                }
            }
        }
    }

    fn create_new_file(&mut self) {
        let count = self.items.len();
        let fname = alloc::format!("NewFile_{}.txt", count + 1);
        if crate::fs::vfs_save_text_file(&fname, b"Created by EOS Finder\n").is_ok() {
            self.status = alloc::format!("Created '{}'", fname);
            self.items = crate::fs::list_directory_contents(&self.current_folder, self.current_cluster);
            self.needs_redraw = true;
        }
    }

    fn delete_selected(&mut self) {
        if let Some(idx) = self.selected_idx {
            let filtered = self.get_filtered_items();
            if idx < filtered.len() {
                if let FsItem::File(fname, _, _) = &filtered[idx] {
                    let _ = crate::fs::vfs_save_text_file(fname.as_str(), &[]);
                    self.status = alloc::format!("Deleted '{}'", fname);
                    self.items = crate::fs::list_directory_contents(&self.current_folder, self.current_cluster);
                    self.selected_idx = None;
                    self.needs_redraw = true;
                }
            }
        }
    }
}

impl App for FinderApp {
    fn wants_redraw(&self) -> bool { self.needs_redraw || ASYNC_IMAGE_DONE.load(Ordering::Relaxed) }

    fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        let sidebar_w = self.sidebar_width.min(w / 3);
        let toolbar_h = 50usize; let statusbar_h = 32usize;

        if ASYNC_IMAGE_DONE.swap(false, Ordering::AcqRel) {
            unsafe {
                let name_ptr = addr_of_mut!(ASYNC_IMAGE_NAME); let name = (*name_ptr).clone();
                let width = ASYNC_IMAGE_W.load(Ordering::Acquire); let height = ASYNC_IMAGE_H.load(Ordering::Acquire);
                let pixels_ptr = addr_of_mut!(ASYNC_IMAGE_PIXELS); let pixels = core::mem::take(&mut *pixels_ptr);
                self.pending_action = Some(AppAction::OpenImage { name, width, height, pixels });
                self.status = String::from("Image decoded");
            }
        }

        draw::draw_rect(fb, x, y, sidebar_w, h, theme.titlebar_bg);
        draw::draw_line_v(fb, x + sidebar_w, y, h, theme.separator);
        draw::draw_text(fb, x + 16, y + 14, "FAVORITES", theme.text_dim, 0);

        let locations = [("📁 EOS SHARE", "EOS SHARE"), ("📁 RootFS", "RootFS"), ("📁 Initrd", "Initrd")];
        for (i, (label, folder)) in locations.iter().enumerate() {
            let row_y = y + 42 + (i * 36);
            let is_sel = self.current_folder == *folder && self.path_stack.is_empty();
            if is_sel {
                draw::draw_rect_rounded(fb, x + 8, row_y, sidebar_w - 16, 30, 6, theme.accent, 255);
                draw::draw_text(fb, x + 16, row_y + 7, label, 0xFFFFFFFF, 1);
            } else { draw::draw_text(fb, x + 16, row_y + 7, label, theme.text_main, 1); }
        }

        let main_x = x + sidebar_w + 1;
        draw::draw_rect(fb, main_x, y, w.saturating_sub(sidebar_w), toolbar_h, theme.window_bg);
        draw::draw_line_h(fb, main_x, y + toolbar_h, w.saturating_sub(sidebar_w), theme.separator);

        let mut path_display = alloc::format!("Path: /{}", self.current_folder);
        for (seg, _) in &self.path_stack { path_display.push('/'); path_display.push_str(seg); }
        draw::draw_text_clipped(fb, main_x + 14, y + 14, w.saturating_sub(sidebar_w + 300), &path_display, theme.text_main, 1);

        let search_w = 160usize; let search_x = w.saturating_sub(search_w + 160).max(main_x + 10);
        let search_border = if self.search_active { theme.accent } else { theme.separator };
        draw::draw_rect_rounded(fb, x + search_x, y + 10, search_w, 30, 6, theme.titlebar_bg, 255);
        draw::draw_rect_outline(fb, x + search_x, y + 10, search_w, 30, search_border, 6);
        let search_text = if self.search_query.is_empty() { "Search..." } else { &self.search_query };
        draw::draw_text(fb, x + search_x + 10, y + 16, search_text, if self.search_query.is_empty() { theme.text_dim } else { theme.text_main }, 1);

        let content_y = y + toolbar_h + 1; let content_h = h.saturating_sub(toolbar_h + statusbar_h + 1);
        draw::draw_rect(fb, main_x, content_y, w.saturating_sub(sidebar_w), content_h, theme.window_bg);
        draw::draw_rect(fb, main_x, content_y, w.saturating_sub(sidebar_w), 30, theme.titlebar_bg);
        draw::draw_line_h(fb, main_x, content_y + 30, w.saturating_sub(sidebar_w), theme.separator);
        
        let col1 = main_x + 18; let col2 = main_x + w.saturating_sub(sidebar_w) / 2; let col3 = main_x + w.saturating_sub(sidebar_w) - 100;
        draw::draw_text(fb, col1, content_y + 7, "Name", theme.text_dim, 1);
        draw::draw_text(fb, col2, content_y + 7, "Size", theme.text_dim, 1);
        draw::draw_text(fb, col3, content_y + 7, "Kind", theme.text_dim, 1);

        let row_h = 36usize; let mut cur_y = content_y + 31;
        let filtered = self.get_filtered_items();

        for (i, item) in filtered.iter().enumerate().skip(self.scroll_offset) {
            if cur_y + row_h > content_y + content_h { break; }
            let is_selected = self.selected_idx == Some(i);
            if is_selected { draw::draw_rect(fb, main_x, cur_y, w.saturating_sub(sidebar_w), row_h, theme.accent); }
            let (text_col, sub_col) = if is_selected { (0xFFFFFFFF, 0xFFE0F2FE) } else { (theme.text_main, theme.text_dim) };

            match item {
                FsItem::Directory(dname, _) => {
                    draw::draw_text_clipped(fb, col1, cur_y + 10, col2 - col1 - 10, &alloc::format!("📁 {}", dname), text_col, 1);
                    draw::draw_text(fb, col2, cur_y + 10, "--", sub_col, 1);
                    draw::draw_text(fb, col3, cur_y + 10, "Folder", sub_col, 1);
                }
                FsItem::File(fname, sz, mtype) => {
                    let prefix = if fname.ends_with(".elf") { "⚙️ " } else if fname.ends_with(".png") || fname.ends_with(".jpg") { "🖼 " } else { "📄 " };
                    draw::draw_text_clipped(fb, col1, cur_y + 10, col2 - col1 - 10, &alloc::format!("{}{}", prefix, fname), text_col, 1);
                    draw::draw_text(fb, col2, cur_y + 10, &alloc::format!("{} B", sz), sub_col, 1);
                    let type_str = match mtype { crate::drivers::ata::MediaType::Executable => "Binary", crate::drivers::ata::MediaType::Image => "Image", _ => "File" };
                    draw::draw_text(fb, col3, cur_y + 10, type_str, sub_col, 1);
                }
            }
            cur_y += row_h;
        }

        let status_y = y + h - statusbar_h;
        draw::draw_rect(fb, main_x, status_y, w.saturating_sub(sidebar_w), statusbar_h, theme.titlebar_bg);
        draw::draw_line_h(fb, main_x, status_y, w.saturating_sub(sidebar_w), theme.separator);
        draw::draw_text_clipped(fb, main_x + 16, status_y + 8, w.saturating_sub(sidebar_w + 32), &self.status, theme.text_dim, 1);
        self.needs_redraw = false;
    }

    fn on_event(&mut self, event: &InputEvent, mx: isize, my: isize) {
        let sidebar_w = self.sidebar_width as isize;
        let toolbar_h = 50isize;

        match event {
            InputEvent::MouseButton { button: 0, pressed: true } => {
                if mx < sidebar_w {
                    match (my - 42) / 36 { 0 => self.open_root_location("EOS SHARE"), 1 => self.open_root_location("RootFS"), 2 => self.open_root_location("Initrd"), _ => {} } return;
                }
                if my <= toolbar_h {
                    self.needs_redraw = true; return;
                }

                let row_h = 36isize; let rel_y = my - (toolbar_h + 31);
                if rel_y >= 0 {
                    let clicked_idx = self.scroll_offset + (rel_y / row_h) as usize;
                    let filtered = self.get_filtered_items();
                    if clicked_idx < filtered.len() {
                        let now = crate::arch::x86_64::pit::get_ticks();
                        if self.last_clicked_idx == Some(clicked_idx) && now.saturating_sub(self.last_click_tick) < 400 {
                            let item_to_exec = filtered[clicked_idx].clone();
                            self.execute_item(item_to_exec);
                            self.last_clicked_idx = None;
                            self.last_click_tick = 0;
                        } else {
                            self.selected_idx = Some(clicked_idx);
                            self.last_clicked_idx = Some(clicked_idx);
                            self.last_click_tick = now;
                            self.needs_redraw = true;
                        }
                    }
                }
            }
            InputEvent::Char(c) => {
                if self.search_active {
                    if *c == '\x08' { self.search_query.pop(); } else if *c >= ' ' && *c <= '~' { self.search_query.push(*c); }
                    self.scroll_offset = 0; self.selected_idx = Some(0); self.needs_redraw = true;
                }
            }
            InputEvent::KeyDown { keycode, .. } => {
                let filtered = self.get_filtered_items();
                match keycode {
                    0x48 => if let Some(idx) = self.selected_idx { if idx > 0 { self.selected_idx = Some(idx - 1); self.needs_redraw = true; } },
                    0x50 => if let Some(idx) = self.selected_idx { if idx + 1 < filtered.len() { self.selected_idx = Some(idx + 1); self.needs_redraw = true; } },
                    0x1C => if let Some(idx) = self.selected_idx { if idx < filtered.len() { self.execute_item(filtered[idx].clone()); } },
                    0x0E if !self.search_active => self.drill_into_dir("..", 0),
                    _ => {}
                }
            }
            InputEvent::Scroll { dy } => {
                if *dy < 0 { self.scroll_offset = self.scroll_offset.saturating_add(1); } else { self.scroll_offset = self.scroll_offset.saturating_sub(1); }
                self.needs_redraw = true;
            }
            _ => {}
        }
    }
    fn on_resize(&mut self, new_w: usize, _new_h: usize) { self.sidebar_width = 200.min(new_w / 4).max(140); self.needs_redraw = true; }
    fn title(&self) -> &str { "Finder" }
    fn icon(&self) -> &'static str { "FND" }
    fn poll_action(&mut self) -> Option<AppAction> { self.pending_action.take() }
}
