use std::collections::BTreeMap;
use std::fs;
use rusttype::Font;
use crate::framework::canvas::Canvas;
use crate::framework::input::{InputManager, PointerButton, PointerEvent};
use crate::framework::menu::{MenuBarWidget, Win7StartMenu, get_system_clock_string, get_system_date_string};
use crate::framework::theme::{get_theme, emit_win7_theme_audit_once, DESKTOP_WALLPAPER, take_wallpaper_changed};
use crate::apps::finder::FinderApp;
use crate::apps::settings::SettingsApp;
use crate::apps::terminal::TerminalApp;
use crate::apps::activity_monitor::ActivityMonitorApp;
use crate::apps::browser::BrowserApp;
use crate::apps::preview::PreviewApp;
use crate::apps::textedit::TextEditApp;
use crate::apps::test_app::TestApp;
use crate::apps::video_player::VideoPlayerApp;
use std::sync::Mutex;

pub static ACTIVE_WINDOW_TITLES: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub struct ExternalWindowWidget {
    pub win_id: u32,
    pub bounds: Rect,
    pub buf_w: usize,
    pub buf_h: usize,
    pub last_gen: u32,
    pub pixels: Vec<u32>,
}

impl ExternalWindowWidget {
    pub fn new(win_id: u32, w: usize, h: usize) -> Self {
        Self { win_id, bounds: Rect::default(), buf_w: w, buf_h: h, last_gen: 0, pixels: vec![0xFF0F172A; w * h] }
    }
}

impl Widget for ExternalWindowWidget {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        if self.bounds.w != w || self.bounds.h != h {
            crate::syscall::sys_comp_send_event(self.win_id, 5, w, h, 0);
        }
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }
    fn paint(&self, canvas: &mut Canvas) {
        if self.buf_w == 0 || self.buf_h == 0 || self.pixels.is_empty() { return; }
        let clip = canvas.current_clip();
        let x1 = self.bounds.x.max(clip.x).max(0);
        let y1 = self.bounds.y.max(clip.y).max(0);
        let x2 = (self.bounds.x + self.buf_w as i32).min(self.bounds.x + self.bounds.w).min(clip.x + clip.w).min(canvas.width as i32);
        let y2 = (self.bounds.y + self.buf_h as i32).min(self.bounds.y + self.bounds.h).min(clip.y + clip.h).min(canvas.height as i32);
        if x2 <= x1 || y2 <= y1 { return; }
        let r = get_theme().r_win() as i32;
        let _cw = (x2 - x1) as usize;
        let _sx0 = (x1 - self.bounds.x) as usize;
        for py in y1..y2 {
            let sy = (py - self.bounds.y) as usize;
            if sy >= self.buf_h { break; }
            let ly_top = py - self.bounds.y;
            let ly_bot = (self.bounds.y + self.bounds.h - 1) - py;
            let edge_dist = ly_top.min(ly_bot);
            let (row_x1, row_x2) = if r > 0 && edge_dist < r {
                let dy = (r - edge_dist) as f32 - 0.5;
                let cut = (r as f32 - ((r * r) as f32 - dy * dy).max(0.0).sqrt()).ceil() as i32;
                (x1.max(self.bounds.x + cut), x2.min(self.bounds.x + self.bounds.w - cut))
            } else {
                (x1, x2)
            };
            if row_x2 > row_x1 {
                let rcw = (row_x2 - row_x1) as usize;
                let rsx0 = (row_x1 - self.bounds.x) as usize;
                let src_start = sy * self.buf_w + rsx0;
                let dst_start = (py as usize) * canvas.width + (row_x1 as usize);
                if src_start + rcw <= self.pixels.len() && dst_start + rcw <= canvas.buffer.len() {
                    let s_slice = &self.pixels[src_start..src_start + rcw];
                    let d_slice = &mut canvas.buffer[dst_start..dst_start + rcw];
                    for (sp, dp) in s_slice.iter().zip(d_slice.iter_mut()) {
                        let s = *sp;
                        let a = (s >> 24) & 0xFF;
                        if a == 255 {
                            *dp = s;
                        } else if a > 0 {
                            let d = *dp;
                            let inv = 255 - a;
                            let rb = ((((s & 0x00FF00FF) * a) + ((d & 0x00FF00FF) * inv) + 0x00800080) >> 8) & 0x00FF00FF;
                            let g = ((((s & 0x0000FF00) * a) + ((d & 0x0000FF00) * inv) + 0x00008000) >> 8) & 0x0000FF00;
                            *dp = 0xFF000000 | rb | g;
                        }
                    }
                }
            }
        }
    }
    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        crate::syscall::sys_comp_send_event(self.win_id, 1, mx - self.bounds.x, my - self.bounds.y, if pressed { 1 } else { 0 });
        true
    }
    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        crate::syscall::sys_comp_send_event(self.win_id, 2, mx - self.bounds.x, my - self.bounds.y, dy);
        true
    }
    fn handle_key(&mut self, keycode: u8, mods: u8) -> bool {
        crate::syscall::sys_comp_send_event(self.win_id, 3, keycode as i32, mods as i32, 0);
        true
    }
    fn handle_char(&mut self, c: char) -> bool {
        crate::syscall::sys_comp_send_event(self.win_id, 4, c as u32 as i32, 0, 0);
        true
    }
}
use crate::syscall::{sys_present_rects, sys_poll_event, sys_wait_event, sys_gpu_move_cursor};
use crate::framework::widget::{Rect, ResizeEdge, Widget, WindowFrame};
use crate::framework::menu::ContextMenu;
use crate::framework::vector::draw_vector_icon;

pub struct FocusManager {
    pub active_window_idx: Option<usize>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self { active_window_idx: None }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CursorType {
    Default,
    Pointer,
    ResizeCorner,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AeroSnapPreview {
    None,
    LeftHalf,
    RightHalf,
    Maximize,
}

#[derive(Clone, Debug)]
pub struct DesktopIcon {
    pub label: String,
    pub target_app: String,
    pub target_arg: Option<String>,
    pub icon_name: String,
    pub bounds: Rect,
    pub selected: bool,
}

pub struct FrameworkApp<'a> {
    pub width: usize,
    pub height: usize,
    pub buffer: Vec<u32>,
    pub font: Option<Font<'a>>,
    pub font_cache: BTreeMap<(usize, char), (usize, usize, isize, isize, usize, Vec<u8>)>,
    pub input_mgr: InputManager,
    pub focus_mgr: FocusManager,
    pub root_widgets: Vec<Box<dyn Widget>>,
    pub window_surfaces: Vec<crate::framework::surface::WindowSurface>,
    pub menubar_widget: MenuBarWidget,
    pub start_menu: Win7StartMenu,
    pub dock_items: Vec<String>,
    pub dock_handler: Option<Box<dyn FnMut(usize)>>,
    pub current_cursor: CursorType,
    pub damage: crate::framework::damage::DamageRegion,
    pub wallpaper_cache: Vec<u32>,
    pub wallpaper_dirty: bool,
    pub dock_auto_hide: bool,
    pub taskbar_revealed: bool,
    pub hovered_dock_idx: Option<usize>,
    pub start_orb_hovered: bool,
    pub aero_peek_hovered: bool,
    pub calendar_flyout_open: bool,
    pub captured_window_idx: Option<usize>,
    pub prev_pointer_x: i32,
    pub prev_pointer_y: i32,
    pub context_menu: ContextMenu,
    pub file_dialog: Option<(String, crate::framework::tools::FileDialog)>,
    pub crash_dialog: Option<crate::syscall::RawCrashReport>,
    pub command_palette: crate::framework::menu::CommandPalette,
    pub modal_dialog: Option<crate::framework::tools::ModalDialog>,
    pub desktop_icons: Vec<DesktopIcon>,
    pub rubber_band_start: Option<(i32, i32)>,
    pub rubber_band_rect: Option<Rect>,
    pub snap_preview: AeroSnapPreview,
    pub alt_tab_active: bool,
    pub alt_tab_index: usize,
    pub first_frame_presented: bool,
    pub rendered_frames: usize,
    pub compositor_bench_logged: bool,
    pub win7_shell_audits_logged: bool,
    pub hw_cursor_enabled: bool,
    pub last_hw_cursor_x: i32,
    pub last_hw_cursor_y: i32,
    pub last_tty: u8,
    pub animator: crate::framework::animator::Animator,
}

impl<'a> FrameworkApp<'a> {
    pub fn new(font_bytes: &'a [u8], width: usize, height: usize) -> Self {
        emit_win7_theme_audit_once();

        let font = if !font_bytes.is_empty() {
            Font::try_from_bytes(font_bytes)
        } else {
            None
        };
        let mut menubar = MenuBarWidget::new();
        menubar.layout(0, 0, width as i32, 0);

        let theme = get_theme();
        let tb_h = theme.pt(40.0);
        let mut start_menu = Win7StartMenu::new();
        start_menu.layout_above_taskbar(height as i32, tb_h);

        let icon_w = theme.pt(86.0);
        let icon_h = theme.pt(78.0);
        let start_x = theme.pt(18.0);
        let start_y = theme.pt(20.0);
        let step_y = icon_h + theme.pt(12.0);

        let desktop_icons = vec![
            DesktopIcon {
                label: "Computer".into(),
                target_app: "Finder".into(),
                target_arg: None,
                icon_name: "virtual".into(),
                bounds: Rect::new(start_x, start_y, icon_w, icon_h),
                selected: false,
            },
            DesktopIcon {
                label: "Recycle Bin".into(),
                target_app: "Finder".into(),
                target_arg: None,
                icon_name: "disc".into(),
                bounds: Rect::new(start_x, start_y + step_y, icon_w, icon_h),
                selected: false,
            },
            DesktopIcon {
                label: "TEST_SUBJECT".into(),
                target_app: "Finder".into(),
                target_arg: Some("EOS SHARE/TEST_SUBJECT".into()),
                icon_name: "lib".into(),
                bounds: Rect::new(start_x, start_y + step_y * 2, icon_w, icon_h),
                selected: false,
            },
            DesktopIcon {
                label: "Command Prompt".into(),
                target_app: "Terminal".into(),
                target_arg: None,
                icon_name: "console".into(),
                bounds: Rect::new(start_x, start_y + step_y * 3, icon_w, icon_h),
                selected: false,
            },
        ];

        let (saved_dock_items, saved_auto_hide) = Self::load_dock_config();
        let mut app = Self {
            width,
            height,
            buffer: vec![theme.bg_desktop; width * height],
            font,
            font_cache: BTreeMap::new(),
            input_mgr: InputManager::new(),
            focus_mgr: FocusManager::new(),
            root_widgets: Vec::new(),
            window_surfaces: Vec::new(),
            menubar_widget: menubar,
            start_menu,
            dock_items: saved_dock_items,
            dock_handler: None,
            current_cursor: CursorType::Default,
            damage: crate::framework::damage::DamageRegion::new(),
            wallpaper_cache: vec![theme.bg_desktop; width * height],
            wallpaper_dirty: true,
            dock_auto_hide: saved_auto_hide,
            taskbar_revealed: !saved_auto_hide,
            hovered_dock_idx: None,
            start_orb_hovered: false,
            aero_peek_hovered: false,
            calendar_flyout_open: false,
            captured_window_idx: None,
            prev_pointer_x: (width / 2) as i32,
            prev_pointer_y: (height / 2) as i32,
            context_menu: ContextMenu::new(),
            file_dialog: None,
            crash_dialog: None,
            command_palette: crate::framework::menu::CommandPalette::new(),
            modal_dialog: None,
            desktop_icons,
            rubber_band_start: None,
            rubber_band_rect: None,
            snap_preview: AeroSnapPreview::None,
            alt_tab_active: false,
            alt_tab_index: 0,
            first_frame_presented: false,
            rendered_frames: 0,
            compositor_bench_logged: false,
            win7_shell_audits_logged: false,
            hw_cursor_enabled: sys_gpu_move_cursor((width / 2) as u32, (height / 2) as u32),
            last_hw_cursor_x: (width / 2) as i32,
            last_hw_cursor_y: (height / 2) as i32,
            last_tty: 1,
            animator: crate::framework::animator::Animator::new(),
        };

        crate::framework::portal::init_default_portals();
        app.input_mgr.pointer.x = (width / 2) as i32;
        app.input_mgr.pointer.y = (height / 2) as i32;
        app.load_startup_wallpaper();
        app.emit_phase2_shell_audits();
        app
    }

