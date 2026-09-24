use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use crate::arch::x86_64::interrupts::GUI_ACTIVE;
use crate::arch::x86_64::syscall::{OVERLAY_ACTIVE, OVERLAY_WIDTH, OVERLAY_HEIGHT, OVERLAY_PIXELS};
use crate::drivers::ata::{save_system_config, load_system_config};
use crate::drivers::gamepad::{
    GAMEPAD_CONNECTED, BTN_MASK, KEYBOARD_NAV_MASK, STICK_X, STICK_Y,
    RAW_BYTES_COUNT, PACKETS_PARSED_COUNT, LAST_RAW_BYTE, pop_editor_char
};
use crate::drivers::rtc::get_riyadh_time;
use crate::writer::WRITER;
use core::ptr::{addr_of, addr_of_mut};
use crate::task::{request_elf_execution, ELF_ACTIVE_RUNNING};
use crate::fs::image::{
    decode_and_display_cached, set_as_wallpaper,
    cleanup_overlay_cache, CUSTOM_WALLPAPER, CUSTOM_WALLPAPER_DIM
};
use crate::fs::{list_directory_contents, vfs_list_all_images, vfs_read_bytes, vfs_save_text_file, FsItem};
use crate::arch::x86_64::pit;
use crate::arch::x86_64::power;
use crate::log_info;

