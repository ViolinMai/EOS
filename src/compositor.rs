use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use crate::arch::x86_64::interrupts::GUI_ACTIVE;
use crate::arch::x86_64::syscall::{OVERLAY_ACTIVE, OVERLAY_WIDTH, OVERLAY_HEIGHT, OVERLAY_PIXELS};
use crate::drivers::gamepad::{GAMEPAD_CONNECTED, BTN_MASK, STICK_X, STICK_Y};
use crate::writer::WRITER;
use core::ptr::{addr_of, addr_of_mut};
use crate::task::{request_elf_execution, ELF_ACTIVE_RUNNING};
use crate::fs::{vfs_list_all, vfs_read_bytes, VfsNode};
use crate::arch::x86_64::pit;

#[derive(Clone)]
pub struct ConsoleCard {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub is_explorer: bool,
    pub exec_path: &'static str,
    pub color: u32,
}

#[derive(PartialEq, Eq)]
enum ViewMode {
    Dashboard,
    FileExplorer,
    FileViewer,
}

pub fn compositor_core_entry() {
    let (fb_ptr, width, height) = unsafe {
        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
            (writer.buffer, writer.width, writer.height)
        } else { return; }
    };

    let total_pixels = width.saturating_mul(height);
    let mut backbuffer: Vec<u32> = vec![0u32; total_pixels];

    let cards = vec![
        ConsoleCard { title: "File Explorer", subtitle: "Browse RootFS & Disks", is_explorer: true, exec_path: "", color: 0xFF0284C7 },
        ConsoleCard { title: "Media Player", subtitle: "PNG Hardware Viewer", is_explorer: false, exec_path: "app.elf", color: 0xFF2563EB },
        ConsoleCard { title: "System Cores", subtitle: "SMP 8-Core Monitor", is_explorer: false, exec_path: "", color: 0xFF059669 },
        ConsoleCard { title: "Console Settings", subtitle: "EOS Configuration", is_explorer: false, exec_path: "", color: 0xFF475569 },
    ];

    let mut current_view = ViewMode::Dashboard;
    let mut selected_card = 0;
    
    let mut explorer_files: Vec<VfsNode> = Vec::new();
    let mut explorer_selected_idx = 0;
    let mut explorer_scroll_offset = 0;

    let mut viewing_filename = String::new();
    let mut viewing_lines: Vec<String> = Vec::new();

    let mut last_nav_tick = pit::get_ticks();
    let mut a_pressed_last = false;
    let mut b_pressed_last = false;

    while GUI_ACTIVE.load(Ordering::SeqCst) {
        let current_tick = pit::get_ticks();
        let btns = BTN_MASK.load(Ordering::Relaxed);
        let x_axis = STICK_X.load(Ordering::Relaxed);
        let y_axis = STICK_Y.load(Ordering::Relaxed);
        
        let left  = (btns & 0x40) != 0 || x_axis < 60;
        let right = (btns & 0x80) != 0 || x_axis > 190;
        let up    = (btns & 0x10) != 0 || y_axis > 190;
        let down  = (btns & 0x20) != 0 || y_axis < 60;

        let a_btn = (btns & 0x01) != 0;
        let b_btn = (btns & 0x02) != 0;

        // --- نظام التوجيه وإغلاق النوافذ العائمة ---
        if OVERLAY_ACTIVE.load(Ordering::Relaxed) {
            if b_btn && !b_pressed_last {
                OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
                last_nav_tick = current_tick + 20;
            }
        } else {
            match current_view {
                ViewMode::Dashboard => {
                    if current_tick.saturating_sub(last_nav_tick) > 16 {
                        if right && selected_card < cards.len() - 1 {
                            selected_card += 1;
                            last_nav_tick = current_tick;
                        } else if left && selected_card > 0 {
                            selected_card -= 1;
                            last_nav_tick = current_tick;
                        }
                    }

                    if a_btn && !a_pressed_last {
                        let card = &cards[selected_card];
                        if card.is_explorer {
                            explorer_files = vfs_list_all();
                            explorer_selected_idx = 0;
                            explorer_scroll_offset = 0;
                            current_view = ViewMode::FileExplorer;
                        } else if !card.exec_path.is_empty() {
                            let _ = request_elf_execution(card.exec_path, "icon.png");
                        }
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::FileExplorer => {
                    if current_tick.saturating_sub(last_nav_tick) > 15 {
                        if down && !explorer_files.is_empty() && explorer_selected_idx + 1 < explorer_files.len() {
                            explorer_selected_idx += 1;
                            if explorer_selected_idx >= explorer_scroll_offset + 9 {
                                explorer_scroll_offset += 1;
                            }
                            last_nav_tick = current_tick;
                        } else if up && explorer_selected_idx > 0 {
                            explorer_selected_idx -= 1;
                            if explorer_selected_idx < explorer_scroll_offset {
                                explorer_scroll_offset = explorer_scroll_offset.saturating_sub(1);
                            }
                            last_nav_tick = current_tick;
                        }
                    }

                    if b_btn && !b_pressed_last {
                        current_view = ViewMode::Dashboard;
                        last_nav_tick = current_tick + 20;
                    }

                    if a_btn && !a_pressed_last && !explorer_files.is_empty() {
                        let chosen = &explorer_files[explorer_selected_idx];
                        let (name, _sz) = match chosen {
                            VfsNode::Ext2(n, s) => (n.clone(), *s),
                            VfsNode::Disk(n, s) => (n.clone(), *s as usize),
                            VfsNode::Ramdisk(n, s) => (n.clone(), *s),
                        };

                        if name.ends_with(".elf") {
                            let _ = request_elf_execution(&name, "");
                        } else if name.ends_with(".png") {
                            let _ = request_elf_execution("app.elf", &name);
                        } else {
                            if let Ok(bytes) = vfs_read_bytes(&name) {
                                viewing_filename = name;
                                if let Ok(text) = core::str::from_utf8(&bytes) {
                                    viewing_lines = text.lines().map(String::from).collect();
                                } else {
                                    viewing_lines = vec![String::from("[Binary File - Non Printable]")];
                                }
                                current_view = ViewMode::FileViewer;
                            }
                        }
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::FileViewer => {
                    if b_btn && !b_pressed_last {
                        current_view = ViewMode::FileExplorer;
                        last_nav_tick = current_tick + 20;
                    }
                }
            }
        }

        a_pressed_last = a_btn;
        b_pressed_last = b_btn;
        let is_running_elf = ELF_ACTIVE_RUNNING.load(Ordering::Relaxed);

        // --- Rendering Pipeline ---
        match current_view {
            ViewMode::Dashboard => {
                draw_ps5_bg(&mut backbuffer, width, height, cards[selected_card].color);
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, "DASHBOARD");

                let card_w = 260;
                let card_h = 260;
                let gap = 40;
                let start_x = (width / 2).saturating_sub((card_w / 2) + selected_card * (card_w + gap));
                let base_y = height / 2 - (card_h / 2) - 40;

                for (i, card) in cards.iter().enumerate() {
                    let cx = start_x as isize + (i * (card_w + gap)) as isize;
                    if cx < -300 || cx > width as isize { continue; }
                    let x = cx as usize;
                    
                    let is_focused = i == selected_card;
                    let (y, h, w) = if is_focused {
                        (base_y.saturating_sub(20), card_h + 40, card_w + 40)
                    } else {
                        (base_y + 20, card_h, card_w)
                    };

                    draw_card(&mut backbuffer, width, height, x, y, w, h, card.color, is_focused);
                }

                let focused = &cards[selected_card];
                draw_text_centered(&mut backbuffer, width, height, width / 2, base_y + card_h + 70, focused.title, 0xFFFFFFFF, 3);
                draw_text_centered(&mut backbuffer, width, height, width / 2, base_y + card_h + 120, focused.subtitle, 0xFFA0AEC0, 2);

                draw_bottom_bar(&mut backbuffer, width, height, "[A] Open App / Folder", "[B] Exit");
            }

            ViewMode::FileExplorer => {
                draw_ps5_bg(&mut backbuffer, width, height, 0xFF0284C7);
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, "FILE EXPLORER");

                let list_x = 100;
                let list_y = 120;
                let row_h = 52;
                let list_w = width - 200;

                let visible_count = 9;
                for i in 0..visible_count {
                    let idx = explorer_scroll_offset + i;
                    if idx >= explorer_files.len() { break; }

                    let node = &explorer_files[idx];
                    let (tag, name, size) = match node {
                        VfsNode::Ext2(n, s) => ("EXT2", n.as_str(), *s),
                        VfsNode::Disk(n, s) => ("FAT32", n.as_str(), *s as usize),
                        VfsNode::Ramdisk(n, s) => ("INITRD", n.as_str(), *s),
                    };

                    let is_focused = idx == explorer_selected_idx;
                    let y = list_y + (i * row_h);

                    draw_explorer_row(&mut backbuffer, width, height, list_x, y, list_w, row_h - 8, tag, name, size, is_focused);
                }

                draw_bottom_bar(&mut backbuffer, width, height, "[A] Execute / View File", "[B] Back to Dashboard");
            }

            ViewMode::FileViewer => {
                draw_ps5_bg(&mut backbuffer, width, height, 0xFF1E293B);
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, "FILE VIEWER");

                draw_text_scaled(&mut backbuffer, width, height, 60, 110, viewing_filename.as_str(), 0xFF38BDF8, 2);

                let start_y = 160;
                let max_lines = 15;
                for (idx, line) in viewing_lines.iter().take(max_lines).enumerate() {
                    let y = start_y + (idx * 30);
                    draw_text_scaled(&mut backbuffer, width, height, 60, y, line.as_str(), 0xFFF1F5F9, 2);
                }

                draw_bottom_bar(&mut backbuffer, width, height, "", "[B] Return to Explorer");
            }
        }

        // رسم النافذة العائمة الخاصة بالصور
        if OVERLAY_ACTIVE.load(Ordering::Relaxed) {
            let ow = OVERLAY_WIDTH.load(Ordering::Relaxed);
            let oh = OVERLAY_HEIGHT.load(Ordering::Relaxed);
            draw_floating_modal(&mut backbuffer, width, height, ow, oh);
        }

        unsafe {
            core::ptr::copy_nonoverlapping(backbuffer.as_ptr(), fb_ptr as *mut u32, total_pixels);
        }

        for _ in 0..4_000 { core::hint::spin_loop(); }
    }
}