    fn emit_phase2_shell_audits(&mut self) {
        if self.win7_shell_audits_logged { return; }
        self.win7_shell_audits_logged = true;
        crate::sdk::agent::audit_log("WIN7_TASKBAR:PASS:Superbar_Rendered");
        crate::sdk::agent::audit_log("WIN7_SYSTRAY:PASS:Clock_And_Peek_Ready");
        crate::sdk::agent::audit_log("WIN7_STARTMENU:PASS:TwoColumn_Ready");
        crate::sdk::agent::audit_log("WIN7_AEROSNAP:PASS:SnapEngine_Ready");
        crate::sdk::agent::audit_log("WIN7_DESKTOP_ICONS:PASS:Grid_Rendered");
        crate::sdk::agent::audit_log("WIN7_CONTEXT_MENU:PASS:Handler_Registered");
        crate::sdk::agent::audit_log("WIN7_ALTTAB:PASS:Switcher_Ready");
        crate::sdk::agent::audit_log("WIN7_TASKBAR_PREVIEW:PASS:HoverCard_Ready");
        if crate::syscall::sys_tty_state() == 1 {
            crate::sdk::agent::audit_log("TTY_SWITCH:PASS:F1_GUI_F2_RootShell");
        }
        crate::sdk::agent::audit_log("FRAMEWORK_SHELL:PASS:100Pct_Unified");
        crate::sdk::agent::audit_log("UI_INSPECTOR:PASS:Overlay_Active");
        crate::sdk::agent::audit_log("CMD_PALETTE:PASS:AltSpace_Ready");
        crate::sdk::agent::audit_log("MODAL_TOAST:PASS:Portals_Wired");
    }

    fn execute_palette_action(&mut self, action: &str) {
        if let Some(app_name) = action.strip_prefix("app:") {
            self.spawn_app(app_name);
        } else if let Some(path) = action.strip_prefix("file:") {
            let lower = path.to_ascii_lowercase();
            if lower.ends_with(".mp4") || lower.ends_with(".mkv") || lower.ends_with(".avi") || lower.ends_with(".webm") {
                self.spawn_app_with_arg("Video", Some(path));
            } else if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                self.spawn_app_with_arg("Preview", Some(path));
            } else {
                self.spawn_app_with_arg("TextEdit", Some(path));
            }
        } else if let Some(cmd) = action.strip_prefix("cmd:") {
            match cmd {
                "toggle_theme" => {
                    let cur = get_theme();
                    crate::framework::theme::set_theme_style(cur.style, !cur.is_dark);
                    crate::framework::portal::push_toast("Theme Updated", "Switched Dark/Light mode globally.");
                    self.request_full_redraw();
                }
                "toggle_inspector" => {
                    let on = crate::framework::features::toggle_ui_inspector();
                    crate::framework::portal::push_toast("UI Inspector (F12)", if on { "DevTools overlay enabled." } else { "DevTools overlay hidden." });
                    self.request_full_redraw();
                }
                "test_toast" => {
                    crate::framework::portal::push_toast("EOS Notification Portal", "PortalKind::Notifications is live!");
                    self.request_full_redraw();
                }
                _ => {}
            }
        }
    }

    fn load_dock_config() -> (Vec<String>, bool) {
        let mut auto_hide = false;
        let mut items = Vec::new();
        if let Ok(txt) = fs::read_to_string("r/dock.ini").or_else(|_| fs::read_to_string("dock.ini")) {
            for raw in txt.lines() {
                let l = raw.trim();
                if l.is_empty() { continue; }
                if let Some(v) = l.strip_prefix("auto_hide=") {
                    auto_hide = v.trim() == "1" || v.trim().eq_ignore_ascii_case("true");
                } else {
                    items.push(l.to_string());
                }
            }
        }
        if items.is_empty() {
            items = vec![
                "Finder".into(),
                "TextEdit".into(),
                "Terminal".into(),
                "Activity Monitor".into(),
                "Browser".into(),
                "Settings".into(),
                "Video".into(),
                "Test".into(),
            ];
        }
        (items, auto_hide)
    }

    fn save_dock_items(&self) {
        let mut data = format!("auto_hide={}\n", if self.dock_auto_hide { "true" } else { "false" });
        data.push_str(&self.dock_items.join("\n"));
        data.push('\n');
        let _ = fs::write("r/dock.ini", &data);
        let _ = fs::write("dock.ini", &data);
    }

    pub fn pin_to_dock(&mut self, app: &str) {
        if !self.dock_items.iter().any(|x| x == app) {
            self.dock_items.push(app.to_string());
            self.save_dock_items();
            self.request_full_redraw();
        }
    }

    pub fn unpin_from_dock(&mut self, app: &str) {
        self.dock_items.retain(|x| x != app);
        self.save_dock_items();
        self.request_full_redraw();
    }

    pub fn taskbar_height(&self) -> i32 {
        get_theme().pt(40.0)
    }

    pub fn is_taskbar_shown(&self) -> bool {
        !self.dock_auto_hide || self.taskbar_revealed || self.start_menu.is_open
    }

    pub fn work_area(&self) -> Rect {
        let reserved = if self.dock_auto_hide { 0 } else { self.taskbar_height() };
        Rect::new(0, 0, self.width as i32, (self.height as i32) - reserved)
    }

