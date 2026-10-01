use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::input::{InputEvent, NavAction};
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
        let items = crate::fs::list_directory_contents("Storage", 0);
        Self {
            current_folder: String::from("Storage"),
            path_stack: Vec::new(),
            current_cluster: 0,
            items,
            selected_idx: Some(0),
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
                let full_folder = if self.path_stack.is_empty() {
                    self.current_folder.clone()
                } else {
                    alloc::format!("{}/{}", self.current_folder, prev_name)
                };
                self.items = crate::fs::list_directory_contents(&full_folder, self.current_cluster);
            } else {
                self.open_root_location(&self.current_folder.clone());
            }
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
        if self.search_query.is_empty() {
            return self.items.clone();
        }
        let q = self.search_query.to_ascii_lowercase();
        self.items.iter().filter(|item| {
            match item {
                FsItem::Directory(name, _) => name.to_ascii_lowercase().contains(&q),
                FsItem::File(name, _, _) => name.to_ascii_lowercase().contains(&q),
            }
        }).cloned().collect()
    }

    fn execute_item(&mut self, item: FsItem) {
        match item {
            FsItem::Directory(dname, cluster) => {
                self.drill_into_dir(&dname, cluster);
            }
            FsItem::File(name, size, mtype) => {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || mtype == crate::drivers::ata::MediaType::Image {
                    if ASYNC_IMAGE_BUSY.load(Ordering::Acquire) {
                        self.status = String::from("Core 1 is decoding another asset...");
                        self.needs_redraw = true;
                        return;
                    }
                    unsafe {
                        let name_ptr = addr_of_mut!(ASYNC_IMAGE_NAME);
                        *name_ptr = name.clone();
                        ASYNC_IMAGE_DONE.store(false, Ordering::Release);
                        ASYNC_IMAGE_BUSY.store(true, Ordering::Release);
                    }
                    self.status = alloc::format!("Core 1: Decoding '{}'...", name);
                    self.needs_redraw = true;
                    let _ = crate::task::dispatch_job(1, core1_image_decoder_worker);
                } else if lower.ends_with(".txt") || lower.ends_with(".rs") || lower.ends_with(".toml") 
                     || lower.ends_with(".conf") || lower.ends_with(".log") || lower.ends_with(".md")
                     || mtype == crate::drivers::ata::MediaType::Text {
                    if let Ok(bytes) = crate::fs::vfs_read_bytes(&name) {
                        let text_content = core::str::from_utf8(&bytes).map(String::from).unwrap_or_else(|_| {
                            bytes.iter().map(|&b| if b.is_ascii() { b as char } else { '.' }).collect()
                        });
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
        let default_content = b"Created by EOS Finder\n";
        if crate::fs::vfs_save_text_file(&fname, default_content).is_ok() {
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
                    // Truncate file content to empty as instant deletion mark
                    let _ = crate::fs::vfs_save_text_file(fname, &[]);
                    self.status = alloc::format!("Deleted/Cleared '{}'", fname);
                    self.items = crate::fs::list_directory_contents(&self.current_folder, self.current_cluster);
                    self.selected_idx = None;
                    self.needs_redraw = true;
                }
            }
        }
    }
}

impl App for FinderApp {
    fn wants_redraw(&self) -> bool {
        self.needs_redraw || ASYNC_IMAGE_DONE.load(Ordering::Relaxed)
    }

    fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        let sidebar_w = self.sidebar_width.min(w / 3);
        let toolbar_h = 44usize;
        let statusbar_h = 30usize;

        if ASYNC_IMAGE_DONE.swap(false, Ordering::AcqRel) {
            unsafe {
                let name_ptr = addr_of_mut!(ASYNC_IMAGE_NAME);
                let name = (*name_ptr).clone();
                let width = ASYNC_IMAGE_W.load(Ordering::Acquire);
                let height = ASYNC_IMAGE_H.load(Ordering::Acquire);
                let pixels_ptr = addr_of_mut!(ASYNC_IMAGE_PIXELS);
                let pixels = core::mem::take(&mut *pixels_ptr);
                self.pending_action = Some(AppAction::OpenImage { name, width, height, pixels });
                self.status = String::from("Image decoded by Core 1");
            }
        }

        // 1. Sidebar
        draw::draw_rect(fb, x, y, sidebar_w, h, theme.titlebar_bg);
        draw::draw_line_v(fb, x + sidebar_w, y, h, theme.separator);
        draw::draw_text(fb, x + 16, y + 14, "LOCATIONS", theme.text_dim, 1);

        let locations = [("📁 Storage", "Storage"), ("📁 Initrd", "Initrd"), ("📁 RootFS", "RootFS")];
        for (i, (label, folder)) in locations.iter().enumerate() {
            let row_y = y + 42 + (i * 36);
            let is_sel = self.current_folder == *folder && self.path_stack.is_empty();
            if is_sel {
                draw::draw_rect_rounded(fb, x + 8, row_y, sidebar_w - 16, 30, 6, theme.accent, 255);
                draw::draw_text(fb, x + 16, row_y + 7, label, 0xFFFFFFFF, 1);
            } else {
                draw::draw_text(fb, x + 16, row_y + 7, label, theme.text_main, 1);
            }
        }

        // 2. Toolbar & Search Box
        let main_x = x + sidebar_w + 1;
        let main_w = w.saturating_sub(sidebar_w + 1);
        draw::draw_rect(fb, main_x, y, main_w, toolbar_h, theme.window_bg);
        draw::draw_line_h(fb, main_x, y + toolbar_h, main_w, theme.separator);

        let mut path_display = alloc::format!("Path: /{}", self.current_folder);
        for (seg, _) in &self.path_stack {
            path_display.push('/');
            path_display.push_str(seg);
        }
        draw::draw_text(fb, main_x + 14, y + 14, &path_display, theme.text_main, 1);

        // Search Input Box
        let search_w = 140usize.min(main_w / 4);
        let search_x = (x + w).saturating_sub(search_w + 160);
        let search_border_col = if self.search_active { theme.accent } else { theme.separator };
        draw::draw_rect_rounded(fb, search_x, y + 8, search_w, 28, 6, theme.titlebar_bg, 255);
        draw::draw_rect_outline(fb, search_x, y + 8, search_w, 28, search_border_col, 6);
        let search_text = if self.search_query.is_empty() { "Search..." } else { &self.search_query };
        let search_col = if self.search_query.is_empty() { theme.text_dim } else { theme.text_main };
        draw::draw_text(fb, search_x + 8, y + 14, search_text, search_col, 1);

        // + File Button
        let new_btn_x = (x + w).saturating_sub(145);
        draw::draw_rect_rounded(fb, new_btn_x, y + 8, 60, 28, 6, 0xFF16A34A, 255);
        draw::draw_text(fb, new_btn_x + 8, y + 14, "+ File", 0xFFFFFFFF, 1);

        // Delete Button
        let del_btn_x = (x + w).saturating_sub(75);
        let del_bg = if self.selected_idx.is_some() { 0xFFDC2626 } else { theme.titlebar_bg };
        let del_fg = if self.selected_idx.is_some() { 0xFFFFFFFF } else { theme.text_dim };
        draw::draw_rect_rounded(fb, del_btn_x, y + 8, 60, 28, 6, del_bg, 255);
        draw::draw_text(fb, del_btn_x + 8, y + 14, "Delete", del_fg, 1);

        // 3. File Table
        let content_y = y + toolbar_h + 1;
        let content_h = h.saturating_sub(toolbar_h + statusbar_h + 1);
        draw::draw_rect(fb, main_x, content_y, main_w, content_h, theme.window_bg);

        draw::draw_rect(fb, main_x, content_y, main_w, 26, theme.titlebar_bg);
        draw::draw_line_h(fb, main_x, content_y + 26, main_w, theme.separator);
        draw::draw_text(fb, main_x + 18, content_y + 5, "Name", theme.text_dim, 1);
        draw::draw_text(fb, main_x + (main_w / 2), content_y + 5, "Size", theme.text_dim, 1);
        draw::draw_text(fb, main_x + main_w - 120, content_y + 5, "Kind", theme.text_dim, 1);

        let row_h = 32usize;
        let mut cur_y = content_y + 27;
        let filtered = self.get_filtered_items();

        for (i, item) in filtered.iter().enumerate().skip(self.scroll_offset) {
            if cur_y + row_h > content_y + content_h { break; }
            let is_selected = self.selected_idx == Some(i);
            if is_selected {
                draw::draw_rect(fb, main_x, cur_y, main_w, row_h, theme.accent);
            }
            let text_col = if is_selected { 0xFFFFFFFF } else { theme.text_main };
            let sub_col = if is_selected { 0xFFE0F2FE } else { theme.text_dim };

            match item {
                FsItem::Directory(dname, _) => {
                    draw::draw_text(fb, main_x + 18, cur_y + 7, &alloc::format!("📁 {}", dname), text_col, 1);
                    draw::draw_text(fb, main_x + (main_w / 2), cur_y + 7, "--", sub_col, 1);
                    draw::draw_text(fb, main_x + main_w - 120, cur_y + 7, "Folder", sub_col, 1);
                }
                FsItem::File(fname, sz, mtype) => {
                    let prefix = if fname.ends_with(".elf") { "⚙️ " }
                                else if fname.ends_with(".png") || fname.ends_with(".jpg") { "🖼 " }
                                else { "📄 " };
                    draw::draw_text(fb, main_x + 18, cur_y + 7, &alloc::format!("{}{}", prefix, fname), text_col, 1);
                    draw::draw_text(fb, main_x + (main_w / 2), cur_y + 7, &alloc::format!("{} B", sz), sub_col, 1);
                    let type_str = match mtype {
                        crate::drivers::ata::MediaType::Executable => "Binary",
                        crate::drivers::ata::MediaType::Image => "Image",
                        crate::drivers::ata::MediaType::Video => "Video",
                        _ => "Text",
                    };
                    draw::draw_text(fb, main_x + main_w - 120, cur_y + 7, type_str, sub_col, 1);
                }
            }
            cur_y += row_h;
        }

        // 4. Status Bar
        let status_y = y + h - statusbar_h;
        draw::draw_rect(fb, main_x, status_y, main_w, statusbar_h, theme.titlebar_bg);
        draw::draw_line_h(fb, main_x, status_y, main_w, theme.separator);
        draw::draw_text(fb, main_x + 16, status_y + 7, &self.status, theme.text_dim, 1);
        self.needs_redraw = false;
    }

    fn on_event(&mut self, event: &InputEvent, mx: isize, my: isize) {
        let sidebar_w = self.sidebar_width as isize;
        let toolbar_h = 44isize;

        match event {
            InputEvent::MouseButton { button: 0, pressed: true } => {
                if mx < sidebar_w {
                    match (my - 42) / 36 {
                        0 => self.open_root_location("Storage"),
                        1 => self.open_root_location("Initrd"),
                        2 => self.open_root_location("RootFS"),
                        _ => {}
                    }
                    return;
                }

                // Check toolbar buttons
                let main_w = 840isize - sidebar_w;
                let search_w = 140isize;
                let search_x = (840 - search_w - 160);
                if my <= toolbar_h {
                    if mx >= search_x && mx <= search_x + search_w {
                        self.search_active = true;
                        self.needs_redraw = true;
                        return;
                    } else {
                        self.search_active = false;
                    }

                    if mx >= (840 - 145) && mx <= (840 - 85) {
                        self.create_new_file();
                        return;
                    }
                    if mx >= (840 - 75) && mx <= 840 {
                        self.delete_selected();
                        return;
                    }
                }

                let row_h = 32isize;
                let rel_y = my - (toolbar_h + 27);
                if rel_y >= 0 {
                    let clicked_idx = self.scroll_offset + (rel_y / row_h) as usize;
                    let filtered = self.get_filtered_items();
                    if clicked_idx < filtered.len() {
                        let now = crate::arch::x86_64::pit::get_ticks();
                        // Explicit Double Click Check: Interval < 45 ticks (~450ms) on the exact same row!
                        if self.last_clicked_idx == Some(clicked_idx) && now.saturating_sub(self.last_click_tick) < 45 {
                            let item = filtered[clicked_idx].clone();
                            self.execute_item(item);
                            self.last_clicked_idx = None;
                        } else {
                            // Single click selects
                            self.selected_idx = Some(clicked_idx);
                            self.last_clicked_idx = Some(clicked_idx);
                            self.needs_redraw = true;
                        }
                        self.last_click_tick = now;
                    }
                }
            }
            InputEvent::Char(c) => {
                if self.search_active {
                    if *c == '\x08' {
                        self.search_query.pop();
                    } else if *c >= ' ' && *c <= '~' {
                        self.search_query.push(*c);
                    }
                    self.scroll_offset = 0;
                    self.selected_idx = Some(0);
                    self.needs_redraw = true;
                }
            }
            InputEvent::Scroll { dy } => {
                let filtered = self.get_filtered_items();
                if *dy < 0 {
                    if self.scroll_offset + 5 < filtered.len() {
                        self.scroll_offset += 1;
                        self.needs_redraw = true;
                    }
                } else if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                    self.needs_redraw = true;
                }
            }
            InputEvent::KeyDown { keycode, .. } => {
                let filtered = self.get_filtered_items();
                match keycode {
                    0x48 => if let Some(idx) = self.selected_idx { if idx > 0 { self.selected_idx = Some(idx - 1); self.needs_redraw = true; } },
                    0x50 => if let Some(idx) = self.selected_idx { if idx + 1 < filtered.len() { self.selected_idx = Some(idx + 1); self.needs_redraw = true; } },
                    0x1C => {
                        if let Some(idx) = self.selected_idx {
                            if idx < filtered.len() {
                                let item = filtered[idx].clone();
                                self.execute_item(item);
                            }
                        }
                    }
                    0x0E if !self.search_active => self.drill_into_dir("..", 0),
                    _ => {}
                }
            }
            InputEvent::Nav(action) => {
                let filtered = self.get_filtered_items();
                match action {
                    NavAction::Up => if let Some(idx) = self.selected_idx { if idx > 0 { self.selected_idx = Some(idx - 1); self.needs_redraw = true; } },
                    NavAction::Down => if let Some(idx) = self.selected_idx { if idx + 1 < filtered.len() { self.selected_idx = Some(idx + 1); self.needs_redraw = true; } },
                    NavAction::Select => {
                        if let Some(idx) = self.selected_idx {
                            if idx < filtered.len() {
                                let item = filtered[idx].clone();
                                self.execute_item(item);
                            }
                        }
                    }
                    NavAction::Back => self.drill_into_dir("..", 0),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn on_resize(&mut self, new_w: usize, _new_h: usize) {
        self.sidebar_width = 200.min(new_w / 4).max(140);
        self.needs_redraw = true;
    }

    fn title(&self) -> &str { "Finder" }
    fn icon(&self) -> &'static str { "FND" }
    fn poll_action(&mut self) -> Option<AppAction> { self.pending_action.take() }
}