fn draw_floating_modal(buf: &mut [u32], fb_w: usize, fb_h: usize, img_w: usize, img_h: usize) {
    let win_w = img_w + 40;
    let win_h = img_h + 80;
    let win_x = (fb_w.saturating_sub(win_w)) / 2;
    let win_y = (fb_h.saturating_sub(win_h)) / 2;

    // تظليل الخلفية بشفافية (Dim Effect)
    for y in 0..fb_h {
        let r = y * fb_w;
        for x in 0..fb_w {
            let p = buf[r + x];
            let cr = ((p >> 16) & 0xFF) / 3;
            let cg = ((p >> 8) & 0xFF) / 3;
            let cb = (p & 0xFF) / 3;
            buf[r + x] = (0xFF << 24) | (cr << 16) | (cg << 8) | cb;
        }
    }

    // إطار النافذة العائمة
    for y in win_y..(win_y + win_h).min(fb_h) {
        let row = y * fb_w;
        for x in win_x..(win_x + win_w).min(fb_w) {
            let is_border = x == win_x || x == win_x + win_w - 1 || y == win_y || y == win_y + win_h - 1;
            let is_header = y < win_y + 40;
            if is_border {
                buf[row + x] = 0xFF38BDF8; 
            } else if is_header {
                buf[row + x] = 0xFF0F172A; 
            } else {
                buf[row + x] = 0xFF1E293B; 
            }
        }
    }

    draw_text_scaled(buf, fb_w, fb_h, win_x + 20, win_y + 12, "IMAGE VIEWER", 0xFFFFFFFF, 2);
    draw_text_scaled(buf, fb_w, fb_h, win_x + win_w - 140, win_y + 14, "[B] Close", 0xFFF87171, 2);

    let content_x = win_x + 20;
    let content_y = win_y + 60;

    unsafe {
        let src = addr_of!(OVERLAY_PIXELS) as *const u32;
        for dy in 0..img_h {
            let py = content_y + dy;
            if py >= fb_h { break; }
            let dst_row = py * fb_w;
            for dx in 0..img_w {
                let px = content_x + dx;
                if px >= fb_w { break; }
                let col = *src.add(dy * img_w + dx);
                let a = (col >> 24) & 0xFF;
                if a == 255 {
                    buf[dst_row + px] = col;
                } else if a > 0 {
                    let bg = buf[dst_row + px];
                    let inv_a = 255 - a;
                    let r = (((col >> 16) & 0xFF) * a + ((bg >> 16) & 0xFF) * inv_a) / 255;
                    let g = (((col >> 8) & 0xFF) * a + ((bg >> 8) & 0xFF) * inv_a) / 255;
                    let b = ((col & 0xFF) * a + (bg & 0xFF) * inv_a) / 255;
                    buf[dst_row + px] = (0xFF << 24) | (r << 16) | (g << 8) | b;
                }
            }
        }
    }
}

