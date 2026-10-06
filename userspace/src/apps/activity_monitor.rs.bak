use crate::framework::*;

#[derive(Clone, Debug)]
pub struct ProcessItem {
    pub pid: usize,
    pub name: String,
    pub cpu_percent: usize,
    pub mem_mb: usize,
    pub status: &'static str,
}

pub struct ActivityMonitorApp {
    bounds: Rect,
    selected_tab: usize,
    last_update_tick: u64,
    processes: Vec<ProcessItem>,
    cpu_history: Vec<usize>,
    mem_used_mb: usize,
    disk_read_kb: usize,
    disk_write_kb: usize,
    net_rx_packets: usize,
    net_tx_packets: usize,
}

impl ActivityMonitorApp {
    pub fn new() -> Self {
        Self {
            bounds: Rect::default(),
            selected_tab: 0,
            last_update_tick: 0,
            processes: vec![
                ProcessItem { pid: 1, name: "kernel_core".into(), cpu_percent: 2, mem_mb: 32, status: "System" },
                ProcessItem { pid: 2, name: "desktop_ui".into(), cpu_percent: 8, mem_mb: 48, status: "Running" },
                ProcessItem { pid: 3, name: "vfs_manager".into(), cpu_percent: 1, mem_mb: 16, status: "Idle" },
                ProcessItem { pid: 4, name: "net_daemon".into(), cpu_percent: 1, mem_mb: 12, status: "Running" },
            ],
            cpu_history: vec![12, 14, 18, 15, 22, 19, 14, 16, 20, 15, 17, 13],
            mem_used_mb: 142,
            disk_read_kb: 4820,
            disk_write_kb: 1240,
            net_rx_packets: 128,
            net_tx_packets: 94,
        }
    }

    fn update_stats(&mut self) {
        let t = crate::framework::perf_log::SmartLogger::read_tsc();
        if t.saturating_sub(self.last_update_tick) < 2_500_000_000 { return; }
        self.last_update_tick = t;

        let step = (t % 7) as usize;
        let new_load = 10 + (step * 3);
        if self.cpu_history.len() >= 20 { self.cpu_history.remove(0); }
        self.cpu_history.push(new_load);

        self.disk_read_kb += 64 + step * 10;
        self.net_rx_packets += 2 + step;
        self.net_tx_packets += 1 + (step / 2);
    }

    pub fn paint_process_list(&self, canvas: &mut Canvas, x: i32, y: i32, w: i32) {
        let theme = get_theme();
        let row_h = theme.pt(28.0);
        let header_h = theme.pt(26.0);

        canvas.draw_rect(x, y, w, header_h, 0xFF1E293B, 4);
        canvas.draw_text(x + theme.pt(12.0), y + theme.pt(5.0), "PID", theme.text_muted, theme.font_caption());
        canvas.draw_text(x + theme.pt(60.0), y + theme.pt(5.0), "Process Name", theme.text_muted, theme.font_caption());
        canvas.draw_text(x + theme.pt(240.0), y + theme.pt(5.0), "CPU %", theme.text_muted, theme.font_caption());
        canvas.draw_text(x + theme.pt(320.0), y + theme.pt(5.0), "Memory", theme.text_muted, theme.font_caption());
        canvas.draw_text(x + theme.pt(400.0), y + theme.pt(5.0), "Status", theme.text_muted, theme.font_caption());

        let mut cy = y + header_h + 2;
        for p in &self.processes {
            canvas.draw_rect(x, cy, w, row_h, 0xFF141E2E, 2);
            canvas.draw_text(x + theme.pt(12.0), cy + theme.pt(6.0), &p.pid.to_string(), theme.text_primary, theme.font_caption());
            canvas.draw_text(x + theme.pt(60.0), cy + theme.pt(6.0), &p.name, 0xFFFFFFFF, theme.font_body());
            canvas.draw_text(x + theme.pt(240.0), cy + theme.pt(6.0), &format!("{}%", p.cpu_percent), theme.accent_hover, theme.font_caption());
            canvas.draw_text(x + theme.pt(320.0), cy + theme.pt(6.0), &format!("{} MB", p.mem_mb), theme.text_secondary, theme.font_caption());
            canvas.draw_text(x + theme.pt(400.0), cy + theme.pt(6.0), p.status, 0xFF10B981, theme.font_caption());
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
            (*app_mut).update_stats();
        }

        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, theme.bg_window, 0);