#[derive(Clone)]
pub struct ConsoleCard {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub card_type: CardType,
    pub target_image: &'static str,
    pub color: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CardType {
    Explorer,
    Media,
    Tests,
    Settings,
}

#[derive(PartialEq, Eq)]
enum ViewMode {
    Dashboard,
    FileExplorer,
    FileViewer,
    TextEditor,
    SettingsMenu,
    WallpaperDialog,
    DiagnosticsTest,
    TestsMenu,
    PowerDialog,
}

struct ThemeColor {
    name: &'static str,
    color: u32,
}

pub fn compositor_core_entry() {
    let (fb_ptr, width, height) = unsafe {
        if let Some(writer) = &mut *addr_of_mut!(WRITER) {
            (writer.buffer, writer.width, writer.height)
        } else { return; }
    };

    let total_pixels = width.saturating_mul(height);
    let mut backbuffer: Vec<u32> = vec![0u32; total_pixels];

    let themes = [
        ThemeColor { name: "PS5 Midnight Blue", color: 0xFF1E3A8A },
        ThemeColor { name: "Obsidian Black", color: 0xFF111827 },
        ThemeColor { name: "Emerald Forest", color: 0xFF065F46 },
        ThemeColor { name: "Crimson Velvet", color: 0xFF881337 },
        ThemeColor { name: "Cyberpunk Violet", color: 0xFF581C87 },
    ];
    let mut current_theme_idx = 0usize;

    if let Some((saved_theme, saved_wall)) = load_system_config() {
        current_theme_idx = (saved_theme as usize).min(themes.len() - 1);
        if !saved_wall.is_empty() {
            if let Ok(wall_bytes) = vfs_read_bytes(&saved_wall) {
                let _ = set_as_wallpaper(&wall_bytes);
            }
        }
    }

    let cards = vec![
        ConsoleCard { title: "File Explorer", subtitle: "Browse RootFS, Storage & Initrd", card_type: CardType::Explorer, target_image: "", color: 0xFF0284C7 },
        ConsoleCard { title: "Media Viewer", subtitle: "Open Default Image", card_type: CardType::Media, target_image: "icon.png", color: 0xFF2563EB },
        ConsoleCard { title: "System Tests", subtitle: "Crates & Hardware Diagnostics", card_type: CardType::Tests, target_image: "", color: 0xFFD97706 },
        ConsoleCard { title: "Console Settings", subtitle: "Wallpaper & Custom Themes", card_type: CardType::Settings, target_image: "", color: 0xFF475569 },
    ];

    let mut current_view = ViewMode::Dashboard;
    let mut selected_card = 0;
    let mut selected_settings_idx = 0;
    let mut selected_tests_idx = 0;
    let mut power_focus = false;
    let mut power_selected_action = 0;

    let mut current_folder = String::new();
    let mut explorer_items: Vec<FsItem> = Vec::new();
    let mut explorer_selected_idx = 0;
    let mut explorer_scroll_offset = 0;

    let mut image_files: Vec<String> = Vec::new();
    let mut image_dialog_idx = 0;
    let mut image_dialog_offset = 0;

    let mut active_text_filename = String::new();
    let mut editor_lines: Vec<String> = Vec::new();
    let mut editor_cursor_x = 0usize;
    let mut editor_cursor_y = 0usize;
    let mut editor_save_status = "";

    let mut crates_test_status = "Status: Not Started";
    let mut crates_test_time_ms = 0u64;

    let mut last_nav_tick = pit::get_ticks();
    let mut a_pressed_last = false;
    let mut b_pressed_last = false;
    let mut x_pressed_last = false;
    let mut y_pressed_last = false;

    while GUI_ACTIVE.load(Ordering::SeqCst) {
        while (crate::serial::SERIAL2.read_status() & 1) != 0 {
            let byte = crate::serial::SERIAL2.read_byte();
            crate::drivers::gamepad::process_serial_byte(byte);
        }

        let current_tick = pit::get_ticks();
        
        let btns = BTN_MASK.load(Ordering::Relaxed) | KEYBOARD_NAV_MASK.load(Ordering::Relaxed);
        let x_axis = STICK_X.load(Ordering::Relaxed);
        let y_axis = STICK_Y.load(Ordering::Relaxed);
        
        let left  = (btns & 0x40) != 0 || x_axis < 96;
        let right = (btns & 0x80) != 0 || x_axis > 160;
        let up    = (btns & 0x10) != 0 || y_axis > 160;
        let down  = (btns & 0x20) != 0 || y_axis < 96;

        let a_btn = (btns & 0x01) != 0;
        let b_btn = (btns & 0x02) != 0;
        let x_btn = (btns & 0x04) != 0;
        let y_btn = (btns & 0x08) != 0;

        if current_view == ViewMode::TextEditor {
            while let Some(ch) = pop_editor_char() {
                if ch == b'\x08' {
                    if editor_cursor_x > 0 && !editor_lines[editor_cursor_y].is_empty() {
                        editor_lines[editor_cursor_y].remove(editor_cursor_x - 1);
                        editor_cursor_x -= 1;
                        editor_save_status = "[Modified]";
                    } else if editor_cursor_y > 0 {
                        let prev = editor_lines.remove(editor_cursor_y);
                        editor_cursor_y -= 1;
                        editor_cursor_x = editor_lines[editor_cursor_y].len();
                        editor_lines[editor_cursor_y].push_str(&prev);
                        editor_save_status = "[Modified]";
                    }
                } else if ch == b'\n' {
                    let rest = editor_lines[editor_cursor_y].split_off(editor_cursor_x);
                    editor_lines.insert(editor_cursor_y + 1, rest);
                    editor_cursor_y += 1;
                    editor_cursor_x = 0;
                    editor_save_status = "[Modified]";
                } else if ch >= 32 && ch <= 126 {
                    editor_lines[editor_cursor_y].insert(editor_cursor_x, ch as char);
                    editor_cursor_x += 1;
                    editor_save_status = "[Modified]";
                }
            }
        }

        if OVERLAY_ACTIVE.load(Ordering::Relaxed) {
            if b_btn && !b_pressed_last {
                cleanup_overlay_cache();
                last_nav_tick = current_tick + 20;
            }
        } else {
            match current_view {
                ViewMode::Dashboard => {
                    if current_tick.saturating_sub(last_nav_tick) > 16 {
                        if !power_focus {
                            if up {
                                power_focus = true;
                                log_info!("UI", "Action: Navigation Focus -> [POWER MENU]");
                                last_nav_tick = current_tick;
                            } else if right && selected_card < cards.len() - 1 {
                                selected_card += 1;
                                log_info!("UI", "Action: Focus Card -> [{}]", cards[selected_card].title);
                                last_nav_tick = current_tick;
                            } else if left && selected_card > 0 {
                                selected_card -= 1;
                                log_info!("UI", "Action: Focus Card -> [{}]", cards[selected_card].title);
                                last_nav_tick = current_tick;
                            }
                        } else {
                            if down {
                                power_focus = false;
                                log_info!("UI", "Action: Navigation Focus -> [CARDS ROW]");
                                last_nav_tick = current_tick;
                            }
                        }
                    }

                    if a_btn && !a_pressed_last {
                        if power_focus {
                            current_view = ViewMode::PowerDialog;
                            power_selected_action = 0;
                            log_info!("UI", "Action: Opened [POWER OPTIONS DIALOG]");
                        } else {
                            let card = &cards[selected_card];
                            log_info!("UI", "Action: Selected Card -> [{}]", card.title);
                            match card.card_type {
                                CardType::Explorer => {
                                    current_folder = String::new();
                                    explorer_items = list_directory_contents("");
                                    explorer_selected_idx = 0;
                                    explorer_scroll_offset = 0;
                                    current_view = ViewMode::FileExplorer;
                                }
                                CardType::Media => {
                                    if !card.target_image.is_empty() {
                                        if let Ok(bytes) = vfs_read_bytes(card.target_image) {
                                            let _ = decode_and_display_cached(card.target_image, &bytes, current_tick);
                                        }
                                    }
                                }
                                CardType::Tests => {
                                    selected_tests_idx = 0;
                                    current_view = ViewMode::TestsMenu;
                                }
                                CardType::Settings => {
                                    selected_settings_idx = 0;
                                    current_view = ViewMode::SettingsMenu;
                                }
                            }
                        }
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::TestsMenu => {
                    if current_tick.saturating_sub(last_nav_tick) > 15 {
                        if down && selected_tests_idx < 1 {
                            selected_tests_idx += 1;
                            last_nav_tick = current_tick;
                        } else if up && selected_tests_idx > 0 {
                            selected_tests_idx -= 1;
                            last_nav_tick = current_tick;
                        }
                    }

                    if a_btn && !a_pressed_last {
                        if selected_tests_idx == 0 {
                            let t_before = pit::read_tsc();
                            if let Ok(()) = request_elf_execution("app.elf", "test") {
                                log_info!("TESTS", "Dispatched Crates Test Suite to Core 2");
                                crates_test_status = "Status: Tests Running in Background";
                            } else {
                                crates_test_status = "Status: Execution Error (Busy/Missing)";
                            }
                            crates_test_time_ms = pit::read_tsc().saturating_sub(t_before);
                        } else {
                            current_view = ViewMode::DiagnosticsTest;
                        }
                        last_nav_tick = current_tick + 20;
                    }

                    if b_btn && !b_pressed_last {
                        current_view = ViewMode::Dashboard;
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::PowerDialog => {
                    if current_tick.saturating_sub(last_nav_tick) > 16 {
                        if (right || down) && power_selected_action == 0 {
                            power_selected_action = 1;
                            log_info!("POWER", "Selected -> [SHUTDOWN]");
                            last_nav_tick = current_tick;
                        } else if (left || up) && power_selected_action == 1 {
                            power_selected_action = 0;
                            log_info!("POWER", "Selected -> [RESTART]");
                            last_nav_tick = current_tick;
                        }
                    }

                    if a_btn && !a_pressed_last {
                        if power_selected_action == 0 {
                            log_info!("POWER", "Executing -> Immediate Reboot");
                            power::reboot();
                        } else {
                            log_info!("POWER", "Executing -> Immediate Shutdown");
                            power::shutdown();
                        }
                    }

                    if b_btn && !b_pressed_last {
                        current_view = ViewMode::Dashboard;
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::DiagnosticsTest => {
                    if b_btn && !b_pressed_last {
                        current_view = ViewMode::TestsMenu;
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::SettingsMenu => {
                    let total_options = themes.len() + 2;
                    if current_tick.saturating_sub(last_nav_tick) > 15 {
                        if down && selected_settings_idx + 1 < total_options {
                            selected_settings_idx += 1;
                            last_nav_tick = current_tick;
                        } else if up && selected_settings_idx > 0 {
                            selected_settings_idx -= 1;
                            last_nav_tick = current_tick;
                        }
                    }

                    if a_btn && !a_pressed_last {
                        if selected_settings_idx < themes.len() {
                            current_theme_idx = selected_settings_idx;
                            unsafe { *addr_of_mut!(CUSTOM_WALLPAPER) = None; }
                            let _ = save_system_config(current_theme_idx as u8, "");
                            log_info!("SETTINGS", "Applied Theme: '{}'", themes[current_theme_idx].name);
                        } else if selected_settings_idx == themes.len() {
                            image_files = vfs_list_all_images();
                            image_dialog_idx = 0;
                            image_dialog_offset = 0;
                            current_view = ViewMode::WallpaperDialog;
                        } else {
                            unsafe { *addr_of_mut!(CUSTOM_WALLPAPER) = None; }
                            let _ = save_system_config(current_theme_idx as u8, "");
                            log_info!("SETTINGS", "Reset Wallpaper to Default");
                        }
                        last_nav_tick = current_tick + 20;
                    }

                    if b_btn && !b_pressed_last {
                        current_view = ViewMode::Dashboard;
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::WallpaperDialog => {
                    if current_tick.saturating_sub(last_nav_tick) > 15 {
                        if down && !image_files.is_empty() && image_dialog_idx + 1 < image_files.len() {
                            image_dialog_idx += 1;
                            if image_dialog_idx >= image_dialog_offset + 9 {
                                image_dialog_offset += 1;
                            }
                            last_nav_tick = current_tick;
                        } else if up && image_dialog_idx > 0 {
                            image_dialog_idx -= 1;
                            if image_dialog_idx < image_dialog_offset {
                                image_dialog_offset = image_dialog_offset.saturating_sub(1);
                            }
                            last_nav_tick = current_tick;
                        }
                    }

                    if a_btn && !a_pressed_last && !image_files.is_empty() {
                        let selected_img = &image_files[image_dialog_idx];
                        if let Ok(bytes) = vfs_read_bytes(selected_img) {
                            if set_as_wallpaper(&bytes).is_ok() {
                                let _ = save_system_config(current_theme_idx as u8, selected_img);
                                log_info!("SETTINGS", "Custom Wallpaper Set & Saved: '{}'", selected_img);
                            }
                        }
                        current_view = ViewMode::SettingsMenu;
                        last_nav_tick = current_tick + 20;
                    }

                    if b_btn && !b_pressed_last {
                        current_view = ViewMode::SettingsMenu;
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::FileExplorer => {
                    let total_rows = if current_folder.is_empty() { explorer_items.len() } else { explorer_items.len() + 1 };

                    if current_tick.saturating_sub(last_nav_tick) > 15 {
                        if down && explorer_selected_idx + 1 < total_rows {
                            explorer_selected_idx += 1;
                            if explorer_selected_idx >= explorer_scroll_offset + 12 {
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
                        if !current_folder.is_empty() {
                            current_folder = String::new();
                            explorer_items = list_directory_contents("");
                            explorer_selected_idx = 0;
                            explorer_scroll_offset = 0;
                            log_info!("VFS", "Navigated up to Root Directory");
                        } else {
                            current_view = ViewMode::Dashboard;
                        }
                        last_nav_tick = current_tick + 20;
                    }

                    if a_btn && !a_pressed_last {
                        if !current_folder.is_empty() && explorer_selected_idx == 0 {
                            current_folder = String::new();
                            explorer_items = list_directory_contents("");
                            explorer_selected_idx = 0;
                            explorer_scroll_offset = 0;
                            log_info!("VFS", "Navigated up to Root Directory");
                        } else {
                            let actual_idx = if current_folder.is_empty() { explorer_selected_idx } else { explorer_selected_idx - 1 };
                            if actual_idx < explorer_items.len() {
                                let selected_item = explorer_items[actual_idx].clone();
                                match selected_item {
                                    FsItem::Directory(dirname) => {
                                        current_folder = dirname.clone();
                                        explorer_items = list_directory_contents(dirname.as_str());
                                        explorer_selected_idx = 0;
                                        explorer_scroll_offset = 0;
                                        log_info!("VFS", "Entered Folder: [{}]", dirname);
                                    }
                                    FsItem::File(filename, _) => {
                                        let name_lower = filename.to_ascii_lowercase();
                                        log_info!("VFS", "Opening Item: '{}'", filename);
                                        if name_lower.ends_with(".png") || name_lower.ends_with(".jpg") || name_lower.ends_with(".jpeg") {
                                            if let Ok(bytes) = vfs_read_bytes(&filename) {
                                                let _ = decode_and_display_cached(&filename, &bytes, current_tick);
                                            }
                                        } else if name_lower.ends_with(".elf") {
                                            let _ = request_elf_execution(&filename, "");
                                        } else {
                                            if let Ok(bytes) = vfs_read_bytes(&filename) {
                                                active_text_filename = filename;
                                                editor_save_status = "";
                                                if let Ok(text) = core::str::from_utf8(&bytes) {
                                                    editor_lines = text.lines().map(String::from).collect();
                                                } else {
                                                    editor_lines = vec![String::from("")];
                                                }
                                                if editor_lines.is_empty() { editor_lines.push(String::new()); }
                                                editor_cursor_x = 0;
                                                editor_cursor_y = 0;
                                                current_view = ViewMode::FileViewer;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::FileViewer => {
                    if x_btn && !x_pressed_last {
                        current_view = ViewMode::TextEditor;
                        editor_save_status = "[Editing Active]";
                        log_info!("EDITOR", "Entered Editor Mode for file: '{}'", active_text_filename);
                        last_nav_tick = current_tick + 20;
                    }

                    if b_btn && !b_pressed_last {
                        current_view = ViewMode::FileExplorer;
                        last_nav_tick = current_tick + 20;
                    }
                }

                ViewMode::TextEditor => {
                    if current_tick.saturating_sub(last_nav_tick) > 14 {
                        if down && editor_cursor_y + 1 < editor_lines.len() {
                            editor_cursor_y += 1;
                            editor_cursor_x = core::cmp::min(editor_cursor_x, editor_lines[editor_cursor_y].len());
                            last_nav_tick = current_tick;
                        } else if up && editor_cursor_y > 0 {
                            editor_cursor_y -= 1;
                            editor_cursor_x = core::cmp::min(editor_cursor_x, editor_lines[editor_cursor_y].len());
                            last_nav_tick = current_tick;
                        } else if left && editor_cursor_x > 0 {
                            editor_cursor_x -= 1;
                            last_nav_tick = current_tick;
                        } else if right && editor_cursor_x < editor_lines[editor_cursor_y].len() {
                            editor_cursor_x += 1;
                            last_nav_tick = current_tick;
                        }
                    }

                    if y_btn && !y_pressed_last {
                        let mut full = String::new();
                        for l in &editor_lines {
                            full.push_str(l);
                            full.push_str("\r\n");
                        }
                        if vfs_save_text_file(&active_text_filename, full.as_bytes()).is_ok() {
                            editor_save_status = "[Saved Successfully!]";
                            log_info!("EDITOR", "Saved changes to '{}' ({} bytes)", active_text_filename, full.len());
                        } else {
                            editor_save_status = "[Save Failed: Read-only]";
                            crate::log_error!("EDITOR", "Save failed for '{}'", active_text_filename);
                        }
                        last_nav_tick = current_tick + 20;
                    }

                    if b_btn && !b_pressed_last {
                        current_view = ViewMode::FileViewer;
                        log_info!("EDITOR", "Exited Editor Mode.");
                        last_nav_tick = current_tick + 20;
                    }
                }
            }
        }

        a_pressed_last = a_btn;
        b_pressed_last = b_btn;
        x_pressed_last = x_btn;
        y_pressed_last = y_btn;

        let is_running_elf = ELF_ACTIVE_RUNNING.load(Ordering::Relaxed);
        let active_bg_color = themes[current_theme_idx].color;

        draw_desktop_wallpaper(&mut backbuffer, width, height, active_bg_color);

        match current_view {
            ViewMode::Dashboard => {
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, "DASHBOARD", power_focus);

                let card_w = 340;
                let card_h = 340;
                let gap = 50;
                let start_x = (width / 2).saturating_sub((card_w / 2) + selected_card * (card_w + gap));
                let base_y = height / 2 - (card_h / 2) - 30;

                for (i, card) in cards.iter().enumerate() {
                    let cx = start_x as isize + (i * (card_w + gap)) as isize;
                    if cx < -400 || cx > width as isize { continue; }
                    let x = cx as usize;
                    
                    let is_focused = !power_focus && i == selected_card;
                    let (y, h, w) = if is_focused {
                        (base_y.saturating_sub(25), card_h + 50, card_w + 50)
                    } else {
                        (base_y + 25, card_h, card_w)
                    };

                    draw_card(&mut backbuffer, width, height, x, y, w, h, card.color, is_focused);
                }

                let focused = &cards[selected_card];
                draw_text_centered(&mut backbuffer, width, height, width / 2, base_y + card_h + 90, focused.title, 0xFFFFFFFF, 4);
                draw_text_centered(&mut backbuffer, width, height, width / 2, base_y + card_h + 150, focused.subtitle, 0xFFA0AEC0, 2);

                let hint = if power_focus { "[A / Enter] Open Power Menu" } else { "[A / Enter] Select   |   [Up] Power Menu" };
                draw_bottom_bar(&mut backbuffer, width, height, hint, "[B / Esc] Exit");
            }

            ViewMode::TestsMenu => {
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, "SYSTEM & CRATES TEST SUITE", false);

                let list_x = 240;
                let list_y = 180;
                let row_h = 75;
                let list_w = width - 480;

                let is_opt0_focused = selected_tests_idx == 0;
                let is_opt1_focused = selected_tests_idx == 1;

                draw_settings_row(&mut backbuffer, width, height, list_x, list_y, list_w, row_h - 12, "Run Rust Crates Test Suite (serde, sha2, rand, regex)", 0xFF10B981, is_opt0_focused, is_running_elf);
                draw_settings_row(&mut backbuffer, width, height, list_x, list_y + row_h, list_w, row_h - 12, "Hardware & Controller Diagnostics (COM2/Serial/Sensors)", 0xFF38BDF8, is_opt1_focused, false);

                let stat_box_y = list_y + (row_h * 2) + 20;
                let stat_box_h = 140;
                for y in stat_box_y..stat_box_y + stat_box_h {
                    let r = y * width;
                    for x in list_x..list_x + list_w {
                        let is_b = x == list_x || x == list_x + list_w - 1 || y == stat_box_y || y == stat_box_y + stat_box_h - 1;
                        buf_pixel(&mut backbuffer, r, x, if is_b { 0xFF64748B } else { 0xFF0F172A });
                    }
                }

                draw_text_scaled(&mut backbuffer, width, height, list_x + 25, stat_box_y + 20, "LATEST TEST EXECUTION LOG:", 0xFFFDE047, 2);
                draw_text_scaled(&mut backbuffer, width, height, list_x + 25, stat_box_y + 55, crates_test_status, if is_running_elf { 0xFFF59E0B } else { 0xFF4ADE80 }, 2);
                
                if crates_test_time_ms > 0 {
                    draw_text_scaled(&mut backbuffer, width, height, list_x + 25, stat_box_y + 90, "Check Terminal / UART Output (COM1) for complete test traces.", 0xFF94A3B8, 2);
                } else {
                    draw_text_scaled(&mut backbuffer, width, height, list_x + 25, stat_box_y + 90, "Press [A / Enter] to execute test suite on dedicated Core #2.", 0xFF94A3B8, 2);
                }

                draw_bottom_bar(&mut backbuffer, width, height, "[A / Enter] Run Test", "[B / Esc] Back");
            }

            ViewMode::PowerDialog => {
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, "POWER OPTIONS", true);
                draw_power_modal(&mut backbuffer, width, height, power_selected_action);
                draw_bottom_bar(&mut backbuffer, width, height, "[A / Enter] Confirm Action", "[B / Esc] Cancel");
            }

            ViewMode::DiagnosticsTest => {
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, "HARDWARE & CONTROLLER DIAGNOSTICS", false);

                let box_x = 180;
                let box_y = 160;
                let box_w = width - 360;
                let box_h = height - 280;

                for y in box_y..box_y + box_h {
                    let row = y * width;
                    for x in box_x..box_x + box_w {
                        let is_b = x == box_x || x == box_x + box_w - 1 || y == box_y || y == box_y + box_h - 1;
                        buf_pixel(&mut backbuffer, row, x, if is_b { 0xFF38BDF8 } else { 0xFF0F172A });
                    }
                }

                let lsr = crate::serial::SERIAL2.read_status();
                let raw_cnt = RAW_BYTES_COUNT.load(Ordering::Relaxed);
                let packets_cnt = PACKETS_PARSED_COUNT.load(Ordering::Relaxed);
                let last_b = LAST_RAW_BYTE.load(Ordering::Relaxed);
                let btn = BTN_MASK.load(Ordering::Relaxed);
                let sx = STICK_X.load(Ordering::Relaxed);
                let sy = STICK_Y.load(Ordering::Relaxed);

                draw_text_scaled(&mut backbuffer, width, height, box_x + 30, box_y + 30, "=== HARDWARE SERIAL PORT (COM2 - 0x2F8) ===", 0xFFFDE047, 2);
                
                let mut line1 = [0u8; 64];
                let l1_len = format_diag_1(&mut line1, lsr as usize, last_b as usize);
                if let Ok(s) = core::str::from_utf8(&line1[..l1_len]) {
                    draw_text_scaled(&mut backbuffer, width, height, box_x + 30, box_y + 70, s, 0xFFFFFFFF, 2);
                }

                let mut line2 = [0u8; 64];
                let l2_len = format_diag_2(&mut line2, raw_cnt as usize, packets_cnt as usize);
                if let Ok(s) = core::str::from_utf8(&line2[..l2_len]) {
                    draw_text_scaled(&mut backbuffer, width, height, box_x + 30, box_y + 110, s, 0xFF38BDF8, 2);
                }

                draw_text_scaled(&mut backbuffer, width, height, box_x + 30, box_y + 170, "=== CONTROLLER REAL-TIME STATE ===", 0xFFFDE047, 2);

                let a = (btn & 0x01) != 0;
                let b = (btn & 0x02) != 0;
                let x_b = (btn & 0x04) != 0;
                let y_b = (btn & 0x08) != 0;
                let up_b = (btn & 0x10) != 0;
                let dn_b = (btn & 0x20) != 0;
                let lt_b = (btn & 0x40) != 0;
                let rt_b = (btn & 0x80) != 0;

                let mut line3 = [0u8; 64];
                let l3_len = format_diag_buttons(&mut line3, up_b, dn_b, lt_b, rt_b);
                if let Ok(s) = core::str::from_utf8(&line3[..l3_len]) {
                    draw_text_scaled(&mut backbuffer, width, height, box_x + 30, box_y + 210, s, 0xFF4ADE80, 2);
                }

                let mut line4 = [0u8; 64];
                let l4_len = format_diag_actions(&mut line4, a, b, x_b, y_b);
                if let Ok(s) = core::str::from_utf8(&line4[..l4_len]) {
                    draw_text_scaled(&mut backbuffer, width, height, box_x + 30, box_y + 250, s, 0xFF4ADE80, 2);
                }

                let mut line5 = [0u8; 64];
                let l5_len = format_diag_sticks(&mut line5, sx as usize, sy as usize);
                if let Ok(s) = core::str::from_utf8(&line5[..l5_len]) {
                    draw_text_scaled(&mut backbuffer, width, height, box_x + 30, box_y + 290, s, 0xFFCBD5E1, 2);
                }

                draw_text_scaled(&mut backbuffer, width, height, box_x + 30, box_y + 360, "Tip: Use Left Analog or D-Pad. Values reach 0..255 fully.", 0xFF94A3B8, 2);
                draw_bottom_bar(&mut backbuffer, width, height, "", "[B / Esc] Return to Tests Menu");
            }

            ViewMode::SettingsMenu => {
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, "SETTINGS - WALLPAPER & THEMES", false);

                let list_x = 240;
                let list_y = 180;
                let row_h = 65;
                let list_w = width - 480;

                for (i, theme) in themes.iter().enumerate() {
                    let y = list_y + (i * row_h);
                    let is_focused = i == selected_settings_idx;
                    let is_active = i == current_theme_idx && unsafe { (*addr_of_mut!(CUSTOM_WALLPAPER)).is_none() };
                    draw_settings_row(&mut backbuffer, width, height, list_x, y, list_w, row_h - 12, theme.name, theme.color, is_focused, is_active);
                }

                let custom_y = list_y + (themes.len() * row_h);
                let is_custom_focused = selected_settings_idx == themes.len();
                let is_custom_active = unsafe { (*addr_of_mut!(CUSTOM_WALLPAPER)).is_some() };
                draw_settings_row(&mut backbuffer, width, height, list_x, custom_y, list_w, row_h - 12, "Select Wallpaper from Disk/Share...", 0xFF38BDF8, is_custom_focused, is_custom_active);

                let reset_y = custom_y + row_h;
                let is_reset_focused = selected_settings_idx == themes.len() + 1;
                draw_settings_row(&mut backbuffer, width, height, list_x, reset_y, list_w, row_h - 12, "Reset to Default Theme", 0xFFEF4444, is_reset_focused, false);

                draw_bottom_bar(&mut backbuffer, width, height, "[A / Enter] Select", "[B / Esc] Back");
            }

            ViewMode::WallpaperDialog => {
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, "CHOOSE WALLPAPER IMAGE", false);

                let list_x = 200;
                let list_y = 160;
                let row_h = 58;
                let list_w = width - 400;

                let visible_count = 9;
                for i in 0..visible_count {
                    let idx = image_dialog_offset + i;
                    if idx >= image_files.len() { break; }
                    let img_name = &image_files[idx];
                    let is_focused = idx == image_dialog_idx;
                    let y = list_y + (i * row_h);
                    draw_dialog_row(&mut backbuffer, width, height, list_x, y, list_w, row_h - 10, img_name.as_str(), is_focused);
                }

                draw_bottom_bar(&mut backbuffer, width, height, "[A / Enter] Set as Wallpaper", "[B / Esc] Cancel");
            }

            ViewMode::FileExplorer => {
                let title_str = if current_folder.is_empty() {
                    String::from("FILE EXPLORER (ROOT)")
                } else {
                    alloc::format!("FILE EXPLORER [{}]", current_folder)
                };
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, title_str.as_str(), false);

                let list_x = 180;
                let list_y = 160;
                let row_h = 60;
                let list_w = width - 360;

                let visible_count = 12;
                let total_rows = if current_folder.is_empty() { explorer_items.len() } else { explorer_items.len() + 1 };

                for i in 0..visible_count {
                    let row_idx = explorer_scroll_offset + i;
                    if row_idx >= total_rows { break; }
                    let y = list_y + (i * row_h);
                    let is_focused = row_idx == explorer_selected_idx;

                    if !current_folder.is_empty() && row_idx == 0 {
                        draw_parent_dir_row(&mut backbuffer, width, height, list_x, y, list_w, row_h - 10, is_focused);
                    } else {
                        let item_idx = if current_folder.is_empty() { row_idx } else { row_idx - 1 };
                        let item = &explorer_items[item_idx];
                        draw_explorer_item_row(&mut backbuffer, width, height, list_x, y, list_w, row_h - 10, item, is_focused);
                    }
                }

                draw_bottom_bar(&mut backbuffer, width, height, "[A / Enter] Open Folder / File", "[B / Esc] Up / Dashboard");
            }

            ViewMode::FileViewer | ViewMode::TextEditor => {
                let mode_title = if current_view == ViewMode::TextEditor { "TEXT EDITOR" } else { "FILE VIEWER" };
                draw_top_bar(&mut backbuffer, width, height, GAMEPAD_CONNECTED.load(Ordering::Relaxed), is_running_elf, mode_title, false);

                draw_text_scaled(&mut backbuffer, width, height, 80, 140, active_text_filename.as_str(), 0xFF38BDF8, 3);
                if !editor_save_status.is_empty() {
                    draw_text_scaled(&mut backbuffer, width, height, 600, 145, editor_save_status, 0xFF4ADE80, 2);
                }

                let start_y = 200;
                let max_lines = 22;
                for (idx, line) in editor_lines.iter().take(max_lines).enumerate() {
                    let y = start_y + (idx * 34);
                    draw_text_scaled(&mut backbuffer, width, height, 80, y, line.as_str(), 0xFFF1F5F9, 2);

                    if current_view == ViewMode::TextEditor && idx == editor_cursor_y {
                        let cx = 80 + (editor_cursor_x * 9 * 2);
                        for cy in y..y + 30 {
                            buf_pixel(&mut backbuffer, cy * width, cx, 0xFF38BDF8);
                        }
                    }
                }

                if current_view == ViewMode::TextEditor {
                    draw_bottom_bar(&mut backbuffer, width, height, "[Y / Ctrl+S] Save File", "[B / Esc] Exit Edit");
                } else {
                    draw_bottom_bar(&mut backbuffer, width, height, "[X / E] Edit File", "[B / Esc] Return");
                }
            }
        }

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

fn draw_parent_dir_row(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, focused: bool) {
    let bg_color = if focused { 0xFF2563EB } else { 0xFF1E293B };
    for cy in y..(y+h).min(fb_h) {
        let row = cy * fb_w;
        for cx in x..(x+w).min(fb_w) {
            let is_b = cx == x || cx == x + w - 1 || cy == y || cy == y + h - 1;
            buf[row + cx] = if focused && is_b { 0xFFFFFFFF } else { bg_color };
        }
    }
    draw_text_scaled(buf, fb_w, fb_h, x + 25, y + 16, "[DIR]", 0xFFF59E0B, 2);
    draw_text_scaled(buf, fb_w, fb_h, x + 120, y + 16, ".. [Parent Directory]", 0xFFFFFFFF, 2);
}

fn draw_explorer_item_row(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, item: &FsItem, focused: bool) {
    let bg_color = if focused { 0xFF2563EB } else { 0xFF1E293B };
    for cy in y..(y+h).min(fb_h) {
        let row = cy * fb_w;
        for cx in x..(x+w).min(fb_w) {
            let is_b = cx == x || cx == x + w - 1 || cy == y || cy == y + h - 1;
            buf[row + cx] = if focused && is_b { 0xFFFFFFFF } else { bg_color };
        }
    }

    match item {
        FsItem::Directory(dirname) => {
            draw_text_scaled(buf, fb_w, fb_h, x + 25, y + 16, "[DIR]", 0xFFF59E0B, 2);
            draw_text_scaled(buf, fb_w, fb_h, x + 120, y + 16, dirname.as_str(), 0xFFFFFFFF, 2);
            draw_text_scaled(buf, fb_w, fb_h, x + w - 180, y + 16, "<Folder>", 0xFF94A3B8, 2);
        }
        FsItem::File(filename, size) => {
            let lower = filename.to_ascii_lowercase();
            let (icon, icon_col) = if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                ("[IMG]", 0xFFFDE047)
            } else if lower.ends_with(".elf") {
                ("[BIN]", 0xFF4ADE80)
            } else {
                ("[TXT]", 0xFF60A5FA)
            };

            draw_text_scaled(buf, fb_w, fb_h, x + 25, y + 16, icon, icon_col, 2);
            draw_text_scaled(buf, fb_w, fb_h, x + 120, y + 16, filename.as_str(), 0xFFFFFFFF, 2);

            let mut sz_buf = [0u8; 16];
            let sz_len = if *size >= 1024 * 1024 {
                let mb = *size / (1024 * 1024);
                let dec = (*size % (1024 * 1024)) / 100_000;
                format_size_slice(&mut sz_buf, mb, dec, "MB")
            } else if *size >= 1024 {
                let kb = *size / 1024;
                format_size_slice(&mut sz_buf, kb, 0, "KB")
            } else {
                format_size_slice(&mut sz_buf, *size, 0, "B")
            };

            if let Ok(s) = core::str::from_utf8(&sz_buf[..sz_len]) {
                draw_text_scaled(buf, fb_w, fb_h, x + w - 180, y + 16, s, 0xFFCBD5E1, 2);
            }
        }
    }
}

fn draw_power_modal(buf: &mut [u32], fb_w: usize, fb_h: usize, selected: usize) {
    let win_w = 560;
    let win_h = 240;
    let win_x = (fb_w - win_w) / 2;
    let win_y = (fb_h - win_h) / 2;

    for y in 0..fb_h {
        let r = y * fb_w;
        for x in 0..fb_w {
            let p = buf[r + x];
            let cr = ((p >> 16) & 0xFF) / 4;
            let cg = ((p >> 8) & 0xFF) / 4;
            let cb = (p & 0xFF) / 4;
            buf[r + x] = (0xFF << 24) | (cr << 16) | (cg << 8) | cb;
        }
    }

    for y in win_y..win_y + win_h {
        let row = y * fb_w;
        for x in win_x..win_x + win_w {
            let is_b = x == win_x || x == win_x + win_w - 1 || y == win_y || y == win_y + win_h - 1;
            let is_h = y < win_y + 50;
            buf[row + x] = if is_b { 0xFFEF4444 } else if is_h { 0xFF1E293B } else { 0xFF0F172A };
        }
    }

    draw_text_scaled(buf, fb_w, fb_h, win_x + 30, win_y + 16, "POWER OPTIONS", 0xFFFFFFFF, 2);

    let opt_w = 230;
    let opt_h = 90;
    let opt_y = win_y + 90;

    let r_x = win_x + 30;
    let s_x = win_x + 300;

    let r_foc = selected == 0;
    draw_power_card(buf, fb_w, fb_h, r_x, opt_y, opt_w, opt_h, "RESTART", 0xFF2563EB, r_foc);

    let s_foc = selected == 1;
    draw_power_card(buf, fb_w, fb_h, s_x, opt_y, opt_w, opt_h, "SHUTDOWN", 0xFFDC2626, s_foc);
}

fn draw_power_card(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, title: &str, col: u32, foc: bool) {
    for cy in y..y + h {
        let row = cy * fb_w;
        for cx in x..x + w {
            let is_b = cx == x || cx == x + w - 1 || cy == y || cy == y + h - 1;
            buf[row + cx] = if foc && is_b { 0xFFFFFFFF } else if is_b { 0xFF475569 } else { col };
        }
    }
    draw_text_scaled(buf, fb_w, fb_h, x + 45, y + 35, title, 0xFFFFFFFF, 2);
}

#[inline(always)]
fn buf_pixel(buf: &mut [u32], row: usize, x: usize, col: u32) {
    buf[row + x] = col;
}

fn format_diag_1(buf: &mut [u8; 64], lsr: usize, last: usize) -> usize {
    let mut i = 0;
    for b in b"UART Status (LSR): 0x".iter() { buf[i] = *b; i += 1; }
    i += put_hex(buf, i, lsr);
    for b in b"   |   Last Byte: 0x".iter() { buf[i] = *b; i += 1; }
    i += put_hex(buf, i, last);
    i
}

fn format_diag_2(buf: &mut [u8; 64], raw: usize, pkts: usize) -> usize {
    let mut i = 0;
    for b in b"Raw Bytes Received: ".iter() { buf[i] = *b; i += 1; }
    i += put_num_dynamic(buf, i, raw);
    for b in b"   |   Valid Packets: ".iter() { buf[i] = *b; i += 1; }
    i += put_num_dynamic(buf, i, pkts);
    i
}

fn format_diag_buttons(buf: &mut [u8; 64], u: bool, d: bool, l: bool, r: bool) -> usize {
    let mut i = 0;
    for b in b"D-Pad:   UP:".iter() { buf[i] = *b; i += 1; }
    buf[i] = if u { b'1' } else { b'0' }; i += 1;
    for b in b"   DOWN:".iter() { buf[i] = *b; i += 1; }
    buf[i] = if d { b'1' } else { b'0' }; i += 1;
    for b in b"   LEFT:".iter() { buf[i] = *b; i += 1; }
    buf[i] = if l { b'1' } else { b'0' }; i += 1;
    for b in b"   RIGHT:".iter() { buf[i] = *b; i += 1; }
    buf[i] = if r { b'1' } else { b'0' }; i += 1;
    i
}

fn format_diag_actions(buf: &mut [u8; 64], a: bool, b: bool, x: bool, y: bool) -> usize {
    let mut i = 0;
    for b in b"Action: A:".iter() { buf[i] = *b; i += 1; }
    buf[i] = if a { b'1' } else { b'0' }; i += 1;
    for b in b"   B:".iter() { buf[i] = *b; i += 1; }
    buf[i] = if b { b'1' } else { b'0' }; i += 1;
    for b in b"   X:".iter() { buf[i] = *b; i += 1; }
    buf[i] = if x { b'1' } else { b'0' }; i += 1;
    for b in b"   Y:".iter() { buf[i] = *b; i += 1; }
    buf[i] = if y { b'1' } else { b'0' }; i += 1;
    i
}

fn format_diag_sticks(buf: &mut [u8; 64], sx: usize, sy: usize) -> usize {
    let mut i = 0;
    for b in b"Left Analog Stick:   X: ".iter() { buf[i] = *b; i += 1; }
    i += put_num_dynamic(buf, i, sx);
    for b in b"   |   Y: ".iter() { buf[i] = *b; i += 1; }
    i += put_num_dynamic(buf, i, sy);
    i
}

fn put_hex(buf: &mut [u8; 64], start: usize, val: usize) -> usize {
    let hex_chars = b"0123456789ABCDEF";
    buf[start] = hex_chars[(val >> 4) & 0xF];
    buf[start + 1] = hex_chars[val & 0xF];
    2
}

fn draw_desktop_wallpaper(buf: &mut [u32], fb_w: usize, fb_h: usize, tint: u32) {
    unsafe {
        if let Some(wall) = &*addr_of_mut!(CUSTOM_WALLPAPER) {
            let (w, h) = *addr_of_mut!(CUSTOM_WALLPAPER_DIM);
            if w > 0 && h > 0 {
                for y in 0..fb_h {
                    let sy = (y * h) / fb_h;
                    let dst_row = y * fb_w;
                    let src_row = sy * w;
                    for x in 0..fb_w {
                        let sx = (x * w) / fb_w;
                        buf[dst_row + x] = wall[src_row + sx];
                    }
                }
                return;
            }
        }
    }

    let tr = (tint >> 16) & 0xFF;
    let tg = (tint >> 8) & 0xFF;
    let tb = tint & 0xFF;
    for y in 0..fb_h {
        let row = y * fb_w;
        let factor = (y * 255) / fb_h; 
        let r = (6 + (factor * tr as usize / 600)) as u32;
        let g = (10 + (factor * tg as usize / 600)) as u32;
        let b = (16 + (factor * tb as usize / 600)) as u32;
        let color = (0xFF << 24) | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
        for x in 0..fb_w { buf[row + x] = color; }
    }
}

fn draw_top_bar(buf: &mut [u32], fb_w: usize, fb_h: usize, connected: bool, running_elf: bool, title: &str, power_focused: bool) {
    draw_text_scaled(buf, fb_w, fb_h, 60, 45, "EOS CONSOLE", 0xFFFFFFFF, 3);
    draw_text_scaled(buf, fb_w, fb_h, 320, 47, "|", 0xFF64748B, 3);
    draw_text_scaled(buf, fb_w, fb_h, 360, 47, title, 0xFF94A3B8, 3);

    let p_x = fb_w - 740;
    let p_y = 38;
    let (p_bg, p_txt) = if power_focused { (0xFFEF4444, 0xFFFFFFFF) } else { (0xFF1E293B, 0xFFCBD5E1) };
    for cy in p_y..p_y + 36 {
        let r = cy * fb_w;
        for cx in p_x..p_x + 130 {
            buf[r + cx] = p_bg;
        }
    }
    draw_text_scaled(buf, fb_w, fb_h, p_x + 15, p_y + 8, "[POWER]", p_txt, 2);

    let dt = get_riyadh_time();
    let mut time_str = [0u8; 32];
    let time_len = format_24h_slice(&mut time_str, dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second);
    if let Ok(s) = core::str::from_utf8(&time_str[..time_len]) {
        draw_text_scaled(buf, fb_w, fb_h, fb_w - 560, 47, s, 0xFFF8FAFC, 2);
    }

    let status = if connected { "[GameSir: Connected]" } else { "[Kbd / Gamepad Active]" };
    let color = if connected { 0xFF34D399 } else { 0xFF38BDF8 };
    draw_text_scaled(buf, fb_w, fb_h, fb_w - 1000, 52, status, color, 1);

    if running_elf {
        draw_text_scaled(buf, fb_w, fb_h, fb_w - 1230, 52, "[Core 2: Processing]", 0xFFF59E0B, 1);
    }
}

fn format_24h_slice(buf: &mut [u8; 32], y: u16, mon: u8, d: u8, h: u8, m: u8, s: u8) -> usize {
    let mut i = 0;
    i += put_num(buf, i, y as usize, 4);
    buf[i] = b'-'; i += 1;
    i += put_num(buf, i, mon as usize, 2);
    buf[i] = b'-'; i += 1;
    i += put_num(buf, i, d as usize, 2);
    buf[i] = b' '; i += 1;
    buf[i] = b' '; i += 1;
    i += put_num(buf, i, h as usize, 2);
    buf[i] = b':'; i += 1;
    i += put_num(buf, i, m as usize, 2);
    buf[i] = b':'; i += 1;
    i += put_num(buf, i, s as usize, 2);
    i
}

fn put_num(buf: &mut [u8; 32], start: usize, mut n: usize, digits: usize) -> usize {
    for d in (0..digits).rev() {
        buf[start + d] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    digits
}

fn draw_floating_modal(buf: &mut [u32], fb_w: usize, fb_h: usize, img_w: usize, img_h: usize) {
    let win_w = img_w + 40;
    let win_h = img_h + 80;
    let win_x = (fb_w.saturating_sub(win_w)) / 2;
    let win_y = (fb_h.saturating_sub(win_h)) / 2;

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

    draw_text_scaled(buf, fb_w, fb_h, win_x + 20, win_y + 12, "IMAGE VIEWER (PNG / JPEG)", 0xFFFFFFFF, 2);
    draw_text_scaled(buf, fb_w, fb_h, win_x + win_w - 140, win_y + 14, "[B / Esc]", 0xFFF87171, 2);

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

fn draw_bottom_bar(buf: &mut [u32], fb_w: usize, fb_h: usize, a_action: &str, b_action: &str) {
    let y = fb_h - 80;
    for cy in y..fb_h {
        let row = cy * fb_w;
        for cx in 0..fb_w { buf[row + cx] = 0xFF090D16; }
    }
    if !a_action.is_empty() { draw_text_scaled(buf, fb_w, fb_h, 80, y + 26, a_action, 0xFFFFFFFF, 2); }
    if !b_action.is_empty() { draw_text_scaled(buf, fb_w, fb_h, 600, y + 26, b_action, 0xFF94A3B8, 2); }
}

fn draw_card(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, color: u32, focused: bool) {
    for cy in y..(y+h).min(fb_h) {
        let row = cy * fb_w;
        for cx in x..(x+w).min(fb_w) {
            let is_border = cx < x + 6 || cx > x + w - 7 || cy < y + 6 || cy > y + h - 7;
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

fn draw_settings_row(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, name: &str, color: u32, focused: bool, active: bool) {
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

    let swatch_x = x + 30;
    let swatch_y = y + 16;
    for sy in swatch_y..swatch_y + 24 {
        let r = sy * fb_w;
        for sx in swatch_x..swatch_x + 45 {
            buf[r + sx] = color;
        }
    }

    draw_text_scaled(buf, fb_w, fb_h, x + 100, y + 16, name, 0xFFFFFFFF, 2);

    if active {
        draw_text_scaled(buf, fb_w, fb_h, x + w - 180, y + 16, "[RUNNING]", 0xFFF59E0B, 2);
    }
}

fn draw_dialog_row(buf: &mut [u32], fb_w: usize, fb_h: usize, x: usize, y: usize, w: usize, h: usize, name: &str, focused: bool) {
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

    draw_text_scaled(buf, fb_w, fb_h, x + 25, y + 14, "[IMG]", if focused { 0xFFFDE047 } else { 0xFF38BDF8 }, 2);
    draw_text_scaled(buf, fb_w, fb_h, x + 120, y + 14, name, 0xFFFFFFFF, 2);
}

fn format_size_slice(buf: &mut [u8; 16], n: usize, dec: usize, unit: &str) -> usize {
    let mut i = 0;
    i += put_num_dynamic(buf, i, n);
    if unit == "MB" && dec > 0 {
        buf[i] = b'.'; i += 1;
        buf[i] = b'0' + (dec as u8 % 10); i += 1;
    }
    buf[i] = b' '; i += 1;
    for b in unit.bytes() { buf[i] = b; i += 1; }
    i
}

fn put_num_dynamic(buf: &mut [u8], start: usize, mut n: usize) -> usize {
    if n == 0 {
        buf[start] = b'0';
        return 1;
    }
    let mut temp = [0u8; 10];
    let mut count = 0;
    while n > 0 {
        temp[count] = b'0' + (n % 10) as u8;
        n /= 10;
        count += 1;
    }
    for idx in 0..count {
        buf[start + idx] = temp[count - 1 - idx];
    }
    count
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