fn draw_ps5_bg(buf: &mut [u32], fb_w: usize, fb_h: usize, tint: u32) {
    let tr = (tint >> 16) & 0xFF;
    let tg = (tint >> 8) & 0xFF;
    let tb = tint & 0xFF;
    for y in 0..fb_h {
        let row = y * fb_w;
        let factor = (y * 255) / fb_h; 
        let r = (8 + (factor * tr as usize / 600)) as u32;
        let g = (12 + (factor * tg as usize / 600)) as u32;
        let b = (20 + (factor * tb as usize / 600)) as u32;
        let color = (0xFF << 24) | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
        for x in 0..fb_w { buf[row + x] = color; }
    }
}

fn draw_top_bar(buf: &mut [u32], fb_w: usize, fb_h: usize, connected: bool, running_elf: bool, title: &str) {
    draw_text_scaled(buf, fb_w, fb_h, 50, 35, "EOS CONSOLE", 0xFFFFFFFF, 2);
    draw_text_scaled(buf, fb_w, fb_h, 240, 37, "|", 0xFF64748B, 2);
    draw_text_scaled(buf, fb_w, fb_h, 270, 37, title, 0xFF94A3B8, 2);

    if running_elf {
        draw_text_scaled(buf, fb_w, fb_h, fb_w - 640, 37, "[Core 2: Processing]", 0xFFF59E0B, 1);
    } else {
        draw_text_scaled(buf, fb_w, fb_h, fb_w - 640, 37, "[Core 2: Idle Worker]", 0xFF64748B, 1);
    }

    let status = if connected { "[GameSir: Connected]" } else { "[Searching Gamepad...]" };
    let color = if connected { 0xFF34D399 } else { 0xFFF87171 };
    draw_text_scaled(buf, fb_w, fb_h, fb_w - 380, 37, status, color, 1);
}

