use std::time::{Duration, Instant};
use crate::framework::*;
use crate::sdk;

pub struct ActivityMonitorApp {
    bounds: Rect,
    selected_tab: usize,
    last_refresh: Instant,
    processes: Vec<sdk::sys::ProcessInfo>,
    selected_pid: Option<u32>,
    kernel_heap_kb: usize,
    total_syscalls_prev: u64,
    cpu_history: Vec<usize>,
    status_msg: String,
}

impl ActivityMonitorApp {
    pub fn new() -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            selected_tab: 0,
            last_refresh: Instant::now() - Duration::from_secs(2),
            processes: Vec::new(),
            selected_pid: None,
            kernel_heap_kb: 0,
            total_syscalls_prev: 0,
            cpu_history: vec![4; 24],
            status_msg: "Live Kernel Telemetry via Syscall 509".into(),
        };
        app.refresh_live_telemetry();
        app
    }

    fn refresh_live_telemetry(&mut self) {
        let now = Instant::now();
        if now.saturating_duration_since(self.last_refresh) < Duration::from_millis(500) {
            return;
        }
        self.last_refresh = now;
        self.processes = sdk::sys::query_processes();
        self.kernel_heap_kb = sdk::agent::query_kernel_heap_used() / 1024;

        let total_sc: u64 = self.processes.iter().map(|p| p.syscalls).sum();
        let delta_sc = total_sc.saturating_sub(self.total_syscalls_prev);
        self.total_syscalls_prev = total_sc;

        let load_pct = ((delta_sc / 25).min(95) as usize).max(2);
        if self.cpu_history.len() >= 24 {
            self.cpu_history.remove(0);
        }
        self.cpu_history.push(load_pct);
    }

    fn paint_process_table(&self, canvas: &mut Canvas, x: i32, y: i32, w: i32, max_h: i32) {
        let theme = get_theme();
        let header_h = theme.pt(26.0);
        let row_h = theme.pt(26.0);

        canvas.draw_rect(x, y, w, header_h, theme.bg_sidebar, 4);
        canvas.draw_text(x + theme.pt(10.0), y + theme.pt(5.0), "PID", theme.text_muted, theme.font_caption());
        canvas.draw_text(x + theme.pt(54.0), y + theme.pt(5.0), "Core", theme.text_muted, theme.font_caption());
        canvas.draw_text(x + theme.pt(104.0), y + theme.pt(5.0), "Process Name", theme.text_muted, theme.font_caption());
        canvas.draw_text(x + theme.pt(280.0), y + theme.pt(5.0), "Memory", theme.text_muted, theme.font_caption());
        canvas.draw_text(x + theme.pt(370.0), y + theme.pt(5.0), "Syscalls", theme.text_muted, theme.font_caption());

        let mut cy = y + header_h + 2;
        for p in &self.processes {
            if cy + row_h > y + max_h { break; }
            let is_sel = self.selected_pid == Some(p.pid);
            let bg = if is_sel { theme.accent } else { theme.bg_panel };
            canvas.draw_rect(x, cy, w, row_h, bg, 3);
            let txt = if is_sel { 0xFFFFFFFF } else { theme.text_primary };
            canvas.draw_text(x + theme.pt(10.0), cy + theme.pt(5.0), &p.pid.to_string(), txt, theme.font_caption());
            canvas.draw_text(x + theme.pt(54.0), cy + theme.pt(5.0), &format!("C{}", p.core_id), txt, theme.font_caption());
            canvas.draw_text_clipped(x + theme.pt(104.0), cy + theme.pt(5.0), theme.pt(170.0), &p.name, txt, theme.font_caption());
            let mem_str = if p.mem_kb >= 1024 {
                format!("{:.1} MB", p.mem_kb as f32 / 1024.0)
            } else {
                format!("{} KB", p.mem_kb)
            };
            canvas.draw_text(x + theme.pt(280.0), cy + theme.pt(5.0), &mem_str, txt, theme.font_caption());
            canvas.draw_text(x + theme.pt(370.0), cy + theme.pt(5.0), &p.syscalls.to_string(), txt, theme.font_caption());
            cy += row_h + 2;
        }
    }
}

