use crate::input::InputEvent;
use super::App;
use crate::gui::draw::{self, FrameBuffer};
use crate::gui::theme::Theme;
use core::sync::atomic::Ordering;

#[derive(PartialEq, Eq, Clone, Copy)]
enum SettingsTab { About, Appearance, Dock, Display, Network, Kernel, Processes }

pub struct SettingsApp { current_tab: SettingsTab, needs_redraw: bool }

impl SettingsApp {
    pub fn new() -> Self { Self { current_tab: SettingsTab::About, needs_redraw: true } }
}

impl App for SettingsApp {
    fn wants_redraw(&self) -> bool { self.needs_redraw }

    fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        let sidebar_w = 180usize.min(w / 3);
        draw::draw_rect(fb, x, y, sidebar_w, h, theme.titlebar_bg);
        draw::draw_line_v(fb, x + sidebar_w, y, h, theme.separator);

        let tabs = [(SettingsTab::About, "About"), (SettingsTab::Appearance, "Appearance"), (SettingsTab::Dock, "Dock & Bar"), (SettingsTab::Display, "Display"), (SettingsTab::Network, "Network"), (SettingsTab::Kernel, "Kernel"), (SettingsTab::Processes, "Activity")];

        for (i, (tab, label)) in tabs.iter().enumerate() {
            let row_y = y + 20 + (i * 36);
            if self.current_tab == *tab {
                draw::draw_rect_rounded(fb, x + 8, row_y, sidebar_w - 16, 30, 6, theme.accent, 255);
                draw::draw_text(fb, x + 16, row_y + 7, label, 0xFFFFFFFF, 1);
            } else { draw::draw_text(fb, x + 16, row_y + 7, label, theme.text_main, 1); }
        }

        let panel_x = x + sidebar_w + 24; let panel_w = w.saturating_sub(sidebar_w + 48); let panel_y = y + 24;
        draw::draw_rect(fb, panel_x, y, panel_w + 24, h, theme.window_bg);

        match self.current_tab {
            SettingsTab::About => {
                draw::draw_rect_rounded(fb, panel_x, panel_y, 64, 64, 16, theme.accent, 255);
                draw::draw_text(fb, panel_x + 18, panel_y + 20, "EOS", 0xFFFFFFFF, 2);
                draw::draw_text(fb, panel_x + 80, panel_y + 12, "EOS Operating System", theme.text_main, 2);
                draw::draw_text(fb, panel_x + 80, panel_y + 40, "macOS Edition - 60 FPS Responsive Kernel", theme.text_dim, 1);
                let card_y = panel_y + 90;
                draw::draw_rect_rounded(fb, panel_x, card_y, panel_w, 140, 10, theme.titlebar_bg, 255);
                draw::draw_text(fb, panel_x + 20, card_y + 16, "HARDWARE & KERNEL STATUS", theme.accent, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 46, &alloc::format!("CPU Topology : {} Symmetrical SMP Cores Online", crate::CORES_ONLINE.load(Ordering::Relaxed)), theme.text_main, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 74, &alloc::format!("Memory Heap  : {} MB Reserved Physical Pool", crate::config::CONFIG.default_heap_size_mb), theme.text_main, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 102, &alloc::format!("System Uptime: {} seconds since boot", crate::arch::x86_64::pit::get_uptime_seconds()), theme.text_dim, 1);
            }
            SettingsTab::Appearance => {
                draw::draw_text(fb, panel_x, panel_y, "Appearance & UI Scale", theme.text_main, 2);
                draw::draw_text(fb, panel_x, panel_y + 30, "Choose visual theme and typography scaling.", theme.text_dim, 1);
                let mode_y = panel_y + 70;
                let is_dark = crate::config::CONFIG.is_dark_mode();
                let toggle_col = if is_dark { 0xFF16A34A } else { 0xFF64748B };
                draw::draw_rect_rounded(fb, panel_x, mode_y, 180, 36, 8, toggle_col, 255);
                draw::draw_text(fb, panel_x + 20, mode_y + 9, if is_dark { "Mode: Dark [ON]" } else { "Mode: Light [ON]" }, 0xFFFFFFFF, 1);

                let scale_y = mode_y + 60;
                draw::draw_text(fb, panel_x, scale_y, "Interface Scale Factor (Live Apply):", theme.text_main, 1);
                let cur_scale = crate::config::CONFIG.get_ui_scale();
                for s in 1..=3 {
                    let bx = panel_x + ((s - 1) * 70); let by = scale_y + 26;
                    let (btn_bg, txt_col) = if cur_scale == s { (theme.accent, 0xFFFFFFFF) } else { (theme.titlebar_bg, theme.text_main) };
                    draw::draw_rect_rounded(fb, bx, by, 60, 32, 6, btn_bg, 255);
                    draw::draw_text(fb, bx + 18, by + 7, &alloc::format!("{}x", s), txt_col, 1);
                }
            }
            _ => { draw::draw_text(fb, panel_x, panel_y, "Other settings panels...", theme.text_dim, 1); }
        }
        self.needs_redraw = false;
    }

    fn on_event(&mut self, event: &InputEvent, mx: isize, my: isize) {
        if let InputEvent::MouseButton { button: 0, pressed: true } = event {
            let sidebar_w = 180isize;
            if mx < sidebar_w {
                let tab_idx = (my.saturating_sub(20)) / 36;
                match tab_idx {
                    0 => self.current_tab = SettingsTab::About,
                    1 => self.current_tab = SettingsTab::Appearance,
                    2 => self.current_tab = SettingsTab::Dock,
                    3 => self.current_tab = SettingsTab::Display,
                    4 => self.current_tab = SettingsTab::Network,
                    5 => self.current_tab = SettingsTab::Kernel,
                    6 => self.current_tab = SettingsTab::Processes,
                    _ => {}
                }
                self.needs_redraw = true;
            } else if self.current_tab == SettingsTab::Appearance {
                if mx >= (sidebar_w + 24) && mx <= (sidebar_w + 204) && my >= 94 && my <= 130 {
                    crate::config::CONFIG.toggle_dark_mode();
                    crate::gui::FULL_REDRAW_REQUIRED.store(true, Ordering::SeqCst);
                    self.needs_redraw = true;
                }
                let scale_y = 154isize;
                if my >= scale_y && my <= scale_y + 32 {
                    for s in 1..=3 {
                        let bx = (sidebar_w + 24) + ((s as isize - 1) * 70);
                        if mx >= bx && mx <= bx + 60 {
                            crate::config::CONFIG.set_ui_scale(s);
                            crate::gui::FULL_REDRAW_REQUIRED.store(true, Ordering::SeqCst);
                            self.needs_redraw = true;
                        }
                    }
                }
            }
        }
    }

    fn title(&self) -> &str { "System Settings" }
    fn icon(&self) -> &'static str { "SET" }
}