fn draw_bottom_bar(buf: &mut [u32], fb_w: usize, fb_h: usize, a_action: &str, b_action: &str) {
    let y = fb_h - 60;
    for cy in y..fb_h {
        let row = cy * fb_w;
        for cx in 0..fb_w { buf[row + cx] = 0xFF090D16; }
    }
    if !a_action.is_empty() { draw_text_scaled(buf, fb_w, fb_h, 60, y + 20, a_action, 0xFFFFFFFF, 2); }
    if !b_action.is_empty() { draw_text_scaled(buf, fb_w, fb_h, 520, y + 20, b_action, 0xFF94A3B8, 2); }
}

fn draw_card(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, color: u32, focused: bool) {
    for cy in y..(y+h).min(fb_h) {
        let row = cy * fb_w;
        for cx in x..(x+w).min(fb_w) {
            let is_border = cx < x + 5 || cx > x + w - 6 || cy < y + 5 || cy > y + h - 6;
            if focused && is_border {
                buf[row + cx] = 0xFFFFFFFF;
            } else if is_border {
                buf[row + cx] = 0xFF1E293B; 
            } else {
                buf[row + cx] = color;
            }
        }
    }
}

fn draw_explorer_row(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, tag: &str, name: &str, size: usize, focused: bool) {
    let bg_color = if focused { 0xFF2563EB } else { 0xFF1E293B };
    for cy in y..(y+h).min(fb_h) {
        let row = cy * fb_w;
        for cx in x..(x+w).min(fb_w) {
            let is_border = cx == x || cx == x + w - 1 || cy == y || cy == y + h - 1;
            if focused && is_border {
                buf[row + cx] = 0xFFFFFFFF;
            } else {
                buf[row + cx] = bg_color;
            }
        }
    }

    draw_text_scaled(buf, fb_w, fb_h, x + 20, y + 12, tag, if focused { 0xFFBAE6FD } else { 0xFF38BDF8 }, 2);
    draw_text_scaled(buf, fb_w, fb_h, x + 180, y + 12, name, 0xFFFFFFFF, 2);
    
    let sz_str = if size > 1024 * 1024 { "MB" } else if size > 1024 { "KB" } else { "Bytes" };
    draw_text_scaled(buf, fb_w, fb_h, x + w - 160, y + 12, sz_str, 0xFF94A3B8, 2);
}

fn draw_text_scaled(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, text: &str, color: u32, scale: usize) {
    let mut cur_x = x;
    for c in text.chars() {
        let glyph = crate::font::get_glyph(c);
        for (gy, byte) in glyph.iter().enumerate() {
            for gx in 0..8 {
                if (byte & (1 << (7 - gx))) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = cur_x + (gx * scale) + dx;
                            let py = y + (gy * scale) + dy;
                            if px < fb_w && py < fb_h { buf[py * fb_w + px] = color; }
                        }
                    }
                }
            }
        }
        cur_x += 9 * scale;
    }
}

fn draw_text_centered(buf: &mut [u32], fb_w: usize, fb_h: usize, center_x: usize, y: usize, text: &str, color: u32, scale: usize) {
    let text_w = text.chars().count() * 9 * scale;
    let start_x = center_x.saturating_sub(text_w / 2);
    draw_text_scaled(buf, fb_w, fb_h, start_x, y, text, color, scale);
}