impl Widget for ActivityMonitorApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        unsafe {
            let app_mut = self as *const Self as *mut Self;
            (*app_mut).refresh_live_telemetry();
        }

        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_app, 0);

        let tb_h = theme.pt(42.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_sidebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);

        let tabs = ["Processes & CPU", "Memory", "SMP Cores"];
        let seg_w = theme.pt(116.0);
        let pad = theme.pt(12.0);
        for (i, &t) in tabs.iter().enumerate() {
            let bx = self.bounds.x + pad + (i as i32 * (seg_w + theme.pt(6.0)));
            let by = self.bounds.y + theme.pt(7.0);
            let bg = if self.selected_tab == i { theme.accent } else { theme.bg_panel };
            canvas.draw_rect(bx, by, seg_w, theme.pt(28.0), bg, 5);
            canvas.draw_text(bx + theme.pt(10.0), by + theme.pt(6.0), t, 0xFFFFFFFF, theme.font_caption());
        }

        // Terminate Process button on top-right
        let term_w = theme.pt(124.0);
        let term_x = self.bounds.x + self.bounds.w - pad - term_w;
        let can_kill = self.selected_pid.map(|p| p >= 3).unwrap_or(false);
        canvas.draw_rect(term_x, self.bounds.y + theme.pt(7.0), term_w, theme.pt(28.0), if can_kill { 0xFFDC2626 } else { 0xFF334155 }, 5);
        canvas.draw_text(term_x + theme.pt(10.0), self.bounds.y + theme.pt(13.0), "Terminate Proc", 0xFFFFFFFF, theme.font_caption());

        let cy = self.bounds.y + tb_h + pad;
        let cw = self.bounds.w - pad * 2;
        let footer_h = theme.pt(24.0);
        let avail_h = (self.bounds.h - tb_h - footer_h - pad * 2).max(100);

        match self.selected_tab {
            0 => {
                let card_h = theme.pt(94.0);
                canvas.draw_rect(self.bounds.x + pad, cy, cw, card_h, theme.bg_panel, 8);
                canvas.draw_rect_outline(self.bounds.x + pad, cy, cw, card_h, theme.border_window, 8);
                let cur_load = self.cpu_history.last().copied().unwrap_or(2);
                canvas.draw_text(self.bounds.x + pad + theme.pt(12.0), cy + theme.pt(10.0), &format!("Live Activity: {}%  |  Active Processes: {}  |  Kernel Heap: {:.1} MB", cur_load, self.processes.len(), self.kernel_heap_kb as f32 / 1024.0), theme.text_primary, theme.font_body());

                let graph_y = cy + theme.pt(40.0);
                let graph_h = theme.pt(42.0);
                let bar_w = ((cw - theme.pt(24.0)) / self.cpu_history.len().max(1) as i32).max(4);
                for (i, &v) in self.cpu_history.iter().enumerate() {
                    let gx = self.bounds.x + pad + theme.pt(12.0) + (i as i32 * bar_w);
                    let bh = ((v as i32 * graph_h) / 100).max(2);
                    canvas.draw_rect(gx, graph_y + graph_h - bh, (bar_w - 3).max(2), bh, theme.accent_hover, 2);
                }

                let tbl_y = cy + card_h + theme.pt(10.0);
                self.paint_process_table(canvas, self.bounds.x + pad, tbl_y, cw, avail_h - card_h - theme.pt(10.0));
            }
            1 => {
                let card_h = theme.pt(110.0);
                canvas.draw_rect(self.bounds.x + pad, cy, cw, card_h, theme.bg_panel, 8);
                canvas.draw_rect_outline(self.bounds.x + pad, cy, cw, card_h, theme.border_window, 8);
                let total_proc_mb: f32 = self.processes.iter().map(|p| p.mem_kb as f32 / 1024.0).sum();
                canvas.draw_text(self.bounds.x + pad + theme.pt(14.0), cy + theme.pt(12.0), &format!("Kernel Heap: {:.1} MB / 256.0 MB", self.kernel_heap_kb as f32 / 1024.0), theme.text_primary, theme.font_title());
                canvas.draw_text(self.bounds.x + pad + theme.pt(14.0), cy + theme.pt(40.0), &format!("Accounted Process Memory: {:.1} MB across {} tasks", total_proc_mb, self.processes.len()), theme.text_secondary, theme.font_caption());
                let bar_w = cw - theme.pt(28.0);
                let fill = (((self.kernel_heap_kb as f32) / (256.0 * 1024.0)) * bar_w as f32).clamp(16.0, bar_w as f32) as i32;
                canvas.draw_rect(self.bounds.x + pad + theme.pt(14.0), cy + theme.pt(70.0), bar_w, theme.pt(16.0), 0xFF0F172A, 4);
                canvas.draw_rect(self.bounds.x + pad + theme.pt(14.0), cy + theme.pt(70.0), fill, theme.pt(16.0), theme.accent, 4);

                let tbl_y = cy + card_h + theme.pt(10.0);
                self.paint_process_table(canvas, self.bounds.x + pad, tbl_y, cw, avail_h - card_h - theme.pt(10.0));
            }
            _ => {
                canvas.draw_rect(self.bounds.x + pad, cy, cw, avail_h, theme.bg_panel, 8);
                canvas.draw_rect_outline(self.bounds.x + pad, cy, cw, avail_h, theme.border_window, 8);
                canvas.draw_text(self.bounds.x + pad + theme.pt(14.0), cy + theme.pt(12.0), "8-Core SMP Hardware Utilization (Real Task Assignment)", theme.text_primary, theme.font_title());
                for core_id in 0..8u32 {
                    let ry = cy + theme.pt(44.0) + (core_id as i32 * theme.pt(28.0));
                    let owner = self.processes.iter().find(|p| p.core_id == core_id).map(|p| p.name.as_str()).unwrap_or("Idle");
                    let active = owner != "Idle";
                    canvas.draw_text(self.bounds.x + pad + theme.pt(14.0), ry + theme.pt(4.0), &format!("Core {} :", core_id), theme.text_secondary, theme.font_caption());
                    let bx = self.bounds.x + pad + theme.pt(84.0);
                    let bw = cw - theme.pt(104.0);
                    canvas.draw_rect(bx, ry, bw, theme.pt(20.0), 0xFF0F172A, 4);
                    canvas.draw_rect(bx, ry, if active { bw / 2 } else { theme.pt(12.0) }, theme.pt(20.0), if active { theme.accent } else { 0xFF334155 }, 4);
                    canvas.draw_text(bx + theme.pt(8.0), ry + theme.pt(3.0), owner, 0xFFFFFFFF, theme.font_caption());
                }
            }
        }

        let fy = self.bounds.y + self.bounds.h - footer_h;
        canvas.draw_rect(self.bounds.x, fy, self.bounds.w, footer_h, theme.bg_sidebar, 0);
        canvas.draw_text(self.bounds.x + pad, fy + theme.pt(4.0), &self.status_msg, theme.text_secondary, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) || !pressed { return false; }
        let theme = get_theme();
        let tb_h = theme.pt(42.0);
        let pad = theme.pt(12.0);

        if my <= self.bounds.y + tb_h {
            let seg_w = theme.pt(116.0);
            for i in 0..3 {
                let bx = self.bounds.x + pad + (i as i32 * (seg_w + theme.pt(6.0)));
                if mx >= bx && mx <= bx + seg_w {
                    self.selected_tab = i;
                    return true;
                }
            }
            let term_w = theme.pt(124.0);
            let term_x = self.bounds.x + self.bounds.w - pad - term_w;
            if mx >= term_x && mx <= term_x + term_w {
                if let Some(pid) = self.selected_pid {
                    if crate::syscall::sys_kill_proc(pid) {
                        self.status_msg = format!("Terminated PID {}", pid);
                        self.selected_pid = None;
                        self.last_refresh = Instant::now() - Duration::from_secs(2);
                        self.refresh_live_telemetry();
                    } else {
                        self.status_msg = format!("Cannot terminate protected PID {}", pid);
                    }
                }
                return true;
            }
            return true;
        }

        // Select row in process table
        if self.selected_tab == 0 || self.selected_tab == 1 {
            let card_h = if self.selected_tab == 0 { theme.pt(94.0) } else { theme.pt(110.0) };
            let tbl_y = self.bounds.y + tb_h + pad + card_h + theme.pt(10.0) + theme.pt(28.0);
            let row_h = theme.pt(28.0);
            if my >= tbl_y {
                let idx = ((my - tbl_y) / row_h) as usize;
                if let Some(p) = self.processes.get(idx) {
                    self.selected_pid = Some(p.pid);
                    self.status_msg = format!("Selected PID {} ({}) on Core {}", p.pid, p.name, p.core_id);
                }
            }
        }
        true
    }
}
