use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use crate::arch::x86_64::interrupts::GUI_ACTIVE;
use crate::arch::x86_64::syscall::{OVERLAY_ACTIVE, OVERLAY_WIDTH, OVERLAY_HEIGHT, OVERLAY_PIXELS};
use crate::input::{self, InputEvent, NavAction};
use crate::writer::WRITER;
use core::ptr::addr_of_mut;

pub const USE_LEGACY_GUI: bool = false;

#[derive(PartialEq, Eq)]
enum ViewMode { Dashboard, Shell }

struct AppWindow {
    x: usize, y: usize,
    is_dragging: bool,
    drag_off_x: isize, drag_off_y: isize,
}

pub fn compositor_core_entry() {
    if !USE_LEGACY_GUI {
        crate::gui::run();
        return;
    }

    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }

    let (fb_ptr, width, height, pitch) = unsafe {
        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
            (writer.buffer, writer.width, writer.height, writer.pitch)
        } else {
            crate::log_error!("COMPOSITOR", "WRITER is None! Cannot start UI.");
            return;
        }
    };

    crate::log_info!("COMPOSITOR", "Active Render Loop Started ({}x{}).", width, height);

    let total_pixels = width.saturating_mul(height);
    let mut backbuffer: Vec<u32> = vec![0u32; total_pixels];
    let mut current_view = ViewMode::Dashboard;
    let mut selected_card = 0usize;
    let mut shell_buffer = String::new();
    let mut mouse_x = width / 2;
    let mut mouse_y = height / 2;
    
    let mut app_win = AppWindow { x: 200, y: 150, is_dragging: false, drag_off_x: 0, drag_off_y: 0 };

    while GUI_ACTIVE.load(Ordering::SeqCst) {
        crate::net::poll();
        let overlay_active = OVERLAY_ACTIVE.load(Ordering::Acquire);
        let overlay_w = OVERLAY_WIDTH.load(Ordering::Acquire);
        let overlay_h = OVERLAY_HEIGHT.load(Ordering::Acquire);

        while let Some(event) = input::poll_event() {
            match event {
                InputEvent::Nav(action) => {
                    match current_view {
                        ViewMode::Dashboard => {
                            if (action == NavAction::Right || action == NavAction::Down) && selected_card < 3 { selected_card += 1; }
                            if (action == NavAction::Left || action == NavAction::Up) && selected_card > 0 { selected_card -= 1; }
                            if action == NavAction::Select {
                                if selected_card == 0 { current_view = ViewMode::Shell; }
                            }
                        }
                        ViewMode::Shell => { if action == NavAction::Back { current_view = ViewMode::Dashboard; } }
                    }
                },
                InputEvent::Char(c) => {
                    if current_view == ViewMode::Shell {
                        if c == '\x08' { shell_buffer.pop(); }
                        else if c == '\n' { crate::arch::x86_64::interrupts::submit_command(shell_buffer.clone()); shell_buffer.clear(); } 
                        else { shell_buffer.push(c); }
                    }
                },
                InputEvent::MouseClick { left, .. } => {
                    if overlay_active {
                        if left {
                            let wx = app_win.x; let wy = app_win.y;
                            if mouse_x >= wx && mouse_x <= wx + overlay_w && mouse_y >= wy.saturating_sub(24) && mouse_y <= wy {
                                app_win.is_dragging = true;
                                app_win.drag_off_x = mouse_x as isize - wx as isize;
                                app_win.drag_off_y = mouse_y as isize - wy as isize;
                            }
                        } else {
                            app_win.is_dragging = false;
                        }
                    }
                },
                InputEvent::MouseMove { x, y } => {
                    mouse_x = ((mouse_x as isize + x).max(0) as usize).min(width.saturating_sub(1));
                    mouse_y = ((mouse_y as isize + y).max(0) as usize).min(height.saturating_sub(1));
                    if app_win.is_dragging {
                        app_win.x = (mouse_x as isize - app_win.drag_off_x).max(0) as usize;
                        app_win.y = (mouse_y as isize - app_win.drag_off_y).max(24) as usize;
                    }
                },
                _ => {}
            }
        }

        for px in backbuffer.iter_mut() { *px = 0xFF0F172A; }

        match current_view {
            ViewMode::Dashboard => {
                draw_text(&mut backbuffer, width, height, 60, 45, "EOS CONSOLE DASHBOARD", 0xFFFFFFFF);
                draw_text(&mut backbuffer, width, height, 60, 80, "Use Left/Right/D-Pad/Tab | Enter/A to Select", 0xFF94A3B8);
                let cards = [("Terminal", 0xFF0284C7), ("Storage", 0xFF2563EB), ("Diagnostics", 0xFFD97706), ("Settings", 0xFF475569)];
                for (i, (title, col)) in cards.iter().enumerate() {
                    let card_x = 80 + (i * 240);
                    let border_col = if i == selected_card { 0xFFFFFFFF } else { *col };
                    draw_rect(&mut backbuffer, width, height, card_x, 180, 200, 240, border_col);
                    draw_text(&mut backbuffer, width, height, card_x + 20, 380, title, 0xFFFFFFFF);
                }
            }
            ViewMode::Shell => {
                draw_text(&mut backbuffer, width, height, 60, 45, "EOS INTERACTIVE TERMINAL", 0xFFFFFFFF);
                draw_text(&mut backbuffer, width, height, 60, 80, "[Esc/B] Return to Dashboard", 0xFF94A3B8);
                let prompt = alloc::format!("eos> {}", shell_buffer);
                draw_text(&mut backbuffer, width, height, 60, 150, &prompt, 0xFF38BDF8);
            }
        }

        if overlay_active && overlay_w > 0 && overlay_h > 0 {
            let wx = app_win.x.min(width.saturating_sub(10));
            let wy = app_win.y.min(height.saturating_sub(10));
            
            draw_rect(&mut backbuffer, width, height, wx, wy.saturating_sub(24), overlay_w, 24, 0xFF334155);
            draw_text(&mut backbuffer, width, height, wx + 8, wy.saturating_sub(20), "Userspace App", 0xFFFFFFFF);
            
            unsafe {
                let src = addr_of_mut!(OVERLAY_PIXELS) as *const u32;
                for cy in 0..overlay_h {
                    if wy + cy >= height { break; }
                    for cx in 0..overlay_w {
                        if wx + cx >= width { break; }
                        backbuffer[(wy + cy) * width + (wx + cx)] = *src.add(cy * overlay_w + cx);
                    }
                }
            }
        }

        draw_cursor(&mut backbuffer, width, height, mouse_x, mouse_y);

        unsafe {
            let dst_base = fb_ptr as *mut u8;
            for y in 0..height {
                let src_ptr = backbuffer.as_ptr().add(y * width) as *const u8;
                let dst_ptr = dst_base.add(y * pitch);
                core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, width * 4);
            }
        }

        crate::arch::x86_64::pit::sleep_ms(16);
    }
}

