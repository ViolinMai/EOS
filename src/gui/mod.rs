pub mod theme;
pub mod draw;
pub mod window;
pub mod apps;
pub mod metrics;

use alloc::vec;
use alloc::vec::Vec;
use alloc::boxed::Box;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::arch::x86_64::interrupts::GUI_ACTIVE;
use crate::input::{self, InputEvent, MOD_CTRL, MOD_SHIFT, MOD_WIN, MOD_ALT};
use crate::writer::WRITER;
use core::ptr::addr_of_mut;

use self::draw::{FrameBuffer, Rect, DamageTracker};
use self::theme::get_current_theme;
use self::window::WindowManager;
use self::apps::{terminal::TerminalApp, finder::FinderApp, settings::SettingsApp, userwin::UserAppOverlay, preview::PreviewApp, textedit::TextEditApp, taskmanager::ActivityMonitorApp, AppAction};
use self::metrics::get_metrics;

pub static FULL_REDRAW_REQUIRED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct DockItemState { target_scaled: isize, current_scaled: isize, velocity_scaled: isize }

pub fn run() {
    unsafe { core::arch::asm!("sti", options(nomem, nostack)); }
    crate::arch::x86_64::pit::calibrate_tsc();
    
    let (fb_ptr, width, height, pitch) = unsafe {
        if let Some(writer) = &mut *addr_of_mut!(WRITER) { (writer.buffer, writer.width, writer.height, writer.pitch) } else { return; }
    };

    crate::log_info!("GUI", "macOS Engine ({}x{}) - Full Dynamic Damage Pipeline", width, height);

    let mut backbuffer: Vec<u32> = vec![get_current_theme().desktop_bg; width * height];
    let mut fb = FrameBuffer { pixels: &mut backbuffer, width, height, pitch_pixels: width };
    let mut wm = WindowManager::new(width, height);
    let mut damage = DamageTracker::new();
    
    let mut mouse_x = (width / 2) as isize;
    let mut mouse_y = (height / 2) as isize;
    let mut prev_mouse_rect = Rect { x: mouse_x as usize, y: mouse_y as usize, w: 24, h: 24 };

    // Default open window: Finder
    wm.add_window(Box::new(FinderApp::new()), 100, 70, 840, 560);
    damage.add(Rect { x: 0, y: 0, w: width, h: height }); 
    
    let target_frame_us = 16_666u64;
    let mut apple_menu_open = false;
    let mut search_open = false;
    let mut search_query = String::new();
    
    let apps_info = [
        ("Finder", "FND", 0xFF0284C7),
        ("Terminal", "TRM", 0xFF18181B),
        ("System Settings", "SET", 0xFF475569),
        ("Activity Monitor", "ACT", 0xFFE11D48),
        ("TextEdit", "TXT", 0xFF0D9488)
    ];

    let mut dock_states = [
        DockItemState { target_scaled: 5000, current_scaled: 5000, velocity_scaled: 0 },
        DockItemState { target_scaled: 5000, current_scaled: 5000, velocity_scaled: 0 },
        DockItemState { target_scaled: 5000, current_scaled: 5000, velocity_scaled: 0 },
        DockItemState { target_scaled: 5000, current_scaled: 5000, velocity_scaled: 0 },
        DockItemState { target_scaled: 5000, current_scaled: 5000, velocity_scaled: 0 },
    ];

    let mut last_clock_tick = 0u64;

    while GUI_ACTIVE.load(Ordering::SeqCst) {
        let frame_start_tsc = crate::arch::x86_64::pit::read_tsc();
        crate::net::poll();

        if FULL_REDRAW_REQUIRED.swap(false, Ordering::SeqCst) {
            wm.invalidate_all_windows();
            damage.add(Rect { x: 0, y: 0, w: width, h: height });
        }

        let theme = get_current_theme();
        let metrics = get_metrics();

        let elf_running = crate::task::ELF_ACTIVE_RUNNING.load(Ordering::SeqCst);
        if elf_running && !wm.has_window("Userspace Desktop") {
            wm.add_window(Box::new(UserAppOverlay::new()), 160, 80, 960, 640);
            damage.add(Rect { x: 160, y: 80, w: 960, h: 640 });
        }

        let mut mouse_moved = false;
        let mut click_processed = false;

        let item_gap = 14isize;
        let total_dock_content_w: isize = dock_states.iter().map(|s| s.current_scaled / 100).sum::<isize>() + (item_gap * 4) + 40;
        let dock_x = (width as isize - total_dock_content_w) / 2;
        let dock_h = 60 + (metrics.scale as isize * 8);
        let dock_y = height as isize - dock_h - 12;

        wm.geometry.top_bar_height = 36;
        wm.geometry.dock_height = dock_h as usize + 20;

        while let Some(event) = input::poll_event() {
            match event {
                InputEvent::MouseMove { x, y } => {
                    mouse_x = (mouse_x + x).clamp(0, (width - 1) as isize);
                    mouse_y = (mouse_y + y).clamp(0, (height - 1) as isize);
                    mouse_moved = true;
                    wm.handle_event(&event, mouse_x, mouse_y, &mut damage);
                }
                InputEvent::KeyDown { keycode, mods } => {
                    if keycode == 0x01 && (mods & MOD_CTRL) != 0 && (mods & MOD_SHIFT) != 0 {
                        if !wm.unminimize_or_focus("Activity Monitor", &mut damage) { 
                            wm.add_window(Box::new(ActivityMonitorApp::new()), 220, 110, 640, 440); 
                            damage.add(Rect { x: 220, y: 110, w: 640, h: 440 });
                        }
                    } 
                    else if (mods & MOD_WIN) != 0 && (mods & MOD_ALT) != 0 {
                        search_open = !search_open;
                        search_query.clear();
                        damage.add(Rect { x: (width/2)-300, y: (height/2)-100, w: 600, h: 200 });
                    }
                    else if search_open {
                        if keycode == 0x0E && !search_query.is_empty() { search_query.pop(); damage.add(Rect { x: (width/2)-300, y: (height/2)-100, w: 600, h: 200 }); }
                        else if keycode == 0x01 { search_open = false; damage.add(Rect { x: (width/2)-300, y: (height/2)-100, w: 600, h: 200 }); }
                        else if keycode == 0x1C {
                            let q = search_query.trim().to_ascii_lowercase();
                            if q == "terminal" { if !wm.unminimize_or_focus("Terminal", &mut damage) { wm.add_window(Box::new(TerminalApp::new()), 160, 120, 700, 440); damage.add(Rect { x: 160, y: 120, w: 700, h: 440 }); } }
                            else if q == "settings" { if !wm.unminimize_or_focus("System Settings", &mut damage) { wm.add_window(Box::new(SettingsApp::new()), 240, 160, 640, 420); damage.add(Rect { x: 240, y: 160, w: 640, h: 420 }); } }
                            else if q == "finder" { if !wm.unminimize_or_focus("Finder", &mut damage) { wm.add_window(Box::new(FinderApp::new()), 120, 80, 840, 560); damage.add(Rect { x: 120, y: 80, w: 840, h: 560 }); } }
                            else if q == "reboot" { crate::arch::x86_64::power::reboot(); }
                            search_open = false;
                            damage.add(Rect { x: (width/2)-300, y: (height/2)-100, w: 600, h: 200 });
                        }
                    } else {
                        wm.handle_event(&event, mouse_x, mouse_y, &mut damage);
                    }
                }
                InputEvent::Char(c) => {
                    if search_open {
                        if c >= ' ' && c <= '~' { search_query.push(c); damage.add(Rect { x: (width/2)-300, y: (height/2)-100, w: 600, h: 200 }); }
                    } else {
                        wm.handle_event(&event, mouse_x, mouse_y, &mut damage);
                    }
                }
                InputEvent::MouseButton { button: 0, pressed: true } => {
                    click_processed = true;
                    if search_open { search_open = false; damage.add(Rect { x: (width/2)-300, y: (height/2)-100, w: 600, h: 200 }); continue; }

                    if mouse_y < 36 && mouse_x >= 10 && mouse_x <= 60 {
                        apple_menu_open = !apple_menu_open;
                        damage.add(Rect { x: 10, y: 36, w: 200, h: 220 });
                    } else if apple_menu_open {
                        if mouse_x >= 10 && mouse_x <= 210 && mouse_y >= 36 && mouse_y <= 256 {
                            match (mouse_y - 36) / 30 {
                                0 => { wm.add_window(Box::new(SettingsApp::new()), 240, 160, 640, 420); damage.add(Rect { x: 240, y: 160, w: 640, h: 420 }); },
                                1 => { wm.add_window(Box::new(FinderApp::new()), 120, 80, 840, 560); damage.add(Rect { x: 120, y: 80, w: 840, h: 560 }); },
                                2 => { wm.add_window(Box::new(ActivityMonitorApp::new()), 180, 110, 640, 440); damage.add(Rect { x: 180, y: 110, w: 640, h: 440 }); },
                                3 => { wm.add_window(Box::new(TerminalApp::new()), 160, 120, 700, 440); damage.add(Rect { x: 160, y: 120, w: 700, h: 440 }); },
                                5 => crate::arch::x86_64::power::reboot(),
                                _ => {}
                            }
                        }
                        apple_menu_open = false;
                        damage.add(Rect { x: 10, y: 36, w: 200, h: 220 });
                    } else if mouse_y >= dock_y - 40 && mouse_y <= dock_y + dock_h && mouse_x >= dock_x && mouse_x <= dock_x + total_dock_content_w {
                        let mut cur_accum_x = dock_x + 20;
                        for (i, (title, _, _)) in apps_info.iter().enumerate() {
                            let isize = dock_states[i].current_scaled / 100;
                            if mouse_x >= cur_accum_x && mouse_x <= cur_accum_x + isize {
                                if !wm.unminimize_or_focus(title, &mut damage) {
                                    match i {
                                        0 => { wm.add_window(Box::new(FinderApp::new()), 100, 70, 840, 560); damage.add(Rect { x: 100, y: 70, w: 840, h: 560 }); },
                                        1 => { wm.add_window(Box::new(TerminalApp::new()), 160, 120, 700, 440); damage.add(Rect { x: 160, y: 120, w: 700, h: 440 }); },
                                        2 => { wm.add_window(Box::new(SettingsApp::new()), 240, 160, 640, 420); damage.add(Rect { x: 240, y: 160, w: 640, h: 420 }); },
                                        3 => { wm.add_window(Box::new(ActivityMonitorApp::new()), 200, 110, 640, 440); damage.add(Rect { x: 200, y: 110, w: 640, h: 440 }); },
                                        4 => { wm.add_window(Box::new(TextEditApp::new("Notes.txt".into(), "".into())), 200, 140, 620, 440); damage.add(Rect { x: 200, y: 140, w: 620, h: 440 }); },
                                        _ => {}
                                    }
                                }
                                break;
                            }
                            cur_accum_x += isize + item_gap;
                        }
                    } else {
                        wm.handle_event(&event, mouse_x, mouse_y, &mut damage);
                    }
                }
                InputEvent::MouseButton { button: 0, pressed: false } => {
                    wm.handle_event(&event, mouse_x, mouse_y, &mut damage);
                }
                _ => {
                    if !search_open { wm.handle_event(&event, mouse_x, mouse_y, &mut damage); }
                }
            }
        }
        
        let mut pending_actions = Vec::new();
        for win in &mut wm.windows {
            if let Some(act) = win.app.poll_action() { pending_actions.push(act); }
        }
        for act in pending_actions {
            match act {
                AppAction::OpenImage { name, width: iw, height: ih, pixels } => {
                    wm.add_window(Box::new(PreviewApp::new(name, iw, ih, pixels)), 140, 90, 720, 520);
                    damage.add(Rect { x: 140, y: 90, w: 720, h: 520 });
                }
                AppAction::OpenText { name, content } => {
                    wm.add_window(Box::new(TextEditApp::new(name, content)), 160, 100, 640, 480);
                    damage.add(Rect { x: 160, y: 100, w: 640, h: 480 });
                }
                AppAction::LaunchElf { name, arg } => {
                    let _ = crate::task::request_elf_execution(&name, &arg);
                }
            }
        }

        for win in &mut wm.windows {
            if !win.is_minimized && win.app.wants_redraw() { win.is_dirty = true; damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h }); }
        }

        let mut dock_animating = false;
        let mut cur_accum_x = dock_x + 20;
        let mut hovered_item = None;

        for i in 0..5 {
            let item_sz = dock_states[i].current_scaled / 100;
            let center_x = cur_accum_x + (item_sz / 2);
            let dist = mouse_x.abs_diff(center_x);
            let in_dock_zone = mouse_y >= dock_y - 30 && mouse_y <= height as isize;

            let base_scale = 4000 + (metrics.scale as isize * 1000);
            if in_dock_zone && dist < 100 {
                dock_states[i].target_scaled = base_scale + (((100 - dist) as isize * 150) / 3);
                if dist < (item_sz as usize / 2) { hovered_item = Some((i, cur_accum_x, item_sz as usize)); }
            } else { dock_states[i].target_scaled = base_scale; }

            let force = ((dock_states[i].target_scaled - dock_states[i].current_scaled) * 35) / 100;
            dock_states[i].velocity_scaled = ((dock_states[i].velocity_scaled + force) * 70) / 100;
            dock_states[i].current_scaled += dock_states[i].velocity_scaled;

            if (dock_states[i].current_scaled - dock_states[i].target_scaled).abs() > 30 || dock_states[i].velocity_scaled.abs() > 10 { dock_animating = true; }
            cur_accum_x += item_sz + item_gap;
        }

        if dock_animating { damage.add(Rect { x: (dock_x - 30).max(0) as usize, y: (dock_y - 60).max(0) as usize, w: total_dock_content_w as usize + 60, h: dock_h as usize + 72 }); }

        let current_tick = crate::arch::x86_64::pit::get_ticks();
        if current_tick.saturating_sub(last_clock_tick) >= 1000 {
            damage.add(Rect { x: width.saturating_sub(280), y: 0, w: 280, h: 36 });
            last_clock_tick = current_tick;
        }

        if mouse_moved || click_processed {
            damage.add(prev_mouse_rect);
            prev_mouse_rect = Rect { x: mouse_x as usize, y: mouse_y as usize, w: 24, h: 24 };
            damage.add(prev_mouse_rect);
        }
        
        if elf_running {
            for win in wm.windows.iter_mut() {
                if win.app.title() == "Userspace Desktop" { damage.add(Rect { x: win.x, y: win.y, w: win.w, h: win.h }); break; }
            }
        }
        
        if !damage.get_rects().is_empty() {
            for d_rect in damage.get_rects() {
                let rect = d_rect.clip(width, height);
                for cy in rect.y..rect.y + rect.h {
                    let idx = cy * width + rect.x;
                    fb.pixels[idx..idx + rect.w].fill(theme.desktop_bg);
                }
            }
            
            wm.draw(&mut fb, theme, &damage);
            
            let mb_rect = Rect { x: 0, y: 0, w: width, h: 36 };
            if damage.get_rects().iter().any(|r| r.intersects(&mb_rect)) {
                draw::draw_rect(&mut fb, 0, 0, width, 34, theme.menubar_bg);
                draw::draw_line_h(&mut fb, 0, 34, width, theme.menubar_border);
                draw::draw_text(&mut fb, 18, 10, " EOS", theme.text_main, 1);
                let current_time = crate::drivers::rtc::get_riyadh_time();
                let status_right = alloc::format!("Cores: {} | {:02}:{:02}:{:02}", crate::CORES_ONLINE.load(Ordering::Relaxed), current_time.hour, current_time.minute, current_time.second);
                draw::draw_text(&mut fb, width - 260, 10, &status_right, theme.text_main, 1);
            }
            
            if apple_menu_open {
                draw::draw_rect_rounded(&mut fb, 10, 36, 200, 210, 8, theme.window_bg, 250);
                draw::draw_rect_outline(&mut fb, 10, 36, 200, 210, theme.window_border, 8);
                let items = ["System Settings", "Finder", "Activity Monitor", "Terminal", "-", "Restart..."];
                for (i, txt) in items.iter().enumerate() {
                    if *txt == "-" { draw::draw_line_h(&mut fb, 10, 46 + i * 30, 200, theme.separator); continue; }
                    draw::draw_text(&mut fb, 24, 46 + i * 30, txt, theme.text_main, 1);
                }
            }

            if search_open {
                let sw = 600usize; let sh = 100usize;
                let sx = (width/2).saturating_sub(sw/2); let sy = (height/2).saturating_sub(sh/2);
                draw::draw_rect_rounded(&mut fb, sx, sy, sw, sh, 16, theme.titlebar_bg, 240);
                draw::draw_rect_outline(&mut fb, sx, sy, sw, sh, theme.accent, 16);
                draw::draw_text(&mut fb, sx + 30, sy + 30, "System Search", theme.accent, 2);
                draw::draw_text(&mut fb, sx + 30, sy + 60, &alloc::format!("> {}_", search_query), theme.text_main, 2);
            }

            let dock_bounds = Rect { x: (dock_x - 10).max(0) as usize, y: (dock_y - 50).max(0) as usize, w: total_dock_content_w as usize + 20, h: dock_h as usize + 62 };
            if damage.get_rects().iter().any(|r| r.intersects(&dock_bounds)) {
                draw::draw_rect_rounded(&mut fb, dock_x as usize, dock_y as usize, total_dock_content_w as usize, dock_h as usize, 18, theme.dock_bg, 230);
                draw::draw_rect_outline(&mut fb, dock_x as usize, dock_y as usize, total_dock_content_w as usize, dock_h as usize, theme.dock_border, 18);
                
                let mut icon_x = dock_x + 20;
                for (i, (title, label, col)) in apps_info.iter().enumerate() {
                    let sz = (dock_states[i].current_scaled / 100) as usize;
                    let iy = (dock_y + 12 - (sz as isize - (40 + metrics.scale as isize * 8))).max(0) as usize;
                    draw::draw_rect_rounded(&mut fb, icon_x as usize, iy, sz, sz, sz / 4, *col, 255);
                    if sz >= 45 { draw::draw_text(&mut fb, icon_x as usize + (sz / 4), iy + (sz / 3), label, 0xFFFFFFFF, 0); }
                    if wm.has_window(title) { draw::draw_rect_rounded(&mut fb, icon_x as usize + (sz / 2) - 2, (dock_y + dock_h - 7) as usize, 4, 4, 2, theme.text_main, 255); }
                    icon_x += sz as isize + item_gap;
                }

                if let Some((idx, item_left_x, item_w)) = hovered_item {
                    let name = apps_info[idx].0;
                    let tip_w = (name.len() * 10) + 20;
                    let tip_x = (item_left_x + (item_w as isize / 2) - (tip_w as isize / 2)).max(0) as usize;
                    let tip_y = (dock_y - 36).max(0) as usize;
                    draw::draw_rect_rounded(&mut fb, tip_x, tip_y, tip_w, 26, 6, theme.titlebar_bg, 240);
                    draw::draw_rect_outline(&mut fb, tip_x, tip_y, tip_w, 26, theme.separator, 6);
                    draw::draw_text(&mut fb, tip_x + 10, tip_y + 6, name, theme.text_main, 1);
                }
            }

            draw::draw_cursor(&mut fb, mouse_x as usize, mouse_y as usize);
            
            unsafe {
                let dst_base = fb_ptr as *mut u32;
                for d_rect in damage.get_rects() {
                    let rect = d_rect.clip(width, height);
                    for cy in rect.y..rect.y + rect.h {
                        core::ptr::copy_nonoverlapping(fb.pixels.as_ptr().add(cy * width + rect.x), dst_base.add(cy * (pitch / 4) + rect.x), rect.w);
                    }
                }
            }
            damage.clear();
        }

        let tsc_per_ms = crate::arch::x86_64::pit::TSC_PER_MS.load(Ordering::Relaxed);
        if tsc_per_ms > 0 {
            let elapsed_us = ((crate::arch::x86_64::pit::read_tsc() - frame_start_tsc) * 1000) / tsc_per_ms;
            if elapsed_us < target_frame_us { crate::arch::x86_64::pit::sleep_us(target_frame_us - elapsed_us); }
        } else { unsafe { core::arch::asm!("hlt"); } }
    }
}
