use crate::input::InputEvent;
use super::App;
use crate::gui::draw::{self, FrameBuffer};
use crate::gui::theme::Theme;
use core::sync::atomic::Ordering;
use alloc::format;

pub struct ActivityMonitorApp {
    last_update_tick: u64,
    cpu_history: [usize; 32],
    history_idx: usize,
    needs_redraw: bool,
}

impl ActivityMonitorApp {
    pub fn new() -> Self {
        Self { last_update_tick: 0, cpu_history: [15; 32], history_idx: 0, needs_redraw: true }
    }
}

impl App for ActivityMonitorApp {
    fn wants_redraw(&self) -> bool { self.needs_redraw }

    fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        let now = crate::arch::x86_64::pit::get_ticks();
        if now.saturating_sub(self.last_update_tick) >= 500 {
            let active_load = if crate::task::ELF_ACTIVE_RUNNING.load(Ordering::Relaxed) { 68 } else { 12 };
            self.cpu_history[self.history_idx] = active_load;
            self.history_idx = (self.history_idx + 1) % 32;
            self.last_update_tick = now;
            self.needs_redraw = true;
        }

        draw::draw_rect(fb, x, y, w, h, theme.window_bg);
        draw::draw_rect(fb, x, y, w, 38, theme.titlebar_bg);
        draw::draw_line_h(fb, x, y + 38, w, theme.separator);
        draw::draw_text(fb, x + 16, y + 10, "⚡ CPU & Threads", theme.accent, 1);
        draw::draw_text(fb, x + 180, y + 10, "💾 Memory", theme.text_dim, 1);

        let graph_x = x + 16; let graph_y = y + 54; let graph_w = w.saturating_sub(32); let graph_h = 100usize;
        draw::draw_rect_rounded(fb, graph_x, graph_y, graph_w, graph_h, 8, theme.titlebar_bg, 255);
        draw::draw_rect_outline(fb, graph_x, graph_y, graph_w, graph_h, theme.separator, 8);
        draw::draw_text(fb, graph_x + 12, graph_y + 8, "SMP Hardware Load History (8 Cores)", theme.text_dim, 1);

        let bar_w = (graph_w - 30) / 32;
        for i in 0..32 {
            let val = self.cpu_history[(self.history_idx + i) % 32];
            let bar_h = (val * 60) / 100;
            let bx = graph_x + 14 + (i * bar_w);
            let by = graph_y + graph_h - 10 - bar_h;
            let col = if val > 50 { 0xFFEF4444 } else { 0xFF22C55E };
            draw::draw_rect(fb, bx, by, bar_w.saturating_sub(2), bar_h, col);
        }

        let table_y = graph_y + graph_h + 16;
        draw::draw_rect(fb, x, table_y, w, 30, theme.titlebar_bg);
        draw::draw_line_h(fb, x, table_y + 30, w, theme.separator);
        
        let cw1 = x + 16; let cw2 = x + 70; let cw3 = x + 300; let cw4 = x + 380; let cw5 = x + 480;
        
        draw::draw_text(fb, cw1, table_y + 8, "PID", theme.text_dim, 1);
        draw::draw_text(fb, cw2, table_y + 8, "Process Name", theme.text_dim, 1);
        draw::draw_text(fb, cw3, table_y + 8, "Core", theme.text_dim, 1);
        draw::draw_text(fb, cw4, table_y + 8, "State", theme.text_dim, 1);
        draw::draw_text(fb, cw5, table_y + 8, "Memory", theme.text_dim, 1);

        let heap_used_mb = crate::mm::heap::HEAP_ALLOCATOR.used() / (1024 * 1024);
        let processes = [
            ("0", "Kernel Idle & Compositor", "Core 0", "Running", format!("{} MB", heap_used_mb)),
            ("1", "Async Worker Daemon", "Core 1", "Sleeping", "4 MB".into()),
            ("2", "Userspace Ring 3 Process", "Core 2", if crate::task::ELF_ACTIVE_RUNNING.load(Ordering::Relaxed) { "Active".into() } else { "Idle".into() }, "12 MB".into()),
            ("3", "Hardware Interrupt Router", "Core 0", "Ready", "2 MB".into()),
        ];

        for (idx, (pid, name, core, state, mem)) in processes.iter().enumerate() {
            let row_y = table_y + 32 + (idx * 34);
            if row_y + 34 > y + h { break; }
            let bg = if idx % 2 == 0 { theme.window_bg } else { theme.titlebar_bg };
            draw::draw_rect(fb, x, row_y, w, 34, bg);
            draw::draw_text(fb, cw1, row_y + 8, pid, theme.text_main, 1);
            draw::draw_text_clipped(fb, cw2, row_y + 8, cw3 - cw2 - 10, name, theme.text_main, 1);
            draw::draw_text(fb, cw3, row_y + 8, core, theme.text_dim, 1);
            let state_col = if *state == "Active" || *state == "Running" { 0xFF16A34A } else { theme.text_dim };
            draw::draw_text(fb, cw4, row_y + 8, state, state_col, 1);
            draw::draw_text(fb, cw5, row_y + 8, mem, theme.text_main, 1);
        }
        self.needs_redraw = false;
    }
    fn on_event(&mut self, _e: &InputEvent, _mx: isize, _my: isize) {}
    fn title(&self) -> &str { "Activity Monitor" }
    fn icon(&self) -> &'static str { "ACT" }
}