fn draw_cursor(buf: &mut [u32], fb_w: usize, fb_h: usize, mx: usize, my: usize) {
    for cy in 0..12 {
        let py = my + cy; if py >= fb_h { break; }
        for cx in 0..12 {
            let px = mx + cx; if px >= fb_w { break; }
            if cx <= cy && (cx + cy) < 14 { buf[py * fb_w + px] = 0xFFFFFFFF; }
        }
    }
}

fn draw_rect(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    for cy in y..(y + h).min(fb_h) {
        let row = cy * fb_w;
        for cx in x..(x + w).min(fb_w) {
            let is_border = cx < x + 4 || cx >= x + w - 4 || cy < y + 4 || cy >= y + h - 4;
            buf[row + cx] = if is_border { color } else { 0xFF1E293B };
        }
    }
}

fn draw_text(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, text: &str, color: u32) {
    let mut cur_x = x;
    for c in text.chars() {
        let glyph = crate::font::get_glyph(c);
        for (gy, byte) in glyph.iter().enumerate() {
            for gx in 0..8 {
                if (byte & (1 << (7 - gx))) != 0 {
                    let px = cur_x + gx * 2; let py = y + gy * 2;
                    if px + 1 < fb_w && py + 1 < fb_h {
                        buf[py * fb_w + px] = color; buf[py * fb_w + px + 1] = color;
                        buf[(py + 1) * fb_w + px] = color; buf[(py + 1) * fb_w + px + 1] = color;
                    }
                }
            }
        }
        cur_x += 18;
    }
}