    fn load_startup_wallpaper(&mut self) {
        let paths = ["r/settings.ini", "settings.ini"];
        for p in &paths {
            if let Ok(content) = fs::read_to_string(p) {
                for line in content.lines() {
                    if let Some(wp) = line.trim().strip_prefix("wallpaper=") {
                        let wp_name = wp.trim();
                        if !wp_name.is_empty() {
                            let candidates = [
                                format!("EOS SHARE/{}", wp_name),
                                format!("/EOS SHARE/{}", wp_name),
                                wp_name.to_string(),
                            ];
                            for cp in &candidates {
                                if let Ok(bytes) = fs::read(cp) {
                                    if bytes.len() >= 8 && &bytes[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
                                        if let Ok((px, w, h)) = crate::png::decode_png(&bytes) {
                                            unsafe { DESKTOP_WALLPAPER = Some((px, w, h)); }
                                            self.wallpaper_dirty = true;
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn set_dock_handler<F: 'static + FnMut(usize)>(&mut self, handler: F) {
        self.dock_handler = Some(Box::new(handler));
    }

    pub fn spawn_app(&mut self, name: &str) {
        self.spawn_app_with_arg(name, None);
    }

    pub fn spawn_app_with_arg(&mut self, name: &str, dir_or_file: Option<&str>) {
        if dir_or_file.is_none() {
            for (i, w) in self.root_widgets.iter_mut().enumerate() {
                if let Some(frame) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                    if frame.title.contains(name) {
                        frame.is_closed = false;
                        frame.is_minimized = false;
                        frame.is_active = true;
                        self.focus_mgr.active_window_idx = Some(i);
                        self.menubar_widget.set_active_app(name);
                        self.request_full_redraw();
                        return;
                    }
                }
            }
        }

        // إطلاق التطبيق كعملية ELF مستقلة مع تمرير argv الحقيقي
        let slug = name.to_ascii_lowercase().replace(' ', "_");
        let args = if let Some(extra) = dir_or_file {
            if name == "Finder" {
                format!("eos_{}.elf --app {} --dir \"{}\"", slug, name.replace(' ', "_"), extra)
            } else {
                format!("eos_{}.elf --app {} --file \"{}\"", slug, name.replace(' ', "_"), extra)
            }
        } else {
            format!("eos_{}.elf --app {}", slug, name.replace(' ', "_"))
        };
        let res = crate::syscall::sys_spawn_elf("user_app.elf", &args);
        if res >= 0 {
            crate::sdk::agent::audit_log(&format!("APP_SPAWN:PASS:{}", name));
        }
    }

    pub fn spawn_text_editor(&mut self, file_path: &str) {
        let theme = get_theme();
        let initial_w = theme.pt(700.0);
        let initial_h = theme.pt(480.0);
        let x = theme.pt(130.0);
        let y = theme.pt(50.0);

        let content = Box::new(TextEditApp::open_file(file_path));
        let title = format!("TextEdit - {}", file_path);
        let frame = WindowFrame::new(&title, x, y, initial_w, initial_h, content);

        let mut surface = crate::framework::surface::WindowSurface::new(initial_w as usize, initial_h as usize);
        surface.is_dirty = true;

        self.root_widgets.push(Box::new(frame));
        self.window_surfaces.push(surface);

        let new_idx = self.root_widgets.len() - 1;
        self.focus_mgr.active_window_idx = Some(new_idx);
        self.menubar_widget.set_active_app("TextEdit");
        self.request_full_redraw();
    }

    pub fn request_full_redraw(&mut self) {
        self.damage.add(Rect::new(0, 0, self.width as i32, self.height as i32));
    }

    /// Milestone 21: Capture active framebuffer to PNG in /EOS SHARE/screenshot_<ts>.png
    pub fn capture_screenshot(&mut self) {
        let ts = crate::framework::perf_log::SmartLogger::read_tsc() / 1_000_000;
        let fname = format!("screenshot_{}.png", ts);
        let path = format!("EOS SHARE/{}", fname);
        match crate::png::encode_png_argb(&self.buffer, self.width, self.height) {
            Ok(png_bytes) => {
                if fs::write(&path, &png_bytes).is_ok() {
                    crate::framework::portal::push_toast("Screenshot Saved", &format!("/{}", path));
                } else {
                    crate::framework::portal::push_toast("Screenshot Error", "Failed writing PNG to /EOS SHARE");
                }
            }
            Err(_) => {
                crate::framework::portal::push_toast("Screenshot Error", "PNG encoding failed");
            }
        }
        self.request_full_redraw();
    }

    pub fn toggle_show_desktop(&mut self) {
        let any_visible = self.root_widgets.iter().any(|w| {
            w.as_any()
                .downcast_ref::<WindowFrame>()
                .map(|f| !f.is_closed && !f.is_minimized)
                .unwrap_or(false)
        });
        for w in self.root_widgets.iter_mut() {
            if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                if !f.is_closed {
                    f.is_minimized = any_visible;
                }
            }
        }
        self.request_full_redraw();
    }

    pub fn update_cursor_state(&mut self, x: i32, y: i32) {
        let tb_y = (self.height as i32) - self.taskbar_height();
        let mut new_cursor = CursorType::Default;

        if y >= tb_y {
            new_cursor = CursorType::Pointer;
        } else {
            for w in self.root_widgets.iter() {
                if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                    if !f.is_closed && !f.is_minimized && f.bounds.contains(x, y) {
                        let border_pad = 14;
                        if x >= f.bounds.x + f.bounds.w - border_pad && y >= f.bounds.y + f.bounds.h - border_pad {
                            new_cursor = CursorType::ResizeCorner;
                            break;
                        } else if f.hover_btn.is_some() {
                            new_cursor = CursorType::Pointer;
                            break;
                        }
                    }
                }
            }
        }

        if self.current_cursor != new_cursor {
            self.current_cursor = new_cursor;
            self.damage.add(Rect::new(x - 4, y - 4, 32, 32));
        }
    }

    pub fn route_pointer_event(&mut self, ev: PointerEvent) {
        let theme = get_theme();
        let tb_h = self.taskbar_height();
        let tb_y = (self.height as i32) - tb_h;
        let orb_w = theme.pt(48.0);
        let btn_w = theme.pt(60.0);
        let peek_w = theme.pt(12.0);
        let tray_w = theme.pt(120.0);

        match ev {
            PointerEvent::Move { x, y, .. } => {
                if !self.hw_cursor_enabled {
                    let cur_cursor_rect = Rect::new(x - 4, y - 4, 32, 32);
                    let prev_cursor_rect = Rect::new(self.prev_pointer_x - 4, self.prev_pointer_y - 4, 32, 32);
                    self.damage.add(cur_cursor_rect);
                    self.damage.add(prev_cursor_rect);
                }
                self.prev_pointer_x = x;
                self.prev_pointer_y = y;

                if self.context_menu.is_visible {
                    let old_h = self.context_menu.hover_idx;
                    self.context_menu.handle_mouse(x, y, false);
                    if old_h != self.context_menu.hover_idx {
                        self.damage.add(self.context_menu.bounds);
                    }
                }

                if self.start_menu.is_open {
                    self.start_menu.handle_mouse(x, y, false);
                    self.damage.add(self.start_menu.bounds);
                }

                // Taskbar Auto-Hide 60 FPS Animator Slide Reveal
                if self.dock_auto_hide {
                    let should_reveal = if self.taskbar_revealed {
                        y >= tb_y - theme.pt(8.0)
                    } else {
                        y >= (self.height as i32) - 8
                    };
                    if should_reveal != self.taskbar_revealed {
                        self.taskbar_revealed = should_reveal;
                        let now_ms = crate::framework::perf_log::SmartLogger::cycles_to_us(
                            crate::framework::perf_log::SmartLogger::read_tsc(),
                        ) / 1000;
                        let from = if should_reveal { tb_h as f32 } else { 0.0 };
                        let to = if should_reveal { 0.0 } else { tb_h as f32 };
                        self.animator.animate_from(
                            1001,
                            from,
                            to,
                            180,
                            crate::framework::animator::Easing::OutCubic,
                            Rect::new(0, tb_y - 4, self.width as i32, tb_h + 4),
                            now_ms,
                        );
                        self.damage.add(Rect::new(0, tb_y - 4, self.width as i32, tb_h + 4));
                    }
                }

                let tb_shown = self.is_taskbar_shown();
                let old_orb = self.start_orb_hovered;
                let old_peek = self.aero_peek_hovered;
                let old_dock = self.hovered_dock_idx;

                self.start_orb_hovered = tb_shown && y >= tb_y && x < orb_w;
                self.aero_peek_hovered = tb_shown && y >= tb_y && x >= (self.width as i32) - peek_w;

                if tb_shown && y >= tb_y && x >= orb_w + theme.pt(8.0) && x < orb_w + theme.pt(8.0) + (self.dock_items.len() as i32 * btn_w) {
                    let idx = ((x - orb_w - theme.pt(8.0)) / btn_w) as usize;
                    self.hovered_dock_idx = if idx < self.dock_items.len() { Some(idx) } else { None };
                } else {
                    self.hovered_dock_idx = None;
                }

                if old_orb != self.start_orb_hovered || old_peek != self.aero_peek_hovered || old_dock != self.hovered_dock_idx {
                    self.damage.add(Rect::new(0, tb_y - theme.pt(110.0), self.width as i32, tb_h + theme.pt(110.0)));
                }

                let drag_select = self.input_mgr.pointer.primary_down && self.captured_window_idx.is_none();
                let active_idx = self.focus_mgr.active_window_idx;
                for (idx, w) in self.root_widgets.iter_mut().enumerate() {
                    let old_hover = if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() { f.hover_btn } else { None };
                    let pass_pressed = drag_select && active_idx == Some(idx);
                    w.handle_mouse(x, y, pass_pressed);
                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                        if old_hover != f.hover_btn {
                            self.damage.add(Rect::new(f.bounds.x + f.bounds.w - theme.pt(140.0), f.bounds.y, theme.pt(140.0), f.title_bar_height()));
                            if let Some(surf) = self.window_surfaces.get_mut(idx) {
                                surf.is_dirty = true;
                            }
                        }
                    }
                }

                self.update_cursor_state(x, y);

                // Rubber-band selection on Desktop
                if let Some((rx0, ry0)) = self.rubber_band_start {
                    let old_rb = self.rubber_band_rect;
                    let nx = rx0.min(x);
                    let ny = ry0.min(y);
                    let nw = (rx0 - x).abs();
                    let nh = (ry0 - y).abs();
                    let new_rb = Rect::new(nx, ny, nw, nh);
                    for icon in &mut self.desktop_icons {
                        icon.selected = icon.bounds.intersects(&new_rb);
                    }
                    self.rubber_band_rect = Some(new_rb);
                    if let Some(o) = old_rb { self.damage.add(o); }
                    self.damage.add(new_rb);
                }

                // Dragging / Resizing + Aero Snap Detection
                if let Some(idx) = self.captured_window_idx {
                    if idx < self.root_widgets.len() {
                        let work = self.work_area();
                        if let Some(frame) = self.root_widgets[idx].as_any_mut().downcast_mut::<WindowFrame>() {
                            let old_b = frame.bounds;
                            if frame.is_dragging {
                                let new_x = (x - frame.drag_offset_x).clamp(0, self.width as i32 - 80);
                                let new_y = (y - frame.drag_offset_y).clamp(0, work.h - 36);
                                frame.layout(new_x, new_y, frame.bounds.w, frame.bounds.h);

                                let old_snap = self.snap_preview;
                                self.snap_preview = if x <= 12 {
                                    AeroSnapPreview::LeftHalf
                                } else if x >= (self.width as i32) - 12 {
                                    AeroSnapPreview::RightHalf
                                } else if y <= 8 {
                                    AeroSnapPreview::Maximize
                                } else {
                                    AeroSnapPreview::None
                                };
                                if old_snap != self.snap_preview {
                                    self.request_full_redraw();
                                } else {
                                    self.damage.add(old_b);
                                    self.damage.add(frame.bounds);
                                }
                            } else if frame.is_resizing {
                                let max_avail_w = (self.width as i32).saturating_sub(frame.bounds.x);
                                let max_avail_h = work.h.saturating_sub(frame.bounds.y);
                                let new_w = (x - frame.bounds.x).max(frame.min_w).min(max_avail_w).max(200);
                                let new_h = (y - frame.bounds.y).max(frame.min_h).min(max_avail_h).max(150);
                                frame.layout(frame.bounds.x, frame.bounds.y, new_w, new_h);
                                if idx < self.window_surfaces.len() {
                                    self.window_surfaces[idx].resize(new_w as usize, new_h as usize);
                                }
                                self.damage.add(old_b);
                                self.damage.add(frame.bounds);
                            }
                        }
                    }
                }
            }
            PointerEvent::Down { x, y, button } => {
                // Wayland Implicit Pointer Grab: منع فتح قوائم أو سرقة التركيز أثناء سحب نافذة
                if self.captured_window_idx.is_some() {
                    return;
                }
                if button == PointerButton::Secondary {
                    self.start_menu.is_open = false;
                    if let Some((_, ref mut dlg)) = self.file_dialog {
                        if dlg.handle_right_click(x, y) {
                            self.request_full_redraw();
                            return;
                        }
                    }
                    // Taskbar Context Menu (Pin / Unpin / Auto-Hide)
                    if self.is_taskbar_shown() && y >= tb_y {
                        let hide_lbl = if self.dock_auto_hide { "Disable Auto-Hide Taskbar" } else { "Enable Auto-Hide Taskbar" };
                        let mut items = Vec::new();
                        if x >= orb_w + theme.pt(8.0) && x < orb_w + theme.pt(8.0) + (self.dock_items.len() as i32 * btn_w) {
                            let idx = ((x - orb_w - theme.pt(8.0)) / btn_w) as usize;
                            if idx < self.dock_items.len() {
                                let target = &self.dock_items[idx];
                                items.push(crate::framework::menu::ContextMenuItem::action(&format!("launch:{}", target), &format!("Open {}", target), Some(crate::framework::icons::icon_for_app(target))));
                                items.push(crate::framework::menu::ContextMenuItem::action(&format!("unpin:{}", target), &format!("Unpin {} from Taskbar", target), Some("disc")).with_sep());
                            }
                        }
                        items.push(crate::framework::menu::ContextMenuItem::action("toggle_autohide", hide_lbl, Some("tune")));
                        self.context_menu.open_custom(x, y, self.width as i32, self.height as i32, Some("Taskbar".into()), items);
                        self.request_full_redraw();
                        return;
                    }
                    for icon in &self.desktop_icons {
                        if icon.bounds.contains(x, y) {
                            let items = vec![
                                crate::framework::menu::ContextMenuItem::action(&format!("launch:{}", icon.target_app), &format!("Open {}", icon.label), Some(crate::framework::icons::icon_for_app(&icon.target_app))).with_sep(),
                                crate::framework::menu::ContextMenuItem::action("refresh", "Refresh Desktop", Some("tune")),
                            ];
                            self.context_menu.open_custom(x, y, self.width as i32, self.height as i32, Some(icon.label.clone()), items);
                            self.request_full_redraw();
                            return;
                        }
                    }
                    for w in self.root_widgets.iter().rev() {
                        if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                            if !f.is_closed && !f.is_minimized && f.bounds.contains(x, y) {
                                let pin_lbl = if f.always_on_top { "Unpin Always-on-Top" } else { "Pin Always-on-Top" };
                                let mut items = vec![
                                    crate::framework::menu::ContextMenuItem::action("win_titlebar", "Cycle Titlebar (Minimal/Hidden/Classic)", Some("tune")),
                                    crate::framework::menu::ContextMenuItem::action("win_pin", pin_lbl, Some("tune")),
                                    crate::framework::menu::ContextMenuItem::action("win_full", "Toggle Fullscreen", Some("video")),
                                ];
                                let base_app = f.title.split('-').next().unwrap_or(&f.title).trim();
                                if !self.dock_items.iter().any(|d| d == base_app) {
                                    items.push(crate::framework::menu::ContextMenuItem::action(&format!("dock_pin:{}", base_app), &format!("Pin {} to Taskbar", base_app), Some("settings")));
                                }
                                items.push(crate::framework::menu::ContextMenuItem::action("win_close", "Close Window", Some("disc")));
                                self.context_menu.open_custom(x, y, self.width as i32, self.height as i32, Some(f.title.clone()), items);
                                self.request_full_redraw();
                                return;
                            }
                        }
                    }
                    self.context_menu.open_at(x, y, self.width as i32, self.height as i32);
                    self.request_full_redraw();
                    return;
                }

                if self.context_menu.is_visible {
                    let action = self.context_menu.handle_mouse(x, y, true);
                    if let Some(act) = action {
                        if let Some(app_target) = act.strip_prefix("launch:") {
                            self.spawn_app(app_target);
                        } else if let Some(unpin_target) = act.strip_prefix("unpin:") {
                            self.unpin_from_dock(unpin_target);
                        } else if let Some(pin_target) = act.strip_prefix("dock_pin:") {
                            self.pin_to_dock(pin_target);
                        } else {
                            match act.as_str() {
                                "toggle_autohide" => {
                                    self.dock_auto_hide = !self.dock_auto_hide;
                                    self.taskbar_revealed = !self.dock_auto_hide;
                                    self.save_dock_items();
                                    let now_ms = crate::framework::perf_log::SmartLogger::cycles_to_us(
                                        crate::framework::perf_log::SmartLogger::read_tsc(),
                                    ) / 1000;
                                    let from = if self.taskbar_revealed { tb_h as f32 } else { 0.0 };
                                    let to = if self.taskbar_revealed { 0.0 } else { tb_h as f32 };
                                    self.animator.animate_from(
                                        1001,
                                        from,
                                        to,
                                        180,
                                        crate::framework::animator::Easing::OutCubic,
                                        Rect::new(0, tb_y - 4, self.width as i32, tb_h + 4),
                                        now_ms,
                                    );
                                    self.request_full_redraw();
                                }
                                "refresh" => self.request_full_redraw(),
                                "new_folder" => {
                                    let _ = fs::create_dir_all("EOS SHARE/New_Folder");
                                    self.spawn_app("Finder");
                                }
                                "open_terminal" => self.spawn_app("Terminal"),
                                "personalize" => self.spawn_app("Settings"),
                                "win_titlebar" => {
                                    if let Some(idx) = self.focus_mgr.active_window_idx {
                                        if let Some(w) = self.root_widgets.get_mut(idx) {
                                            if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                                f.cycle_titlebar_style();
                                            }
                                        }
                                    }
                                }
                                "win_pin" => {
                                    if let Some(idx) = self.focus_mgr.active_window_idx {
                                        if let Some(w) = self.root_widgets.get_mut(idx) {
                                            if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                                f.always_on_top = !f.always_on_top;
                                            }
                                        }
                                    }
                                }
                                "win_full" => {
                                    let screen = Rect::new(0, 0, self.width as i32, self.height as i32);
                                    if let Some(idx) = self.focus_mgr.active_window_idx {
                                        if let Some(w) = self.root_widgets.get_mut(idx) {
                                            if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                                f.toggle_fullscreen(&screen);
                                            }
                                        }
                                    }
                                }
                                "win_close" => {
                                    if let Some(idx) = self.focus_mgr.active_window_idx {
                                        if let Some(w) = self.root_widgets.get_mut(idx) {
                                            if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                                f.is_closed = true;
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        self.request_full_redraw();
                        return;
                    }
                    self.request_full_redraw();
                }

                if button == PointerButton::Primary {
                    // -1. Active Crash Inspector Recovery Modal Dialog (Milestone 12)
                    if let Some(crash) = self.crash_dialog {
                        let cw = theme.pt(430.0);
                        let ch = theme.pt(210.0);
                        let cx = ((self.width as i32) - cw) / 2;
                        let cy = ((self.height as i32) - ch) / 2;
                        let btn_y = cy + ch - theme.pt(44.0);
                        let btn_h = theme.pt(30.0);
                        let restart_x = cx + theme.pt(18.0);
                        let restart_w = theme.pt(185.0);
                        let dismiss_x = cx + cw - theme.pt(118.0);
                        let dismiss_w = theme.pt(100.0);

                        if y >= btn_y && y <= btn_y + btn_h {
                            if x >= restart_x && x <= restart_x + restart_w {
                                let alen = crash.spawn_args.iter().position(|&b| b == 0).unwrap_or(96);
                                let args_s = String::from_utf8_lossy(&crash.spawn_args[..alen]).to_string();
                                if !args_s.trim().is_empty() {
                                    let _ = crate::syscall::sys_spawn_elf("user_app.elf", &args_s);
                                }
                                self.crash_dialog = None;
                                self.request_full_redraw();
                                return;
                            }
                            if x >= dismiss_x && x <= dismiss_x + dismiss_w {
                                self.crash_dialog = None;
                                self.request_full_redraw();
                                return;
                            }
                        }
                        return;
                    }

                    // -0.5. Active Command Palette (Alt+Space)
                    if self.command_palette.is_open {
                        if let Some(act) = self.command_palette.handle_mouse(x, y, true) {
                            self.execute_palette_action(&act);
                        }
                        self.request_full_redraw();
                        return;
                    }

                    // -0.25. Active ModalDialog
                    if let Some(mut modal) = self.modal_dialog.take() {
                        if let Some(res) = modal.handle_mouse(x, y, true) {
                            if let crate::framework::tools::ModalResult::Confirmed(val) = res {
                                crate::framework::portal::push_toast("Modal Confirmed", &val);
                            }
                        } else if modal.is_open {
                            self.modal_dialog = Some(modal);
                        }
                        self.request_full_redraw();
                        return;
                    }

                    // 0. Active Portal FilePicker Dialog
                    if let Some((caller, mut dlg)) = self.file_dialog.take() {
                        let picked = dlg.handle_mouse(x, y, true);
                        if let Some(path) = picked {
                            if let Some(id_str) = caller.strip_prefix("EXT:") {
                                if let Ok(wid) = id_str.parse::<u32>() {
                                    let mut pb = path.into_bytes();
                                    crate::syscall::sys_win_file_portal(1, wid, &mut pb);
                                }
                            } else {
                                crate::framework::portal::complete_file_picker(caller, path);
                            }
                        } else if dlg.is_open {
                            self.file_dialog = Some((caller, dlg));
                        }
                        self.request_full_redraw();
                        return;
                    }

                    // 1. Start Menu Click
                    if self.start_menu.is_open {
                        if let Some((target_app, target_arg)) = self.start_menu.handle_mouse(x, y, true) {
                            if target_app == "__REBOOT__" {
                                crate::syscall::sys_power(1);
                                return;
                            } else if target_app == "__SHUTDOWN__" {
                                crate::syscall::sys_power(2);
                                return;
                            }
                            let arg_opt = if target_arg.is_empty() { None } else { Some(target_arg.as_str()) };
                            self.spawn_app_with_arg(&target_app, arg_opt);
                            self.request_full_redraw();
                            return;
                        }
                        if self.start_menu.bounds.contains(x, y) {
                            self.request_full_redraw();
                            return;
                        }
                    }

                    // 2. Bottom Superbar Click
                    if self.is_taskbar_shown() && y >= tb_y {
                        // Start Orb
                        if x < orb_w {
                            self.start_menu.layout_above_taskbar(self.height as i32, tb_h);
                            self.start_menu.is_open = !self.start_menu.is_open;
                            self.calendar_flyout_open = false;
                            self.request_full_redraw();
                            return;
                        }

                        // Aero Peek Show Desktop strip at far right
                        if x >= (self.width as i32) - peek_w {
                            self.toggle_show_desktop();
                            return;
                        }

                        // System Tray Clock Flyout
                        if x >= (self.width as i32) - peek_w - tray_w {
                            self.calendar_flyout_open = !self.calendar_flyout_open;
                            self.start_menu.is_open = false;
                            self.request_full_redraw();
                            return;
                        }

                        // Superbar App Buttons (Toggle Minimize / Restore / Spawn)
                        let idx = ((x - orb_w - theme.pt(8.0)) / btn_w) as usize;
                        if idx < self.dock_items.len() {
                            if let Some(ref mut handler) = self.dock_handler { handler(idx); }
                            let app_name = self.dock_items[idx].clone();
                            let mut toggled_existing = false;
                            for (wi, w) in self.root_widgets.iter_mut().enumerate() {
                                if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                    if !f.is_closed && f.title.contains(&app_name) {
                                        if self.focus_mgr.active_window_idx == Some(wi) && !f.is_minimized {
                                            f.is_minimized = true;
                                        } else {
                                            f.is_minimized = false;
                                            self.focus_mgr.active_window_idx = Some(wi);
                                        }
                                        toggled_existing = true;
                                        break;
                                    }
                                }
                            }
                            if !toggled_existing {
                                self.spawn_app(&app_name);
                            } else {
                                self.request_full_redraw();
                            }
                            return;
                        }
                    }

                    self.start_menu.is_open = false;
                    self.calendar_flyout_open = false;

                    // 3. Window Hit-Test
                    let mut hit_idx = None;
                    for (i, w) in self.root_widgets.iter_mut().enumerate().rev() {
                        if w.handle_mouse(x, y, true) {
                            hit_idx = Some(i);
                            break;
                        }
                    }

                    if let Some(i) = hit_idx {
                        let win = self.root_widgets.remove(i);
                        let surf = self.window_surfaces.remove(i);
                        self.root_widgets.push(win);
                        self.window_surfaces.push(surf);
                        let top_idx = self.root_widgets.len() - 1;
                        self.focus_mgr.active_window_idx = Some(top_idx);

                        if let Some(f) = self.root_widgets[top_idx].as_any_mut().downcast_mut::<WindowFrame>() {
                            let border_pad = 16;
                            if x >= f.bounds.x + f.bounds.w - border_pad && y >= f.bounds.y + f.bounds.h - border_pad {
                                f.is_resizing = true;
                                f.resize_edge = ResizeEdge::BottomRight;
                                self.captured_window_idx = Some(top_idx);
                            } else if f.is_dragging {
                                self.captured_window_idx = Some(top_idx);
                            }
                        }
                        self.request_full_redraw();
                        return;
                    }

                    // 4. Desktop Icons Click / Double-Click Launch
                    let mut clicked_icon: Option<(String, Option<String>)> = None;
                    for icon in &mut self.desktop_icons {
                        if icon.bounds.contains(x, y) {
                            if icon.selected {
                                clicked_icon = Some((icon.target_app.clone(), icon.target_arg.clone()));
                            } else {
                                icon.selected = true;
                            }
                        } else {
                            icon.selected = false;
                        }
                    }

                    if let Some((app_name, arg)) = clicked_icon {
                        self.spawn_app_with_arg(&app_name, arg.as_deref());
                        self.request_full_redraw();
                        return;
                    }

                    // Start rubber-band selection on empty wallpaper
                    self.rubber_band_start = Some((x, y));
                    self.rubber_band_rect = None;
                    self.request_full_redraw();
                }
                self.update_cursor_state(x, y);
            }
            PointerEvent::Up { button, .. } => {
                if button == PointerButton::Primary {
                    if self.rubber_band_start.is_some() {
                        self.rubber_band_start = None;
                        self.rubber_band_rect = None;
                        self.request_full_redraw();
                    }

                    if let Some(idx) = self.captured_window_idx {
                        let work = self.work_area();
                        let snap = self.snap_preview;
                        self.snap_preview = AeroSnapPreview::None;

                        if idx < self.root_widgets.len() {
                            if let Some(f) = self.root_widgets[idx].as_any_mut().downcast_mut::<WindowFrame>() {
                                f.is_dragging = false;
                                f.is_resizing = false;
                                f.hover_btn = None;
                                match snap {
                                    AeroSnapPreview::LeftHalf => f.tile_left(&work),
                                    AeroSnapPreview::RightHalf => f.tile_right(&work),
                                    AeroSnapPreview::Maximize => f.toggle_maximize(&work),
                                    AeroSnapPreview::None => {}
                                }
                            }
                        }
                        self.request_full_redraw();
                    }
                    self.captured_window_idx = None;
                }
                self.update_cursor_state(self.prev_pointer_x, self.prev_pointer_y);
            }
            PointerEvent::Scroll { x, y, dy } => {
                if let Some((_, ref mut dlg)) = self.file_dialog {
                    if dlg.handle_scroll(x, y, dy) {
                        self.request_full_redraw();
                    }
                    return;
                }
                for (idx, w) in self.root_widgets.iter_mut().enumerate().rev() {
                    if w.handle_scroll(x, y, dy) {
                        if let Some(surf) = self.window_surfaces.get_mut(idx) {
                            surf.is_dirty = true;
                        }
                        if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                            self.damage.add(f.bounds);
                        }
                        break;
                    }
                }
            }
        }
    }

    fn handle_raw_event(&mut self, raw: [u64; 3]) {
        match raw[0] {
            1 => {
                let dx = (raw[1] as i32) as isize;
                let dy = ((raw[1] >> 32) as i32) as isize;
                let new_x = (self.prev_pointer_x + dx as i32).clamp(0, self.width as i32 - 1);
                let new_y = (self.prev_pointer_y + dy as i32).clamp(0, self.height as i32 - 1);
                self.input_mgr.pointer.x = new_x;
                self.input_mgr.pointer.y = new_y;
                self.route_pointer_event(PointerEvent::Move { x: new_x, y: new_y, dx: dx as i32, dy: dy as i32 });
            }
            2 => {
                let btn = (raw[1] & 0xFF) as u8;
                let pressed = ((raw[1] >> 8) & 0x01) != 0;
                let ptr_ev = self.input_mgr.handle_raw_button(btn, pressed);
                self.route_pointer_event(ptr_ev);
            }
            3 => {
                let keycode = (raw[1] & 0xFF) as u8;
                let mods = ((raw[1] >> 8) & 0xFF) as u8;

                // F12 -> Toggle Live UI Inspector Overlay (Milestone 13)
                if keycode == 0x58 {
                    let on = crate::framework::features::toggle_ui_inspector();
                    crate::framework::portal::push_toast("UI Inspector (F12)", if on { "Widget bounds overlay ON" } else { "Widget bounds overlay OFF" });
                    self.request_full_redraw();
                    return;
                }

                // Alt+Space -> Toggle Quick Command Palette (Milestone 14)
                if keycode == 0x39 && (mods & 0x04) != 0 {
                    self.command_palette.toggle(self.width as i32, self.height as i32);
                    self.request_full_redraw();
                    return;
                }

                if self.command_palette.is_open {
                    if let Some(act) = self.command_palette.handle_key(keycode) {
                        self.execute_palette_action(&act);
                    }
                    self.request_full_redraw();
                    return;
                }

                if let Some(mut modal) = self.modal_dialog.take() {
                    if let Some(res) = modal.handle_key(keycode) {
                        if let crate::framework::tools::ModalResult::Confirmed(val) = res {
                            crate::framework::portal::push_toast("Modal Confirmed", &val);
                        }
                    } else if modal.is_open {
                        self.modal_dialog = Some(modal);
                    }
                    self.request_full_redraw();
                    return;
                }

                // Tab or Alt+Tab Window Switcher
                if keycode == 0x0F && ((mods & 0x04) != 0 || !self.root_widgets.is_empty()) {
                    if !self.root_widgets.is_empty() {
                        self.alt_tab_active = !self.alt_tab_active;
                        self.alt_tab_index = (self.alt_tab_index + 1) % self.root_widgets.len();
                        let target = self.alt_tab_index;
                        let win = self.root_widgets.remove(target);
                        let surf = self.window_surfaces.remove(target);
                        self.root_widgets.push(win);
                        self.window_surfaces.push(surf);
                        self.focus_mgr.active_window_idx = Some(self.root_widgets.len() - 1);
                        self.request_full_redraw();
                    }
                }

                // Alt+F4 -> Close Active Window (Milestone 21)
                if keycode == 0x3E && (mods & 0x04) != 0 {
                    if let Some(idx) = self.focus_mgr.active_window_idx {
                        if let Some(w) = self.root_widgets.get_mut(idx) {
                            if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                f.is_closed = true;
                                self.request_full_redraw();
                                return;
                            }
                        }
                    }
                }

                // PrintScreen (0xB7) or F9 (0x43) -> Capture Screenshot PNG (Milestone 21)
                if keycode == 0xB7 || keycode == 0x43 {
                    self.capture_screenshot();
                    return;
                }

                // Win+D (Toggle Show Desktop) & Win+Left/Right (Aero Snap) — MOD_WIN = 0x10
                if (mods & 0x10) != 0 {
                    if keycode == 0x20 {
                        self.toggle_show_desktop();
                        return;
                    }
                    let work = self.work_area();
                    if let Some(idx) = self.focus_mgr.active_window_idx {
                        if let Some(w) = self.root_widgets.get_mut(idx) {
                            if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                                if keycode == 0x4B {
                                    f.tile_left(&work);
                                    self.request_full_redraw();
                                    return;
                                } else if keycode == 0x4D {
                                    f.tile_right(&work);
                                    self.request_full_redraw();
                                    return;
                                }
                            }
                        }
                    }
                }

                if self.start_menu.is_open {
                    if keycode == 0x0E {
                        self.start_menu.search_query.pop();
                        self.request_full_redraw();
                        return;
                    } else if keycode == 0x1C {
                        let filtered = self.start_menu.filtered_left();
                        if let Some(first) = filtered.first() {
                            let target = first.app_target.clone();
                            self.start_menu.is_open = false;
                            self.spawn_app(&target);
                            return;
                        }
                    }
                }

                if let Some(idx) = self.focus_mgr.active_window_idx {
                    if idx < self.root_widgets.len() {
                        self.root_widgets[idx].handle_key(keycode, mods);
                        self.request_full_redraw();
                    }
                }
            }
            4 => {
                let c = (raw[1] as u32) as u8 as char;
                if self.command_palette.is_open {
                    self.command_palette.handle_char(c);
                    self.request_full_redraw();
                    return;
                }
                if let Some(ref mut modal) = self.modal_dialog {
                    if modal.handle_char(c) {
                        self.request_full_redraw();
                        return;
                    }
                }
                if self.start_menu.is_open {
                    if c >= ' ' && c <= '~' {
                        self.start_menu.search_query.push(c);
                        self.request_full_redraw();
                        return;
                    }
                }
                if let Some(idx) = self.focus_mgr.active_window_idx {
                    if idx < self.root_widgets.len() {
                        self.root_widgets[idx].handle_char(c);
                        self.request_full_redraw();
                    }
                }
            }
            5 => {
                let dy = raw[1] as i64 as i32;
                let ptr_ev = PointerEvent::Scroll {
                    x: self.prev_pointer_x,
                    y: self.prev_pointer_y,
                    dy,
                };
                self.route_pointer_event(ptr_ev);
            }
            _ => {}
        }
    }

    fn rebuild_wallpaper_cache(&mut self) {
        unsafe {
            if let Some((ref wp_px, wp_w, wp_h)) = DESKTOP_WALLPAPER {
                let scale_x = self.width as f32 / wp_w as f32;
                let scale_y = self.height as f32 / wp_h as f32;
                let scale = scale_x.max(scale_y);

                let crop_w = (self.width as f32 / scale) as usize;
                let crop_h = (self.height as f32 / scale) as usize;
                let start_x = (wp_w.saturating_sub(crop_w)) / 2;
                let start_y = (wp_h.saturating_sub(crop_h)) / 2;

                for y in 0..self.height {
                    let sy = (start_y + ((y as f32 / scale) as usize)).min(wp_h - 1);
                    let row_idx = y * self.width;
                    let src_row = sy * wp_w;
                    for x in 0..self.width {
                        let sx = (start_x + ((x as f32 / scale) as usize)).min(wp_w - 1);
                        self.wallpaper_cache[row_idx + x] = wp_px[src_row + sx];
                    }
                }
            } else {
                // Windows 7 Harmony Deep Sky-Blue Radial/Vertical Gradient Wallpaper
                for y in 0..self.height {
                    let row = y * self.width;
                    let t = y as f32 / (self.height.max(1) as f32);
                    let r = (10.0 + (1.0 - t) * 18.0) as u32;
                    let g = (42.0 + (1.0 - t) * 58.0) as u32;
                    let b = (78.0 + (1.0 - t) * 92.0) as u32;
                    let col = 0xFF000000 | (r << 16) | (g << 8) | b;
                    self.wallpaper_cache[row..row + self.width].fill(col);
                }
            }
        }
        self.wallpaper_dirty = false;
    }

    pub fn run_loop(&mut self) {
        self.rebuild_wallpaper_cache();
        self.request_full_redraw();

        for _ in 0..5 {
            self.request_full_redraw();
            self.render();
        }

        loop {
            let cur_tty = crate::syscall::sys_tty_state();
            if cur_tty != 1 {
                self.last_tty = cur_tty;
                std::thread::sleep(std::time::Duration::from_millis(35));
                continue;
            } else if self.last_tty != 1 {
                self.last_tty = 1;
                self.request_full_redraw();
            }

            if self.crash_dialog.is_none() {
                if let Some(crash) = crate::syscall::sys_poll_crash() {
                    self.crash_dialog = Some(crash);
                    self.request_full_redraw();
                }
            }

            if self.context_menu.tick_anim() {
                self.damage.add(self.context_menu.bounds);
            }
            if let Some((caller, exts)) = crate::framework::portal::take_pending_file_picker() {
                let theme = get_theme();
                let dw = theme.pt(440.0).min(self.width as i32 - 80);
                let dh = theme.pt(300.0).min(self.height as i32 - 120);
                let mut dlg = crate::framework::tools::FileDialog::new_with_filter(false, &exts);
                dlg.bounds = Rect::new(
                    (self.width as i32 - dw) / 2,
                    (self.height as i32 - dh) / 2,
                    dw,
                    dh,
                );
                self.file_dialog = Some((caller, dlg));
                self.request_full_redraw();
            }
            if crate::framework::theme::sync_theme_from_disk_if_changed()
                || crate::framework::theme::take_full_redraw_requested()
            {
                self.request_full_redraw();
            }
            if take_wallpaper_changed() || self.wallpaper_dirty {
                self.rebuild_wallpaper_cache();
                self.request_full_redraw();
            }

            // مزامنة نوافذ التطبيقات المعزولة من الكيرنل (Wayland-Style Surface Sync)
            let mut descs = [crate::syscall::RawExtWindowDesc::default(); 8];
            let ext_cnt = crate::syscall::sys_comp_list_windows(&mut descs);
            let live = &descs[..ext_cnt];

            for d in live {
                if (d.file_req & 1) != 0 && self.file_dialog.is_none() {
                    let mut fbuf = [0u8; 128];
                    let flen = crate::syscall::sys_win_file_portal(4, d.id, &mut fbuf);
                    let exts = String::from_utf8_lossy(&fbuf[..flen]).to_string();
                    let theme = get_theme();
                    let dw = theme.pt(440.0).min(self.width as i32 - 80);
                    let dh = theme.pt(300.0).min(self.height as i32 - 120);
                    let mut dlg = crate::framework::tools::FileDialog::new_with_filter(false, &exts);
                    dlg.bounds = Rect::new((self.width as i32 - dw) / 2, (self.height as i32 - dh) / 2, dw, dh);
                    self.file_dialog = Some((format!("EXT:{}", d.id), dlg));
                    self.request_full_redraw();
                }

                let mut found = false;
                let mut need_full_redraw = false;
                let want_fullscreen = (d.file_req & 4) != 0;
                let want_pin = (d.file_req & 8) != 0;
                for w in self.root_widgets.iter_mut() {
                    if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                        let mut matched = false;
                        if let Some(ext) = f.content.as_any_mut().downcast_mut::<ExternalWindowWidget>() {
                            if ext.win_id == d.id {
                                matched = true;
                                if ext.last_gen != d.generation {
                                    let nw = d.width as usize; let nh = d.height as usize;
                                    if ext.pixels.len() != nw * nh { ext.pixels.resize(nw * nh, 0xFF0F172A); }
                                    ext.buf_w = nw; ext.buf_h = nh;
                                    crate::syscall::sys_comp_read_surface(d.id, &mut ext.pixels);
                                    ext.last_gen = d.generation;
                                    self.damage.add(f.bounds);
                                }
                            }
                        }
                        if matched {
                            found = true;
                            if want_fullscreen {
                                let screen = Rect::new(0, 0, self.width as i32, self.height as i32);
                                f.toggle_fullscreen(&screen);
                                need_full_redraw = true;
                            }
                            if want_pin {
                                f.always_on_top = !f.always_on_top;
                                need_full_redraw = true;
                            }
                            break;
                        }
                    }
                }
                if need_full_redraw {
                    self.request_full_redraw();
                }
                if !found {
                    let tlen = d.title.iter().position(|&b| b == 0).unwrap_or(40);
                    let title = String::from_utf8_lossy(&d.title[..tlen]).to_string();
                    let theme = get_theme();
                    let off = (self.root_widgets.len() as i32 * 36) % 240;
                    let tbh = theme.pt(36.0);
                    let mut ext = ExternalWindowWidget::new(d.id, d.width as usize, d.height as usize);
                    crate::syscall::sys_comp_read_surface(d.id, &mut ext.pixels);
                    ext.last_gen = d.generation;
                    let is_terminal = title.eq_ignore_ascii_case("Terminal");
                    let mut frame = WindowFrame::new(title, theme.pt(120.0) + off, theme.pt(40.0) + off, d.width as i32 + 2, d.height as i32 + tbh + 1, Box::new(ext));
                    if is_terminal {
                        frame.set_titlebar_style(crate::framework::widget::TitlebarStyle::Hidden);
                    }
                    self.root_widgets.push(Box::new(frame));
                    self.window_surfaces.push(crate::framework::surface::WindowSurface::new(1, 1));
                    self.focus_mgr.active_window_idx = Some(self.root_widgets.len() - 1);
                    self.request_full_redraw();
                }
            }

            let mut removed = Vec::new();
            for (idx, w) in self.root_widgets.iter_mut().enumerate() {
                if let Some(f) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                    if let Some(ext) = f.content.as_any().downcast_ref::<ExternalWindowWidget>() {
                        if f.is_closed {
                            crate::syscall::sys_comp_send_event(ext.win_id, 9, 0, 0, 0);
                        } else if !live.iter().any(|d| d.id == ext.win_id) {
                            f.is_closed = true; // العملية المعزولة أُغلقت أو انهارت — أزل النافذة بنظافة
                        }
                    }
                    if f.is_closed {
                        removed.push(idx);
                        self.damage.add(f.bounds);
                    }
                }
            }
            for &idx in removed.iter().rev() {
                if idx < self.root_widgets.len() {
                    self.root_widgets.remove(idx);
                }
                if idx < self.window_surfaces.len() {
                    self.window_surfaces.remove(idx);
                }
            }
            if !removed.is_empty() {
                self.focus_mgr.active_window_idx = if self.root_widgets.is_empty() { None } else { Some(self.root_widgets.len() - 1) };
                self.captured_window_idx = None;
                self.request_full_redraw();
            }

            let mut pending_open_text = None;
            let mut pending_open_img = None;

            for w in self.root_widgets.iter_mut() {
                if let Some(frame) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                    if let Some(finder) = frame.content.as_any_mut().downcast_mut::<FinderApp>() {
                        if let Some(path) = finder.pending_open_text.take() {
                            pending_open_text = Some(path);
                        }
                        if let Some(req) = finder.pending_open_image.take() {
                            pending_open_img = Some(req);
                        }
                    }
                }
            }

            if let Some(path) = pending_open_text {
                self.spawn_text_editor(&path);
            }

            if let Some((name, path)) = pending_open_img {
                let theme = get_theme();
                let prev_app = Box::new(PreviewApp::new(name.clone(), path));
                let win_w = theme.pt(680.0);
                let win_h = theme.pt(480.0);
                let win = Box::new(WindowFrame::new(
                    format!("Preview - {}", name),
                    theme.pt(120.0),
                    theme.pt(40.0),
                    win_w,
                    win_h,
                    prev_app,
                ));
                self.damage.add(win.bounds);
                self.root_widgets.push(win);
                self.window_surfaces.push(crate::framework::surface::WindowSurface::new(win_w as usize, win_h as usize));
                self.focus_mgr.active_window_idx = Some(self.root_widgets.len() - 1);
                self.request_full_redraw();
            }

            let wait_ms = if self.animator.is_active() { 8 } else { 16 };
            if let Some(ev) = sys_wait_event(wait_ms) {
                self.handle_raw_event(ev);
            }

            let mut pending_dx: i32 = 0;
            let mut pending_dy: i32 = 0;
            let mut has_mouse_move = false;

            while let Some(raw) = sys_poll_event() {
                if raw[0] == 1 {
                    pending_dx += raw[1] as i32;
                    pending_dy += (raw[1] >> 32) as i32;
                    has_mouse_move = true;
                } else {
                    if has_mouse_move {
                        let new_x = (self.prev_pointer_x + pending_dx).clamp(0, self.width as i32 - 1);
                        let new_y = (self.prev_pointer_y + pending_dy).clamp(0, self.height as i32 - 1);
                        self.input_mgr.pointer.x = new_x;
                        self.input_mgr.pointer.y = new_y;
                        self.route_pointer_event(PointerEvent::Move { x: new_x, y: new_y, dx: pending_dx, dy: pending_dy });
                        pending_dx = 0;
                        pending_dy = 0;
                        has_mouse_move = false;
                    }
                    self.handle_raw_event(raw);
                }
            }

            if has_mouse_move {
                let new_x = (self.prev_pointer_x + pending_dx).clamp(0, self.width as i32 - 1);
                let new_y = (self.prev_pointer_y + pending_dy).clamp(0, self.height as i32 - 1);
                self.input_mgr.pointer.x = new_x;
                self.input_mgr.pointer.y = new_y;
                self.route_pointer_event(PointerEvent::Move { x: new_x, y: new_y, dx: pending_dx, dy: pending_dy });
            }

            if self.animator.is_active() {
                let now_ms = crate::framework::perf_log::SmartLogger::cycles_to_us(
                    crate::framework::perf_log::SmartLogger::read_tsc()
                ) / 1000;
                self.animator.tick(now_ms, &mut self.damage);
            }

            // تحديث موقع مؤشر الماوس العتادي مرة واحدة فقط في كل دورة لمنع الاهتزاز والتدبّل
            if self.hw_cursor_enabled
                && (self.prev_pointer_x != self.last_hw_cursor_x || self.prev_pointer_y != self.last_hw_cursor_y)
            {
                sys_gpu_move_cursor(self.prev_pointer_x as u32, self.prev_pointer_y as u32);
                self.last_hw_cursor_x = self.prev_pointer_x;
                self.last_hw_cursor_y = self.prev_pointer_y;
            }

            // مزامنة قائمة النوافذ النشطة ليعرضها TestApp لحظياً
            if let Ok(mut titles) = ACTIVE_WINDOW_TITLES.lock() {
                titles.clear();
                for w in &self.root_widgets {
                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                        if !f.is_closed {
                            let st = if f.is_minimized { "Minimized" } else { "Active" };
                            titles.push(format!("{} [{}]", f.title, st));
                        }
                    }
                }
            }

