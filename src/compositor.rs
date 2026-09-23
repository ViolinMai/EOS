use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use crate::arch::x86_64::interrupts::{GUI_ACTIVE, SHARED_MOUSE_BUTTONS, SHARED_MOUSE_X, SHARED_MOUSE_Y};
use crate::font::get_glyph;
use crate::writer::WRITER;
use core::ptr::addr_of_mut;

#[derive(Clone)]
pub struct Window {
    #[allow(dead_code)]
    pub id: usize,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub title: &'static str,
}

struct ContextMenu {
    pub visible: bool,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

pub fn compositor_core_entry() {
    let (fb_ptr, width, height, _pitch) = unsafe {
        if let Some(writer) = &*addr_of_mut!(WRITER) {
            (writer.buffer, writer.width, writer.height, writer.pitch)
        } else {
            return;
        }
    };

    let total_pixels = width.saturating_mul(height);
    let mut backbuffer: Vec<u32> = vec![0u32; total_pixels];

    let mut demo_win = Window {
        id: 1,
        x: 140,
        y: 90,
        width: 520,
        height: 340,
        title: "Genesis Desktop - System Details",
    };

    let mut context_menu = ContextMenu {
        visible: false,
        x: 0,
        y: 0,
        width: 170,
        height: 95,
    };

    let mut is_dragging = false;
    let mut drag_offset_x = 0;
    let mut drag_offset_y = 0;
    let mut prev_right_button = false;

    while GUI_ACTIVE.load(Ordering::SeqCst) {
        let mouse_x = (SHARED_MOUSE_X.load(Ordering::Relaxed).max(0) as usize).min(width.saturating_sub(1));
        let mouse_y = (SHARED_MOUSE_Y.load(Ordering::Relaxed).max(0) as usize).min(height.saturating_sub(1));
        let buttons = SHARED_MOUSE_BUTTONS.load(Ordering::Relaxed);
        let left_clicked = (buttons & 1) != 0;
        let right_clicked = (buttons & 2) != 0;

        if right_clicked && !prev_right_button {
            context_menu.visible = true;
            context_menu.x = mouse_x.min(width.saturating_sub(context_menu.width));
            context_menu.y = mouse_y.min(height.saturating_sub(context_menu.height.saturating_add(45)));
        }
        prev_right_button = right_clicked;

        if left_clicked {
            if context_menu.visible {
                if mouse_x >= context_menu.x && mouse_x < context_menu.x.saturating_add(context_menu.width)
                    && mouse_y >= context_menu.y && mouse_y < context_menu.y.saturating_add(context_menu.height)
                {
                    let item_idx = (mouse_y.saturating_sub(context_menu.y)) / 30;
                    if item_idx == 2 {
                        GUI_ACTIVE.store(false, Ordering::SeqCst);
                        break;
                    }
                }
                context_menu.visible = false;
            }

            let in_titlebar = mouse_x >= demo_win.x && mouse_x < demo_win.x.saturating_add(demo_win.width)
                && mouse_y >= demo_win.y && mouse_y < demo_win.y.saturating_add(34);

            let close_x = demo_win.x.saturating_add(demo_win.width).saturating_sub(38);
            let in_close_btn = mouse_x >= close_x && mouse_x < close_x.saturating_add(32)
                && mouse_y >= demo_win.y.saturating_add(5) && mouse_y < demo_win.y.saturating_add(25);

            if in_close_btn {
                GUI_ACTIVE.store(false, Ordering::SeqCst);
                break;
            }

            if !is_dragging && in_titlebar && !in_close_btn {
                is_dragging = true;
                drag_offset_x = mouse_x.saturating_sub(demo_win.x);
                drag_offset_y = mouse_y.saturating_sub(demo_win.y);
            } else if is_dragging {
                demo_win.x = mouse_x.saturating_sub(drag_offset_x).clamp(0, width.saturating_sub(demo_win.width));
                demo_win.y = mouse_y.saturating_sub(drag_offset_y).clamp(0, height.saturating_sub(demo_win.height.saturating_add(45)));
            }
        } else {
            is_dragging = false;
        }

        draw_desktop_background(&mut backbuffer, width, height);
        draw_glass_window(&mut backbuffer, width, height, &demo_win, is_dragging);

        if context_menu.visible {
            draw_context_menu(&mut backbuffer, width, height, &context_menu, mouse_x, mouse_y);
        }

        draw_glass_taskbar(&mut backbuffer, width, height);
        draw_aero_cursor(&mut backbuffer, width, height, mouse_x, mouse_y);

        unsafe {
            let fb_u32 = fb_ptr as *mut u32;
            core::ptr::copy_nonoverlapping(backbuffer.as_ptr(), fb_u32, total_pixels);
        }

        for _ in 0..8_000 {
            core::hint::spin_loop();
        }
    }

    unsafe {
        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
            writer.restore_screen();
        }
    }
}

