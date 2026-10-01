use crate::input::InputEvent;
use super::App;
use crate::gui::draw::{self, FrameBuffer};
use crate::gui::theme::Theme;
use core::sync::atomic::Ordering;

pub struct ActivityMonitorApp {
    last_update_tick: u64,
    cpu_history: [usize; 32],
    history_idx: usize,
}

impl ActivityMonitorApp {
    pub fn new() -> Self {
        Self {
            last_update_tick: 0,
            cpu_history: [15; 32],
            history_idx: 0,
        }
    }
}

impl App for ActivityMonitorApp {
    fn wants_redraw(&self) -> bool { true }

    fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        let now = crate::arch::x86_64::pit::get_ticks();
        if now.saturating_sub(self.last_update_tick) >= 500 {
            let active_load = if crate::task::ELF_ACTIVE_RUNNING.load(Ordering::Relaxed) { 68 } else { 12 };
            self.cpu_history[self.history_idx] = active_load;
            self.history_idx = (self.history_idx + 1) % 32;
            self.last_update_tick = now;
        }

        draw::draw_rect(fb, x, y, w, h, theme.window_bg);

        // Header Tab
        draw::draw_rect(fb, x, y, w, 38, theme.titlebar_bg);
        draw::draw_line_h(fb, x, y + 38, w, theme.separator);
        draw::draw_text(fb, x + 16, y + 10, "⚡ CPU & Threads", theme.accent, 1);
        draw::draw_text(fb, x + 180, y + 10, "💾 Memory", theme.text_dim, 1);
        draw::draw_text(fb, x + 300, y + 10, "📦 Disks & VFS", theme.text_dim, 1);

        // Realtime Multi-Core Load Matrix
        let graph_x = x + 16;
        let graph_y = y + 54;
        let graph_w = w.saturating_sub(32);
        let graph_h = 90usize;

        draw::draw_rect_rounded(fb, graph_x, graph_y, graph_w, graph_h, 8, theme.titlebar_bg, 255);
        draw::draw_rect_outline(fb, graph_x, graph_y, graph_w, graph_h, theme.separator, 8);
        draw::draw_text(fb, graph_x + 12, graph_y + 8, "SMP Hardware Load History (8 Cores)", theme.text_dim, 1);

        // Render bar chart
        let bar_w = (graph_w - 30) / 32;
        for i in 0..32 {
            let val = self.cpu_history[(self.history_idx + i) % 32];
            let bar_h = (val * 50) / 100;
            let bx = graph_x + 14 + (i * bar_w);
            let by = graph_y + graph_h - 10 - bar_h;
            let col = if val > 50 { 0xFFEF4444 } else { 0xFF22C55E };
            draw::draw_rect(fb, bx, by, bar_w.saturating_sub(2), bar_h, col);
        }

        // Process Table
        let table_y = graph_y + graph_h + 16;
        draw::draw_rect(fb, x, table_y, w, 26, theme.titlebar_bg);
        draw::draw_line_h(fb, x, table_y + 26, w, theme.separator);
        draw::draw_text(fb, x + 16, table_y + 5, "PID", theme.text_dim, 1);
        draw::draw_text(fb, x + 70, table_y + 5, "Process Name", theme.text_dim, 1);
        draw::draw_text(fb, x + 240, table_y + 5, "Core", theme.text_dim, 1);
        draw::draw_text(fb, x + 320, table_y + 5, "State", theme.text_dim, 1);
        draw::draw_text(fb, x + 420, table_y + 5, "Memory", theme.text_dim, 1);

        let processes = [
            ("0", "Kernel Idle & Compositor", "Core 0", "Running", "18 MB"),
            ("1", "Async Worker Daemon", "Core 1", "Sleeping", "4 MB"),
            ("2", "Userspace Ring 3 Process", "Core 2", if crate::task::ELF_ACTIVE_RUNNING.load(Ordering::Relaxed) { "Active" } else { "Idle" }, "8 MB"),
            ("3", "Hardware Interrupt Router", "Core 0", "Ready", "2 MB"),
        ];

        for (idx, (pid, name, core, state, mem)) in processes.iter().enumerate() {
            let row_y = table_y + 28 + (idx * 30);
            if row_y + 28 > y + h { break; }
            let bg = if idx % 2 == 0 { theme.window_bg } else { theme.titlebar_bg };
            draw::draw_rect(fb, x, row_y, w, 30, bg);
            draw::draw_text(fb, x + 16, row_y + 6, pid, theme.text_main, 1);
            draw::draw_text(fb, x + 70, row_y + 6, name, theme.text_main, 1);
            draw::draw_text(fb, x + 240, row_y + 6, core, theme.text_dim, 1);
            let state_col = if *state == "Active" || *state == "Running" { 0xFF16A34A } else { theme.text_dim };
            draw::draw_text(fb, x + 320, row_y + 6, state, state_col, 1);
            draw::draw_text(fb, x + 420, row_y + 6, mem, theme.text_main, 1);
        }
    }

    fn on_event(&mut self, _event: &InputEvent, _mx: isize, _my: isize) {}
    fn title(&self) -> &str { "Activity Monitor" }
    fn icon(&self) -> &'static str { "ACT" }
}