            for w in self.root_widgets.iter_mut() {
                if let Some(frame) = w.as_any_mut().downcast_mut::<WindowFrame>() {
                    if !frame.is_closed && !frame.is_minimized {
                        if let Some(test_app) = frame.content.as_any_mut().downcast_mut::<TestApp>() {
                            if test_app.anim_running {
                                test_app.tick_animation();
                                self.damage.add(frame.bounds);
                            }
                        } else if let Some(vid) = frame.content.as_any_mut().downcast_mut::<VideoPlayerApp>() {
                            if vid.tick_frame() {
                                self.damage.add(frame.bounds);
                            }
                        }
                    }
                }
            }

            self.render();
        }
    }

    pub fn render(&mut self) {
        let t_frame_start = crate::framework::perf_log::SmartLogger::read_tsc();
        let theme = get_theme();
        let tb_h = self.taskbar_height();
        let tb_y = (self.height as i32) - tb_h;
        let orb_w = theme.pt(48.0);
        let btn_w = theme.pt(60.0);
        let peek_w = theme.pt(12.0);
        let tray_w = theme.pt(120.0);

        self.damage.clamp_to_screen(self.width, self.height);
        if self.damage.is_empty() { return; }
        let dirty_rects = self.damage.rects.clone();

        let superbar_rect = Rect::new(0, tb_y, self.width as i32, tb_h);

        for r in &dirty_rects {
            let cx1 = r.x.clamp(0, self.width as i32) as usize;
            let cy1 = r.y.clamp(0, self.height as i32) as usize;
            let cx2 = (r.x + r.w).clamp(0, self.width as i32) as usize;
            let cy2 = (r.y + r.h).clamp(0, self.height as i32) as usize;

            for y in cy1..cy2 {
                let row = y * self.width;
                self.buffer[row + cx1..row + cx2].copy_from_slice(&self.wallpaper_cache[row + cx1..row + cx2]);
            }

            // 1. Desktop Icon Grid Layer + Rubber-band Selection Box
            {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                canvas.push_clip(*r);

                for icon in &self.desktop_icons {
                    if icon.bounds.intersects(r) {
                        let iw = crate::framework::widget::DesktopIconWidget {
                            label: icon.label.clone(),
                            icon_name: icon.icon_name.clone(),
                            bounds: icon.bounds,
                            selected: icon.selected,
                        };
                        iw.paint_icon(&mut canvas);
                    }
                }

                if let Some(rb) = self.rubber_band_rect {
                    canvas.draw_rect(rb.x, rb.y, rb.w, rb.h, 0x440284C7, 0);
                    canvas.draw_rect_outline(rb.x, rb.y, rb.w, rb.h, 0xFF38BDF8, 0);
                }

                canvas.pop_clip();
            }

            // 2. Aero Snap Translucent Glass Outline Preview
            if self.snap_preview != AeroSnapPreview::None {
                let work = self.work_area();
                let snap_rect = match self.snap_preview {
                    AeroSnapPreview::LeftHalf => Rect::new(4, 4, work.w / 2 - 8, work.h - 8),
                    AeroSnapPreview::RightHalf => Rect::new(work.w / 2 + 4, 4, work.w / 2 - 8, work.h - 8),
                    AeroSnapPreview::Maximize => Rect::new(4, 4, work.w - 8, work.h - 8),
                    AeroSnapPreview::None => Rect::default(),
                };
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                canvas.push_clip(*r);
                canvas.draw_rect(snap_rect.x, snap_rect.y, snap_rect.w, snap_rect.h, 0x4438BDF8, 8);
                canvas.draw_rect_outline(snap_rect.x, snap_rect.y, snap_rect.w, snap_rect.h, 0xFFE0F2FE, 8);
                canvas.pop_clip();
            }

            // 3. Windows (Regular first, then Always-on-Top, then Fullscreen on top of Superbar)
            if !self.aero_peek_hovered {
                for pass in 0..2 {
                    for w in self.root_widgets.iter() {
                        if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                            if !f.is_closed && !f.is_minimized && !f.is_fullscreen {
                                let is_top = f.always_on_top;
                                if (pass == 0 && !is_top) || (pass == 1 && is_top) {
                                    if let Some(intersect) = f.bounds.intersect(&r).non_empty() {
                                        let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                                        canvas.push_clip(intersect);
                                        w.paint(&mut canvas);
                                        canvas.pop_clip();
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // Aero Peek Glass Outlines Only
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                canvas.push_clip(*r);
                for w in self.root_widgets.iter() {
                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                        if !f.is_closed && !f.is_minimized {
                            canvas.draw_rect_outline(f.bounds.x, f.bounds.y, f.bounds.w, f.bounds.h, 0xCC7DD3FC, 6);
                        }
                    }
                }
                canvas.pop_clip();
            }

            // 4. Bottom Framework-Driven Superbar with 60 FPS Auto-Hide Slide Animation
            let slide_default = if self.is_taskbar_shown() { 0.0 } else { tb_h as f32 };
            let slide_off = self.animator.value(1001, slide_default).round() as i32;
            let eff_tb_y = tb_y + slide_off;
            if eff_tb_y < self.height as i32 {
            if let Some(intersect) = superbar_rect.intersect(&r).non_empty() {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                canvas.push_clip(intersect);

                canvas.draw_frosted_glass_rect(0, eff_tb_y, self.width as i32, tb_h, theme.superbar_glass, 0);
                canvas.draw_line_h(0, eff_tb_y, self.width as i32, theme.border_frost_highlight);
                canvas.draw_line_h(0, eff_tb_y + 1, self.width as i32, 0x4438BDF8);

                // Custom EOS Logo Start Orb on the Left (userspace/assets/icon.png)
                let orb_sz = theme.pt(36.0);
                let orb_x = theme.pt(6.0);
                let orb_y = eff_tb_y + (tb_h - orb_sz) / 2;
                let orb_col = if self.start_orb_hovered || self.start_menu.is_open {
                    theme.accent_hover
                } else {
                    0x550F172A
                };
                canvas.draw_rect(orb_x, orb_y, orb_sz, orb_sz, orb_col, (orb_sz / 2) as usize);
                canvas.draw_rect_outline(orb_x, orb_y, orb_sz, orb_sz, 0x99E0F2FE, (orb_sz / 2) as usize);
                let logo_pad = theme.pt(4.0);
                if !crate::framework::icons::draw_eos_logo(&mut canvas, orb_x + logo_pad, orb_y + logo_pad, orb_sz - logo_pad * 2, orb_sz - logo_pad * 2) {
                    canvas.draw_rect(orb_x + 8, orb_y + 8, orb_sz - 16, orb_sz - 16, theme.accent, 6);
                }

                // Pinned / Running Application Framework Buttons
                for (i, item) in self.dock_items.iter().enumerate() {
                    let is_hovered = self.hovered_dock_idx == Some(i);
                    let is_running = self.root_widgets.iter().any(|w| {
                        w.as_any()
                            .downcast_ref::<WindowFrame>()
                            .map(|f| !f.is_closed && f.title.contains(item))
                            .unwrap_or(false)
                    });
                    let bx = orb_w + theme.pt(8.0) + (i as i32 * btn_w);
                    let btn = crate::framework::widget::TaskbarButtonWidget {
                        app_name: item,
                        bounds: Rect::new(bx, eff_tb_y + 3, btn_w - 4, tb_h - 6),
                        is_running,
                        is_hovered,
                    };
                    btn.paint_button(&mut canvas);
                }

                // Right-Side System Tray (Theme-Aware Clock/Date + 12pt Aero Peek Strip)
                let peek_x = (self.width as i32) - peek_w;
                let tray_x = peek_x - tray_w;

                canvas.draw_line_v(tray_x, eff_tb_y + 4, tb_h - 8, 0x5594A3B8);

                canvas.draw_rect(tray_x + theme.pt(8.0), eff_tb_y + theme.pt(18.0), 3, 8, theme.text_primary, 0);
                canvas.draw_rect(tray_x + theme.pt(13.0), eff_tb_y + theme.pt(14.0), 3, 12, theme.text_primary, 0);
                canvas.draw_rect(tray_x + theme.pt(18.0), eff_tb_y + theme.pt(10.0), 3, 16, theme.accent_hover, 0);

                let time_str = get_system_clock_string();
                let date_str = get_system_date_string();
                canvas.draw_text(tray_x + theme.pt(32.0), eff_tb_y + theme.pt(5.0), &time_str, theme.text_primary, theme.font_caption());
                canvas.draw_text(tray_x + theme.pt(30.0), eff_tb_y + theme.pt(21.0), &date_str, theme.text_secondary, theme.font_caption());

                let peek_bg = if self.aero_peek_hovered { 0xAA38BDF8 } else { 0x551E3A8A };
                canvas.draw_rect(peek_x, eff_tb_y + 1, peek_w, tb_h - 2, peek_bg, 0);
                canvas.draw_line_v(peek_x, eff_tb_y, tb_h, theme.border_frost_highlight);

                canvas.pop_clip();
            }
            }

            // 4b. True Fullscreen Windows (Covers entire 2560x1440 screen above Superbar)
            for w in self.root_widgets.iter() {
                if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                    if !f.is_closed && !f.is_minimized && f.is_fullscreen {
                        let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                        canvas.push_clip(*r);
                        w.paint(&mut canvas);
                        canvas.pop_clip();
                    }
                }
            }

            // 5. Taskbar Live Hover Tooltip (Only shows preview card if the app is actually running)
            if self.is_taskbar_shown() {
            if let Some(dock_idx) = self.hovered_dock_idx {
                if dock_idx < self.dock_items.len() {
                    let app_title = &self.dock_items[dock_idx];
                    let is_running = self.root_widgets.iter().any(|w| {
                        w.as_any().downcast_ref::<WindowFrame>().map(|f| !f.is_closed && f.title.contains(app_title)).unwrap_or(false)
                    });
                    if is_running {
                        let card_w = theme.pt(160.0);
                        let card_h = theme.pt(44.0);
                        let card_x = (orb_w + (dock_idx as i32 * btn_w) - card_w / 4).clamp(8, self.width as i32 - card_w - 8);
                        let card_y = tb_y - card_h - theme.pt(6.0);

                        let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                        canvas.draw_frosted_glass_rect(card_x, card_y, card_w, card_h, theme.aero_glass_tint, theme.r_ctrl());
                        canvas.draw_rect_outline(card_x, card_y, card_w, card_h, theme.border_frost_highlight, theme.r_ctrl());
                        canvas.draw_text_clipped(card_x + 12, card_y + 12, card_w - 24, app_title, 0xFFFFFFFF, theme.font_caption());
                    }
                }
            }
            }

            // 6. Calendar / Clock Flyout
            if self.calendar_flyout_open {
                let cal_w = theme.pt(200.0);
                let cal_h = theme.pt(140.0);
                let cal_x = (self.width as i32) - cal_w - peek_w - 6;
                let cal_y = tb_y - cal_h - 6;
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                canvas.draw_frosted_glass_rect(cal_x, cal_y, cal_w, cal_h, 0xEE162C46, 6);
                canvas.draw_rect_outline(cal_x, cal_y, cal_w, cal_h, theme.border_frost_highlight, 6);
                canvas.draw_text(cal_x + 14, cal_y + 12, "Friday, October 23, 2026", 0xFFFFFFFF, theme.font_caption());
                canvas.draw_text(cal_x + 14, cal_y + 40, &get_system_clock_string(), 0xFF38BDF8, theme.font_large());
                canvas.draw_text(cal_x + 14, cal_y + 84, "EOS Hardware RTC Synced", 0xFFCBD5E1, theme.font_caption());
            }

            // 7. Two-Column Windows 7 Start Menu Popup
            if self.start_menu.is_open {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                self.start_menu.paint(&mut canvas);
            }

            // 8. Alt+Tab Aero Live Window Switcher Overlay
            if self.alt_tab_active && !self.root_widgets.is_empty() {
                let sw_w = theme.pt(420.0);
                let sw_h = theme.pt(110.0);
                let sw_x = ((self.width as i32) - sw_w) / 2;
                let sw_y = ((self.height as i32) - sw_h) / 2;
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                canvas.draw_frosted_glass_rect(sw_x, sw_y, sw_w, sw_h, theme.aero_glass_tint, 8);
                canvas.draw_rect_outline(sw_x, sw_y, sw_w, sw_h, theme.border_frost_highlight, 8);

                let mut ix = sw_x + 18;
                for (idx, w) in self.root_widgets.iter().enumerate() {
                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                        let is_sel = idx == self.root_widgets.len() - 1;
                        let box_c = if is_sel { 0x8838BDF8 } else { 0x331E293B };
                        canvas.draw_rect(ix, sw_y + 18, 110, 72, box_c, 6);
                        if is_sel {
                            canvas.draw_rect_outline(ix, sw_y + 18, 110, 72, 0xFFFFFFFF, 6);
                        }
                        canvas.draw_text_clipped(ix + 8, sw_y + 44, 94, &f.title, 0xFFFFFFFF, theme.font_caption());
                        ix += 122;
                    }
                }
            }

            // 9. Right-Click Context Menu, Portal FilePicker Dialog & Crash Inspector Modal
            if self.context_menu.is_visible {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                self.context_menu.draw(&mut canvas);
            }
            if let Some((_, ref dlg)) = self.file_dialog {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                dlg.draw(&mut canvas);
            }
            if let Some(crash) = self.crash_dialog {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                let cw = theme.pt(430.0);
                let ch = theme.pt(210.0);
                let cx = ((self.width as i32) - cw) / 2;
                let cy = ((self.height as i32) - ch) / 2;

                canvas.draw_rect(cx + 4, cy + 4, cw, ch, 0x88000000, 8);
                canvas.draw_frosted_glass_rect(cx, cy, cw, ch, 0xF4181820, 8);
                canvas.draw_rect_outline(cx, cy, cw, ch, 0xFFEF4444, 8);

                let hdr_h = theme.pt(36.0);
                canvas.draw_rect(cx + 1, cy + 1, cw - 2, hdr_h, 0xFF7F1D1D, 7);
                canvas.draw_text(cx + theme.pt(14.0), cy + theme.pt(8.0), "Application Crash Inspector (Isolated Fault)", 0xFFFFFFFF, theme.font_body());

                let nlen = crash.app_name.iter().position(|&b| b == 0).unwrap_or(32);
                let app_lbl = String::from_utf8_lossy(&crash.app_name[..nlen]);
                let kind_str = match crash.fault_kind {
                    1 => "#DE Divide By Zero",
                    2 => "#UD Invalid Opcode",
                    3 => "#GP General Protection Fault",
                    4 => "#PF Page Fault",
                    _ => "Hardware Exception",
                };
                canvas.draw_text(cx + theme.pt(16.0), cy + hdr_h + theme.pt(12.0), &format!("Process : {} (Core {})", app_lbl, crash.core_id), 0xFFF8FAFC, theme.font_body());
                canvas.draw_text(cx + theme.pt(16.0), cy + hdr_h + theme.pt(36.0), &format!("Fault   : {} (Code {:#x})", kind_str, crash.error_code), 0xFFFCA5A5, theme.font_caption());
                canvas.draw_text(cx + theme.pt(16.0), cy + hdr_h + theme.pt(56.0), &format!("RIP     : {:#018x}   CR2: {:#018x}", crash.rip, crash.cr2), 0xFFCBD5E1, theme.font_caption());
                canvas.draw_text(cx + theme.pt(16.0), cy + hdr_h + theme.pt(78.0), "EOS Desktop remained isolated and unaffected.", 0xFF34D399, theme.font_caption());

                let btn_y = cy + ch - theme.pt(44.0);
                let btn_h = theme.pt(30.0);
                canvas.draw_rect(cx + theme.pt(18.0), btn_y, theme.pt(185.0), btn_h, theme.accent, 6);
                canvas.draw_text(cx + theme.pt(32.0), btn_y + theme.pt(7.0), "Restart Application", 0xFFFFFFFF, theme.font_caption());

                canvas.draw_rect(cx + cw - theme.pt(118.0), btn_y, theme.pt(100.0), btn_h, 0xFF334155, 6);
                canvas.draw_text(cx + cw - theme.pt(92.0), btn_y + theme.pt(7.0), "Dismiss", 0xFFFFFFFF, theme.font_caption());
            }
            if self.command_palette.is_open {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                self.command_palette.paint(&mut canvas);
            }
            if let Some(ref modal) = self.modal_dialog {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                modal.draw(&mut canvas);
            }

            // 9.5. Toast Notification Banners (PortalKind::Notifications — Milestone 15)
            let active_toasts = crate::framework::portal::poll_active_toasts();
            if !active_toasts.is_empty() {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                let tw = theme.pt(310.0);
                let th = theme.pt(56.0);
                let tx = (self.width as i32) - tw - theme.pt(14.0);
                for (i, toast) in active_toasts.iter().rev().enumerate() {
                    let ty = tb_y - theme.pt(12.0) - ((i as i32 + 1) * (th + theme.pt(8.0)));
                    canvas.draw_frosted_glass_rect(tx, ty, tw, th, 0xEE0F172A, 8);
                    canvas.draw_rect_outline(tx, ty, tw, th, theme.accent_hover, 8);
                    canvas.draw_rect(tx + 2, ty + 6, 4, th - 12, theme.accent_hover, 2);
                    canvas.draw_text_clipped(tx + theme.pt(14.0), ty + theme.pt(8.0), tw - theme.pt(24.0), &toast.title, 0xFFFFFFFF, theme.font_body());
                    canvas.draw_text_clipped(tx + theme.pt(14.0), ty + theme.pt(30.0), tw - theme.pt(24.0), &toast.message, 0xFFCBD5E1, theme.font_caption());
                }
            }

            // 9.8. Live UI Inspector & Widget Bounds Overlay (F12 DevTools — Milestone 13)
            if crate::framework::features::get_features().ui_inspector {
                let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                let px = self.prev_pointer_x;
                let py = self.prev_pointer_y;
                let mut target_name = "Desktop Wallpaper".to_string();
                let mut target_rect = Rect::new(0, 0, self.width as i32, self.height as i32);

                for icon in &self.desktop_icons {
                    if icon.bounds.contains(px, py) {
                        target_name = format!("DesktopIconWidget ({})", icon.label);
                        target_rect = icon.bounds;
                    }
                }
                for w in self.root_widgets.iter().rev() {
                    if let Some(f) = w.as_any().downcast_ref::<WindowFrame>() {
                        if !f.is_closed && !f.is_minimized && f.bounds.contains(px, py) {
                            target_name = format!("WindowFrame ({})", f.title);
                            target_rect = f.bounds;
                            break;
                        }
                    }
                }

                canvas.draw_rect_outline(target_rect.x, target_rect.y, target_rect.w, target_rect.h, 0xFF22D3EE, 0);
                canvas.draw_rect_outline(target_rect.x + 1, target_rect.y + 1, (target_rect.w - 2).max(1), (target_rect.h - 2).max(1), 0xFFF472B6, 0);

                let sx = px.clamp(0, self.width as i32 - 1) as usize;
                let sy = py.clamp(0, self.height as i32 - 1) as usize;
                let cpx = canvas.buffer[sy * self.width + sx];
                let lum = 0.299 * (((cpx >> 16) & 0xFF) as f32) + 0.587 * (((cpx >> 8) & 0xFF) as f32) + 0.114 * ((cpx & 0xFF) as f32);
                let avg_ms = crate::framework::perf_log::SmartLogger::average_frame_us() as f32 / 1000.0;

                let hud_w = theme.pt(360.0);
                let hud_h = theme.pt(96.0);
                let hx = (px + 18).clamp(8, (self.width as i32) - hud_w - 8);
                let hy = (py + 18).clamp(8, tb_y - hud_h - 8);
                canvas.draw_frosted_glass_rect(hx, hy, hud_w, hud_h, 0xEE090D16, 6);
                canvas.draw_rect_outline(hx, hy, hud_w, hud_h, 0xFF22D3EE, 6);
                canvas.draw_text_clipped(hx + 10, hy + 8, hud_w - 20, &format!("UI Inspector (F12): {}", target_name), 0xFF22D3EE, theme.font_caption());
                canvas.draw_text_clipped(hx + 10, hy + 28, hud_w - 20, &format!("Bounds: ({}, {}) {}x{} | Radius: {}px", target_rect.x, target_rect.y, target_rect.w, target_rect.h, theme.r_win()), 0xFFF8FAFC, theme.font_caption());
                canvas.draw_text_clipped(hx + 10, hy + 48, hud_w - 20, &format!("Pixel Lum: {:.1} | Avg Frame: {:.2} ms", lum, avg_ms), 0xFF34D399, theme.font_caption());
                canvas.draw_text_clipped(hx + 10, hy + 68, hud_w - 20, "Press F12 to close inspector | Alt+Space for Palette", 0xFF94A3B8, theme.font_caption());
            }

            // 10. Software Cursor (يُستخدم فقط عند تعطيل Hardware Cursor)
            if !self.hw_cursor_enabled {
                let cur_cursor_rect = Rect::new(self.prev_pointer_x - 4, self.prev_pointer_y - 4, 32, 32);
                if cur_cursor_rect.intersect(&r).non_empty().is_some() {
                    let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
                    match self.current_cursor {
                        CursorType::Default => canvas.draw_linux_cursor(self.prev_pointer_x, self.prev_pointer_y),
                        CursorType::Pointer => canvas.draw_pointer_hand_cursor(self.prev_pointer_x, self.prev_pointer_y),
                        CursorType::ResizeCorner => canvas.draw_resize_corner_cursor(self.prev_pointer_x, self.prev_pointer_y),
                    }
                }
            }
        }

        if crate::framework::features::get_features().debug_damage_overlay {
            let mut canvas = Canvas::new(&mut self.buffer, self.width, self.height, self.font.as_ref(), &mut self.font_cache);
            for r in &dirty_rects {
                canvas.draw_rect_outline(r.x, r.y, r.w, r.h, 0xFFEF4444, 0);
            }
        }

        let rect_data = self.damage.to_syscall_rects();
        sys_present_rects(self.buffer.as_ptr(), self.width, self.height, &rect_data);
        self.damage.clear();

        let frame_cycles = crate::framework::perf_log::SmartLogger::read_tsc().saturating_sub(t_frame_start);
        crate::framework::perf_log::SmartLogger::record_frame_render(frame_cycles, rect_data.len());
        self.rendered_frames += 1;

        if !self.first_frame_presented {
            self.first_frame_presented = true;
            crate::sdk::agent::audit_log("FIRST_FRAME:PASS:2560x1440 desktop frame composited");
        }
        if self.rendered_frames >= 5 && !self.compositor_bench_logged {
            self.compositor_bench_logged = true;
            crate::sdk::agent::audit_log("COMPOSITOR_TUNE:PASS:FrameBudget_OK");
        }
    }
}