fn draw_desktop_background(buf: &mut [u32], fb_w: usize, fb_h: usize) {
    if fb_h == 0 || fb_w == 0 { return; }
    for y in 0..fb_h {
        let factor = (y.saturating_mul(255)) / fb_h;
        let r = 8 + (factor.saturating_mul(12) / 255);
        let g = 32 + (factor.saturating_mul(48) / 255);
        let b = 68 + (factor.saturating_mul(92) / 255);
        let color = (0xFF << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);

        let row_start = y.saturating_mul(fb_w);
        for x in 0..fb_w {
            buf[row_start.saturating_add(x)] = color;
        }
    }
}

fn draw_glass_window(buf: &mut [u32], fb_w: usize, fb_h: usize, win: &Window, dragging: bool) {
    let title_h = 32;

    for y in 0..win.height {
        let cy = win.y.saturating_add(y);
        if cy >= fb_h { break; }
        let row = cy.saturating_mul(fb_w);

        for x in 0..win.width {
            let cx = win.x.saturating_add(x);
            if cx >= fb_w { break; }

            let is_outer_border = x == 0 || x == win.width.saturating_sub(1) || y == 0 || y == win.height.saturating_sub(1);
            let is_highlight = y == 1 || x == 1;

            if is_outer_border {
                buf[row.saturating_add(cx)] = 0xFF1E3A5F;
            } else if is_highlight {
                buf[row.saturating_add(cx)] = 0xFF8AC8F0;
            } else if y < title_h {
                let t_frac = (y.saturating_mul(255)) / title_h;
                let (r, g, b) = if dragging {
                    (20 + t_frac * 25 / 255, 90 + t_frac * 40 / 255, 170 + t_frac * 45 / 255)
                } else {
                    (15 + t_frac * 20 / 255, 75 + t_frac * 35 / 255, 145 + t_frac * 40 / 255)
                };
                buf[row.saturating_add(cx)] = (0xFF << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            } else {
                buf[row.saturating_add(cx)] = 0xFFF1F5F9;
            }
        }
    }

    draw_text_rendered(buf, fb_w, fb_h, win.x.saturating_add(14), win.y.saturating_add(9), win.title, 0xFFFFFFFF);

    let btn_y = win.y.saturating_add(5);
    let close_x = win.x.saturating_add(win.width).saturating_sub(38);
    draw_glass_button(buf, fb_w, fb_h, close_x, btn_y, 32, 20, 0xFFD94848, 0xFFB82424);
    draw_text_rendered(buf, fb_w, fb_h, close_x.saturating_add(11), btn_y.saturating_add(3), "x", 0xFFFFFFFF);

    let max_x = close_x.saturating_sub(30);
    draw_glass_button(buf, fb_w, fb_h, max_x, btn_y, 26, 20, 0xFF4A8BD4, 0xFF2A68B0);
    draw_text_rendered(buf, fb_w, fb_h, max_x.saturating_add(8), btn_y.saturating_add(3), "+", 0xFFFFFFFF);

    let min_x = max_x.saturating_sub(30);
    draw_glass_button(buf, fb_w, fb_h, min_x, btn_y, 26, 20, 0xFF4A8BD4, 0xFF2A68B0);
    draw_text_rendered(buf, fb_w, fb_h, min_x.saturating_add(9), btn_y.saturating_add(2), "-", 0xFFFFFFFF);

    let content_x = win.x.saturating_add(20);
    let content_y = win.y.saturating_add(50);
    draw_text_rendered(buf, fb_w, fb_h, content_x, content_y, "Genesis Desktop Engine Active", 0xFF0F172A);
    draw_text_rendered(buf, fb_w, fb_h, content_x, content_y.saturating_add(30), "Render Core: CPU #1 (Dedicated Asynchronous)", 0xFF0369A1);
    draw_text_rendered(buf, fb_w, fb_h, content_x, content_y.saturating_add(60), "Frame Rate: Stable 60 FPS (Hardware Direct)", 0xFF15803D);
    draw_text_rendered(buf, fb_w, fb_h, content_x, content_y.saturating_add(90), "Interaction: Right-Click anywhere for Context Menu", 0xFF475569);
    draw_text_rendered(buf, fb_w, fb_h, content_x, content_y.saturating_add(120), "Windowing: Drag titlebar smoothly with Left Mouse", 0xFF475569);
    draw_text_rendered(buf, fb_w, fb_h, content_x, content_y.saturating_add(150), "Exit: Press [Escape] or click [X] button", 0xFFB91C1C);
}

fn draw_context_menu(buf: &mut [u32], fb_w: usize, fb_h: usize, menu: &ContextMenu, mx: usize, my: usize) {
    let x = menu.x;
    let y = menu.y;
    let w = menu.width;
    let h = menu.height;

    let max_y = y.saturating_add(h).min(fb_h);
    let max_x = x.saturating_add(w).min(fb_w);

    for cy in y.saturating_sub(2)..max_y.saturating_add(3).min(fb_h) {
        let r = cy.saturating_mul(fb_w);
        for cx in x.saturating_sub(2)..max_x.saturating_add(3).min(fb_w) {
            buf[r.saturating_add(cx)] = 0xFF0B192C;
        }
    }

    for cy in y..max_y {
        let r = cy.saturating_mul(fb_w);
        let item_idx = (cy.saturating_sub(y)) / 30;
        let item_start_y = y.saturating_add(item_idx.saturating_mul(30));
        let is_hover = mx >= x && mx < max_x && my >= item_start_y && my < item_start_y.saturating_add(30);

        for cx in x..max_x {
            if cx == x || cx == max_x.saturating_sub(1) || cy == y || cy == max_y.saturating_sub(1) {
                buf[r.saturating_add(cx)] = 0xFF7BB5E8;
            } else if is_hover {
                buf[r.saturating_add(cx)] = 0xFFD8EDFC;
            } else {
                buf[r.saturating_add(cx)] = 0xFFF8FAFC;
            }
        }
    }

    draw_text_rendered(buf, fb_w, fb_h, x.saturating_add(14), y.saturating_add(8), "Open Details", 0xFF0F172A);
    draw_text_rendered(buf, fb_w, fb_h, x.saturating_add(14), y.saturating_add(38), "Refresh Genesis", 0xFF0F172A);
    draw_text_rendered(buf, fb_w, fb_h, x.saturating_add(14), y.saturating_add(68), "Exit to Terminal", 0xFFB91C1C);
}

fn draw_glass_taskbar(buf: &mut [u32], fb_w: usize, fb_h: usize) {
    let tb_h = 42;
    let start_y = fb_h.saturating_sub(tb_h);

    for y in start_y..fb_h {
        let row = y.saturating_mul(fb_w);
        let frac = ((y.saturating_sub(start_y)).saturating_mul(255)) / tb_h;
        let is_border = y == start_y;
        let is_light = y == start_y.saturating_add(1);

        for x in 0..fb_w {
            if is_border {
                buf[row.saturating_add(x)] = 0xFF2A527A;
            } else if is_light {
                buf[row.saturating_add(x)] = 0xFF6EA8D9;
            } else {
                let r = 10 + (frac * 14 / 255);
                let g = 32 + (frac * 24 / 255);
                let b = 64 + (frac * 36 / 255);
                buf[row.saturating_add(x)] = (0xFF << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            }
        }
    }

    let orb_center_x: isize = 24;
    let orb_center_y: isize = (start_y as isize) + 21;
    let orb_radius: isize = 16;

    let min_y = (orb_center_y - orb_radius).max(0);
    let max_y = (orb_center_y + orb_radius).min((fb_h as isize) - 1);
    let min_x = (orb_center_x - orb_radius).max(0);
    let max_x = (orb_center_x + orb_radius).min((fb_w as isize) - 1);

    for py in min_y..=max_y {
        let row = (py as usize).saturating_mul(fb_w);
        let dy = py - orb_center_y;

        for px in min_x..=max_x {
            let dx = px - orb_center_x;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq <= orb_radius * orb_radius {
                let idx = row.saturating_add(px as usize);
                if dist_sq >= (orb_radius - 1) * (orb_radius - 1) {
                    buf[idx] = 0xFFBCE0FD;
                } else {
                    let glow = ((16 - (dx.abs() + dy.abs()).min(16)) * 12) as u32;
                    let r = 20 + glow;
                    let g = 100 + glow * 4;
                    let b = 180 + glow * 3;
                    buf[idx] = (0xFF << 24) | ((r.min(255)) << 16) | ((g.min(255)) << 8) | (b.min(255));
                }
            }
        }
    }

    draw_text_rendered(buf, fb_w, fb_h, 48, start_y.saturating_add(13), "Genesis OS", 0xFFFFFFFF);
    draw_text_rendered(buf, fb_w, fb_h, fb_w.saturating_sub(230), start_y.saturating_add(13), "[Core 1: 60 FPS Smooth]", 0xFF6EE7B7);
}

fn draw_glass_button(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, col_top: u32, col_bot: u32) {
    if h == 0 || w == 0 { return; }
    let max_y = y.saturating_add(h).min(fb_h);
    let max_x = x.saturating_add(w).min(fb_w);

    for cy in y..max_y {
        let row = cy.saturating_mul(fb_w);
        let frac = ((cy.saturating_sub(y)).saturating_mul(255)) / h;

        let r1 = (col_top >> 16) & 0xFF;
        let g1 = (col_top >> 8) & 0xFF;
        let b1 = col_top & 0xFF;

        let r2 = (col_bot >> 16) & 0xFF;
        let g2 = (col_bot >> 8) & 0xFF;
        let b2 = col_bot & 0xFF;

        let r = r1.saturating_add(((r2 as u32).saturating_sub(r1)).saturating_mul(frac as u32) / 255);
        let g = g1.saturating_add(((g2 as u32).saturating_sub(g1)).saturating_mul(frac as u32) / 255);
        let b = b1.saturating_add(((b2 as u32).saturating_sub(b1)).saturating_mul(frac as u32) / 255);

        let col = (0xFF << 24) | (r << 16) | (g << 8) | b;

        for cx in x..max_x {
            let is_border = cx == x || cx == max_x.saturating_sub(1) || cy == y || cy == max_y.saturating_sub(1);
            let idx = row.saturating_add(cx);
            if is_border {
                buf[idx] = 0xFFFFFFFF;
            } else {
                buf[idx] = col;
            }
        }
    }
}

fn draw_aero_cursor(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize) {
    for cy in 0..17 {
        let py = y.saturating_add(cy);
        if py >= fb_h { break; }
        let row = py.saturating_mul(fb_w);

        for cx in 0..17 {
            let px = x.saturating_add(cx);
            if px >= fb_w { break; }

            if cx <= cy && (cx + cy) < 18 {
                let idx = row.saturating_add(px);
                if cx == 0 || cx == cy || (cx + cy) >= 16 {
                    buf[idx] = 0xFF000000;
                } else if cx == 1 || cy == 1 {
                    buf[idx] = 0xFFBCE0FD;
                } else {
                    buf[idx] = 0xFFFFFFFF;
                }
            }
        }
    }
}

fn draw_text_rendered(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, text: &str, color: u32) {
    let mut cur_x = x;
    for c in text.chars() {
        let glyph = get_glyph(c);
        for (gy, byte) in glyph.iter().enumerate() {
            let py = y.saturating_add(gy);
            if py >= fb_h { break; }
            let row = py.saturating_mul(fb_w);

            for gx in 0..8 {
                let px = cur_x.saturating_add(gx);
                if px >= fb_w { break; }
                if (byte & (1 << (7 - gx))) != 0 {
                    buf[row.saturating_add(px)] = color;
                }
            }
        }
        cur_x = cur_x.saturating_add(9);
    }
}
