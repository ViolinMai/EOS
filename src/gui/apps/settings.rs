use crate::input::InputEvent;
use super::App;
use crate::gui::draw::{self, FrameBuffer};
use crate::gui::theme::Theme;
use core::sync::atomic::Ordering;

#[derive(PartialEq, Eq, Clone, Copy)]
enum SettingsTab {
    About,
    Appearance,
    Dock,
    Display,
    Network,
    Kernel,
    Processes,
}

pub struct SettingsApp {
    current_tab: SettingsTab,
    needs_redraw: bool,
}

impl SettingsApp {
    pub fn new() -> Self {
        Self {
            current_tab: SettingsTab::About,
            needs_redraw: true,
        }
    }
}

impl App for SettingsApp {
    fn wants_redraw(&self) -> bool { self.needs_redraw }

    fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        let sidebar_w = 180usize.min(w / 3);
        
        draw::draw_rect(fb, x, y, sidebar_w, h, theme.titlebar_bg);
        draw::draw_line_v(fb, x + sidebar_w, y, h, theme.separator);

        let tabs = [
            (SettingsTab::About, "ℹ️ About"),
            (SettingsTab::Appearance, "🎨 Appearance"),
            (SettingsTab::Dock, "🗂 Dock & Bar"),
            (SettingsTab::Display, "🖥 Display"),
            (SettingsTab::Network, "🌐 Network"),
            (SettingsTab::Kernel, "⚙️ Kernel"),
            (SettingsTab::Processes, "⚡ Activity"),
        ];

        for (i, (tab, label)) in tabs.iter().enumerate() {
            let row_y = y + 20 + (i * 36);
            if row_y + 32 > y + h { break; }
            let is_sel = self.current_tab == *tab;
            if is_sel {
                draw::draw_rect_rounded(fb, x + 8, row_y, sidebar_w - 16, 30, 6, theme.accent, 255);
                draw::draw_text(fb, x + 16, row_y + 7, label, 0xFFFFFFFF, 1);
            } else {
                draw::draw_text(fb, x + 16, row_y + 7, label, theme.text_main, 1);
            }
        }

        let panel_x = x + sidebar_w + 16;
        let panel_w = w.saturating_sub(sidebar_w + 32);
        let panel_y = y + 20;

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
                draw::draw_text(fb, panel_x + 20, card_y + 102, &alloc::format!("System Uptime: {} seconds since Limine boot", crate::arch::x86_64::pit::get_uptime_seconds()), theme.text_dim, 1);
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
                    let bx = panel_x + ((s - 1) * 70);
                    let by = scale_y + 26;
                    let btn_bg = if cur_scale == s { theme.accent } else { theme.titlebar_bg };
                    let txt_col = if cur_scale == s { 0xFFFFFFFF } else { theme.text_main };
                    draw::draw_rect_rounded(fb, bx, by, 60, 32, 6, btn_bg, 255);
                    draw::draw_text(fb, bx + 18, by + 7, &alloc::format!("{}x", s), txt_col, 1);
                }
            }
            SettingsTab::Dock => {
                draw::draw_text(fb, panel_x, panel_y, "Dock Configuration", theme.text_main, 2);
                let card_y = panel_y + 60;
                draw::draw_rect_rounded(fb, panel_x, card_y, panel_w, 90, 8, theme.titlebar_bg, 255);
                draw::draw_text(fb, panel_x + 20, card_y + 20, "• Continuous Spring Interpolation: Enabled", theme.text_main, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 50, "• Collision-Free Neighbor Spread : Enabled", theme.text_main, 1);
            }
            SettingsTab::Display => {
                draw::draw_text(fb, panel_x, panel_y, "Display Architecture", theme.text_main, 2);
                let card_y = panel_y + 60;
                draw::draw_rect_rounded(fb, panel_x, card_y, panel_w, 110, 8, theme.titlebar_bg, 255);
                draw::draw_text(fb, panel_x + 20, card_y + 20, "Resolution: 1920x1080 TrueColor ARGB", theme.text_main, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 48, "Refresh Target: 60 FPS (Hardware TSC Synchronized)", theme.text_main, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 76, "Engine: Non-Blocking Dirty Region Compositor", theme.text_dim, 1);
            }
            SettingsTab::Network => {
                draw::draw_text(fb, panel_x, panel_y, "Network Adapters", theme.text_main, 2);
                let card_y = panel_y + 60;
                draw::draw_rect_rounded(fb, panel_x, card_y, panel_w, 100, 8, theme.titlebar_bg, 255);
                draw::draw_text(fb, panel_x + 20, card_y + 20, "Interface: Intel E1000 Gigabit NIC", theme.text_main, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 48, "Link State: Active | IPv4: 10.0.2.15", 0xFF16A34A, 1);
            }
            SettingsTab::Kernel => {
                draw::draw_text(fb, panel_x, panel_y, "Kernel Subsystems", theme.text_main, 2);
                let card_y = panel_y + 60;
                draw::draw_rect_rounded(fb, panel_x, card_y, panel_w, 130, 8, theme.titlebar_bg, 255);
                draw::draw_text(fb, panel_x + 20, card_y + 18, "• Preemptive Scheduler: 1000 Hz PIT Interrupt", theme.text_main, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 46, "• FPU Context Restore: 512-byte FXSAVE on switch", theme.text_main, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 74, "• Memory Boundary    : Hardware CR3 Isolation per Task", theme.text_main, 1);
            }
            SettingsTab::Processes => {
                draw::draw_text(fb, panel_x, panel_y, "System Processes (Press Ctrl+Shift+Esc for Manager)", theme.text_main, 1);
                let card_y = panel_y + 40;
                draw::draw_rect_rounded(fb, panel_x, card_y, panel_w, 140, 8, theme.titlebar_bg, 255);
                draw::draw_text(fb, panel_x + 20, card_y + 18, "PID 0: Compositor Engine & Kernel (Core 0) - Active", theme.text_main, 1);
                draw::draw_text(fb, panel_x + 20, card_y + 46, "PID 1: Async Worker Daemon (Core 1)        - Idle", theme.text_main, 1);
                let elf_active = crate::task::ELF_ACTIVE_RUNNING.load(Ordering::Relaxed);
                draw::draw_text(fb, panel_x + 20, card_y + 74, if elf_active { "PID 2: Isolated Ring 3 User Binary - Running" } else { "PID 2: Ring 3 User Space - Inactive" }, if elf_active { 0xFF0284C7 } else { theme.text_dim }, 1);
            }
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
                // Toggle Dark Mode button
                if mx >= (sidebar_w + 16) && mx <= (sidebar_w + 196) && my >= 90 && my <= 126 {
                    crate::config::CONFIG.toggle_dark_mode();
                    self.needs_redraw = true;
                }
                // Scale buttons
                let scale_y = 154isize;
                if my >= scale_y && my <= scale_y + 32 {
                    for s in 1..=3 {
                        let bx = (sidebar_w + 16) + ((s as isize - 1) * 70);
                        if mx >= bx && mx <= bx + 60 {
                            crate::config::CONFIG.set_ui_scale(s);
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