        let tb_h = theme.pt(44.0);
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, tb_h, theme.bg_titlebar, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + tb_h, self.bounds.w, theme.border_window);

        let tabs = ["CPU", "Memory", "Disk", "Network"];
        let seg_w = theme.pt(90.0);
        let start_x = self.bounds.x + (self.bounds.w - (tabs.len() as i32 * seg_w)) / 2;

        for (i, &t) in tabs.iter().enumerate() {
            let bx = start_x + (i as i32 * seg_w);
            let by = self.bounds.y + theme.pt(8.0);
            if self.selected_tab == i {
                canvas.draw_rect(bx, by, seg_w, theme.pt(28.0), theme.accent, theme.pt(6.0) as usize);
                let (tw, _) = canvas.measure_text(t, theme.font_body());
                canvas.draw_text(bx + (seg_w - tw as i32) / 2, by + theme.pt(5.0), t, 0xFFFFFFFF, theme.font_body());
            } else {
                canvas.draw_rect_outline(bx, by, seg_w, theme.pt(28.0), theme.border_window, theme.pt(6.0) as usize);
                let (tw, _) = canvas.measure_text(t, theme.font_body());
                canvas.draw_text(bx + (seg_w - tw as i32) / 2, by + theme.pt(5.0), t, theme.text_secondary, theme.font_body());
            }
        }

        let content_y = self.bounds.y + tb_h + theme.pt(16.0);
        let pad_x = self.bounds.x + theme.pt(24.0);
        let card_w = self.bounds.w - theme.pt(48.0);

        match self.selected_tab {
            0 => {
                let card_h = theme.pt(120.0);
                canvas.draw_rect(pad_x, content_y, card_w, card_h, theme.bg_titlebar, theme.pt(8.0) as usize);
                canvas.draw_rect_outline(pad_x, content_y, card_w, card_h, theme.border_window, theme.pt(8.0) as usize);

                let cur_cpu = self.cpu_history.last().copied().unwrap_or(15);
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(14.0), &format!("Processor Load: {}% (8 Active SMP Cores)", cur_cpu), theme.text_primary, theme.font_title());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(40.0), "Real-time thread dispatcher: Symmetrical Cores Online", theme.text_secondary, theme.font_caption());

                let graph_y = content_y + theme.pt(65.0);
                let bar_w = (card_w - theme.pt(40.0)) / self.cpu_history.len().max(1) as i32;
                for (idx, &load) in self.cpu_history.iter().enumerate() {
                    let gx = pad_x + theme.pt(16.0) + (idx as i32 * bar_w);
                    let bar_h = (load as i32 * theme.pt(35.0)) / 100;
                    canvas.draw_rect(gx, graph_y + theme.pt(35.0) - bar_h, bar_w - 3, bar_h, theme.accent, 2);
                }

                let list_y = content_y + card_h + theme.pt(16.0);
                self.paint_process_list(canvas, pad_x, list_y, card_w);
            }
            1 => {
                let card_h = theme.pt(100.0);
                canvas.draw_rect(pad_x, content_y, card_w, card_h, theme.bg_titlebar, theme.pt(8.0) as usize);
                canvas.draw_rect_outline(pad_x, content_y, card_w, card_h, theme.border_window, theme.pt(8.0) as usize);

                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(14.0), &format!("Physical Memory: {} MB / 8192 MB", self.mem_used_mb), theme.text_primary, theme.font_title());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(38.0), "Kernel Heap: 256 MB Dedicated | Paging: 4-Level x86_64 PML4", theme.text_secondary, theme.font_caption());

                let bar_total_w = card_w - theme.pt(32.0);
                let fill_w = (self.mem_used_mb as i32 * bar_total_w) / 8192;
                canvas.draw_rect(pad_x + theme.pt(16.0), content_y + theme.pt(65.0), bar_total_w, theme.pt(14.0), 0xFF1E293B, 4);
                canvas.draw_rect(pad_x + theme.pt(16.0), content_y + theme.pt(65.0), fill_w.max(20), theme.pt(14.0), theme.accent, 4);

                let list_y = content_y + card_h + theme.pt(16.0);
                self.paint_process_list(canvas, pad_x, list_y, card_w);
            }
            2 => {
                let card_h = theme.pt(140.0);
                canvas.draw_rect(pad_x, content_y, card_w, card_h, theme.bg_titlebar, theme.pt(8.0) as usize);
                canvas.draw_rect_outline(pad_x, content_y, card_w, card_h, theme.border_window, theme.pt(8.0) as usize);

                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(14.0), "Storage Subsystem & VFS Disks", theme.text_primary, theme.font_title());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(42.0), &format!("Total Read: {} KB | Total Written: {} KB", self.disk_read_kb, self.disk_write_kb), theme.accent_hover, theme.font_body());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(72.0), "Drive 0: UEFI FAT32 Boot Partition (128 MB)", theme.text_secondary, theme.font_caption());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(92.0), "Drive 1: Shared FAT32 Host Bridge (EOS SHARE)", theme.text_secondary, theme.font_caption());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(112.0), "Drive 2: EXT2 Native Persistent RootFS (Active)", 0xFF10B981, theme.font_caption());
            }
            3 => {
                let card_h = theme.pt(140.0);
                canvas.draw_rect(pad_x, content_y, card_w, card_h, theme.bg_titlebar, theme.pt(8.0) as usize);
                canvas.draw_rect_outline(pad_x, content_y, card_w, card_h, theme.border_window, theme.pt(8.0) as usize);

                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(14.0), "Network Interface: eth0 (Intel E1000 PCI)", theme.text_primary, theme.font_title());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(42.0), &format!("Traffic: RX {} packets | TX {} packets", self.net_rx_packets, self.net_tx_packets), theme.accent_hover, theme.font_body());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(72.0), "IPv4 Address: 10.0.2.15 | Subnet: 255.255.255.0", theme.text_secondary, theme.font_caption());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(92.0), "Default Gateway: 10.0.2.2 | DNS Server: 10.0.2.3", theme.text_secondary, theme.font_caption());
                canvas.draw_text(pad_x + theme.pt(16.0), content_y + theme.pt(112.0), "Status: Online (Zero-copy raw ring buffer)", 0xFF10B981, theme.font_caption());
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let tb_h = theme.pt(44.0);

        if pressed && my <= self.bounds.y + tb_h {
            let seg_w = theme.pt(90.0);
            let start_x = self.bounds.x + (self.bounds.w - (4 * seg_w)) / 2;
            if mx >= start_x && mx <= start_x + (4 * seg_w) {
                self.selected_tab = ((mx - start_x) / seg_w) as usize;
                return true;
            }
        }
        true
    }
}
