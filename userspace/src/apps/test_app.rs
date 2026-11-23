use std::fs;
use crate::framework::*;
use crate::sdk;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TestTab {
    GpuProcs,
    UiAnim,
    Color10BitFx,
    DamageStress,
    DebugBeta,
}

struct StressBox {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    color: u32,
}

pub struct TestApp {
    bounds: Rect,
    tab: TestTab,
    gpu_info: sdk::gpu::GpuInfo,
    procs: Vec<sdk::sys::ProcessInfo>,
    virgl_status: String,
    decode_status: String,
    image_widget: Option<ImageWidget>,
    scrollbar: SmoothScrollBar,
    pub anim_running: bool,
    anim_phase: f32,
    stress_boxes: Vec<StressBox>,
    stress_active: bool,
    aa_radius: usize,
    click_count: usize,
    last_proc_poll: std::time::Instant,
    last_anim_tick: std::time::Instant,
}

impl TestApp {
    pub fn new() -> Self {
        let colors = [0xFFEF4444, 0xFFF59E0B, 0xFF10B981, 0xFF3B82F6, 0xFF8B5CF6, 0xFFEC4899];
        let mut stress_boxes = Vec::with_capacity(24);
        for i in 0..24 {
            stress_boxes.push(StressBox {
                x: 20.0 + ((i * 29) % 360) as f32,
                y: 20.0 + ((i * 17) % 180) as f32,
                vx: if i % 2 == 0 { 4.5 + (i % 5) as f32 } else { -(4.0 + (i % 4) as f32) },
                vy: if i % 3 == 0 { 3.5 + (i % 4) as f32 } else { -(3.0 + (i % 3) as f32) },
                color: colors[i % colors.len()],
            });
        }

        let mut app = Self {
            bounds: Rect::default(),
            tab: TestTab::GpuProcs,
            gpu_info: sdk::gpu::query_info(),
            procs: sdk::sys::query_processes(),
            virgl_status: "Pending".into(),
            decode_status: "Not run".into(),
            image_widget: None,
            scrollbar: SmoothScrollBar::new(),
            anim_running: true,
            anim_phase: 0.0,
            stress_boxes,
            stress_active: true,
            aa_radius: 14,
            click_count: 0,
            last_proc_poll: std::time::Instant::now(),
            last_anim_tick: std::time::Instant::now(),
        };
        app.run_gpu_probe();
        app.run_media_decode_probe();
        app
    }

    pub fn tick_animation(&mut self) -> bool {
        let now = std::time::Instant::now();
        let mut changed = self.scrollbar.tick();

        match self.tab {
            TestTab::GpuProcs => {
                if now.saturating_duration_since(self.last_proc_poll) >= std::time::Duration::from_millis(500) {
                    self.procs = sdk::sys::query_processes();
                    self.last_proc_poll = now;
                    changed = true;
                }
            }
            TestTab::UiAnim | TestTab::Color10BitFx => {
                if self.anim_running && get_features().smooth_animations
                    && now.saturating_duration_since(self.last_anim_tick) >= std::time::Duration::from_millis(33)
                {
                    self.anim_phase = (self.anim_phase + 0.03) % 1.0;
                    self.last_anim_tick = now;
                    changed = true;
                }
            }
            TestTab::DamageStress => {
                if self.stress_active
                    && now.saturating_duration_since(self.last_anim_tick) >= std::time::Duration::from_millis(33)
                {
                    let max_w = (self.bounds.w - 60).max(200) as f32;
                    let max_h = (self.bounds.h - 140).max(120) as f32;
                    for b in &mut self.stress_boxes {
                        b.x += b.vx;
                        b.y += b.vy;
                        if b.x < 0.0 || b.x > max_w { b.vx = -b.vx; b.x = b.x.clamp(0.0, max_w); }
                        if b.y < 0.0 || b.y > max_h { b.vy = -b.vy; b.y = b.y.clamp(0.0, max_h); }
                    }
                    self.last_anim_tick = now;
                    changed = true;
                }
            }
            TestTab::DebugBeta => {}
        }
        changed
    }

    fn run_gpu_probe(&mut self) {
        let t0 = SmartLogger::read_tsc();
        self.gpu_info = sdk::gpu::query_info();
        let ok = sdk::gpu::submit_virgl_nop();
        let us = SmartLogger::cycles_to_us(SmartLogger::read_tsc().saturating_sub(t0));
        self.virgl_status = if ok {
            format!("PASS: VirGL 3D NOP executed on Host GPU ({} us)", us)
        } else {
            "SOFTWARE MODE: VirGL 3D disabled".into()
        };
    }

    fn run_media_decode_probe(&mut self) {
        let paths = [
            "/EOS SHARE/TEST_SUBJECT/color_bars_16x16.png",
            "EOS SHARE/TEST_SUBJECT/color_bars_16x16.png",
        ];
        for p in &paths {
            if let Ok(b) = fs::read(p) {
                if !b.is_empty() {
                    let t0 = SmartLogger::read_tsc();
                    match sdk::media::decode_image(&b) {
                        Ok((px, w, h, via)) => {
                            let us = SmartLogger::cycles_to_us(SmartLogger::read_tsc().saturating_sub(t0));
                            self.image_widget = Some(ImageWidget::new(px, w, h));
                            self.decode_status = format!("PASS: {}x{} via {} ({} us)", w, h, via, us);
                        }
                        Err(e) => self.decode_status = format!("FAIL: {}", e),
                    }
                    return;
                }
            }
        }
    }
}

impl Widget for TestApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let feats = get_features();
        let x = self.bounds.x;
        let y = self.bounds.y;
        let w = self.bounds.w;
        let h = self.bounds.h;
        let pad = theme.pt(10.0);

        canvas.draw_rect(x, y, w, h, 0xFF0F172A, 0);

        // 5-Tab Navigation Bar
        let tb_h = theme.pt(34.0);
        canvas.draw_rect(x, y, w, tb_h, 0xFF1E293B, 0);
        canvas.draw_line_h(x, y + tb_h, w, theme.border_window);

        let tabs = [
            (TestTab::GpuProcs, "1. GPU & Procs"),
            (TestTab::UiAnim, "2. UI & Anim"),
            (TestTab::Color10BitFx, "3. 10-bit & FX"),
            (TestTab::DamageStress, "4. Damage Stress"),
            (TestTab::DebugBeta, "5. Debug & Beta"),
        ];
        let tab_w = theme.pt(112.0);
        for (i, (t, label)) in tabs.iter().enumerate() {
            let tx = x + pad + (i as i32 * (tab_w + theme.pt(4.0)));
            let ty = y + theme.pt(4.0);
            let th = tb_h - theme.pt(8.0);
            let bg = if self.tab == *t { theme.accent } else { 0xFF0F172A };
            canvas.draw_rect(tx, ty, tab_w, th, bg, theme.pt(5.0) as usize);
            canvas.draw_text(tx + theme.pt(8.0), ty + theme.pt(5.0), label, 0xFFFFFFFF, theme.font_caption());
        }

        let cy = y + tb_h + pad;
        let cw = w - pad * 2;
        let ch = (y + h - cy - pad).max(theme.pt(100.0));

        match self.tab {
            TestTab::GpuProcs => {
                // Real Kernel Telemetry + Process Table (Syscall 509)
                let avg_us = SmartLogger::average_frame_us();
                canvas.draw_rect(x + pad, cy, cw, theme.pt(76.0), 0xFF1E293B, theme.pt(8.0) as usize);
                canvas.draw_text(x + pad + theme.pt(10.0), cy + theme.pt(8.0), &format!("Backend: {} ({}x{}) | Avg Frame: {} us", self.gpu_info.backend_name(), self.gpu_info.width, self.gpu_info.height, avg_us), 0xFF38BDF8, theme.font_body());
                canvas.draw_text(x + pad + theme.pt(10.0), cy + theme.pt(32.0), &self.virgl_status, 0xFF34D399, theme.font_caption());
                canvas.draw_text(x + pad + theme.pt(10.0), cy + theme.pt(52.0), "Real Kernel Process Table (Syscall 509 - Live Mathematical Accounting):", 0xFFF8FAFC, theme.font_caption());

                let table_y = cy + theme.pt(84.0);
                let row_h = theme.pt(22.0);
                canvas.draw_rect(x + pad, table_y, cw, row_h, 0xFF334155, 4);
                canvas.draw_text(x + pad + theme.pt(8.0), table_y + theme.pt(3.0), "PID   Core   Kernel Process       Real Mem (KB)    Syscalls", 0xFFF8FAFC, theme.font_caption());

                let mut cur_y = table_y + row_h + 2;
                for p in &self.procs {
                    if cur_y + row_h > y + h - pad { break; }
                    canvas.draw_rect(x + pad, cur_y, cw, row_h, 0xFF141E2E, 2);
                    let line = format!("{:<5} {:<6} {:<20} {:<16} {}", p.pid, p.core_id, p.name, p.mem_kb, p.syscalls);
                    canvas.draw_text(x + pad + theme.pt(8.0), cur_y + theme.pt(3.0), &line, 0xFFE2E8F0, theme.font_caption());
                    cur_y += row_h + 2;
                }

                // حالة الصوت العتادي
                cur_y += theme.pt(8.0);
                if cur_y + row_h < y + h - pad {
                    let aud = if sdk::audio::is_available() { "Intel AC'97 (48kHz Stereo ACTIVE)" } else { "Audio Hardware OFF" };
                    canvas.draw_text(x + pad + theme.pt(8.0), cur_y, &format!("Audio Subsystem: {}", aud), 0xFF34D399, theme.font_caption());
                }
            }
            TestTab::UiAnim => {
                // 4 Easing Curves + ImageWidget + SmoothScrollBar + Accent Color Swatches
                let half_w = (cw - pad) / 2;
                canvas.draw_rect(x + pad, cy, half_w, ch, 0xFF1E293B, theme.pt(8.0) as usize);
                canvas.draw_text(x + pad + theme.pt(10.0), cy + theme.pt(8.0), "Easing Curves (Linear / OutCubic / InOut / OutBack)", 0xFF38BDF8, theme.font_caption());

                let t = self.anim_phase;
                let curves = [
                    ("Linear", t, 0xFF38BDF8),
                    ("OutCubic", 1.0 - (1.0 - t).powi(3), 0xFF10B981),
                    ("InOutCubic", if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }, 0xFFF59E0B),
                    ("OutBack", (1.0 + 2.70158 * (t - 1.0).powi(3) + 1.70158 * (t - 1.0).powi(2)).clamp(0.0, 1.05), 0xFFEC4899),
                ];
                let track_w = half_w - theme.pt(110.0);
                for (i, (name, val, col)) in curves.iter().enumerate() {
                    let ry = cy + theme.pt(36.0) + (i as i32 * theme.pt(34.0));
                    canvas.draw_text(x + pad + theme.pt(10.0), ry + theme.pt(4.0), name, 0xFFE2E8F0, theme.font_caption());
                    let tx = x + pad + theme.pt(90.0);
                    canvas.draw_rect(tx, ry + theme.pt(8.0), track_w, theme.pt(6.0), 0xFF0F172A, 3);
                    let ball_x = tx + ((track_w - theme.pt(16.0)) as f32 * val.clamp(0.0, 1.0)) as i32;
                    canvas.draw_rect(ball_x, ry + theme.pt(2.0), theme.pt(18.0), theme.pt(18.0), *col, theme.pt(9.0) as usize);
                }

                // Accent Color Switcher (applies upstream to all apps!)
                let sw_y = cy + theme.pt(180.0);
                canvas.draw_text(x + pad + theme.pt(10.0), sw_y, "Live Upstream Accent Color:", 0xFFCBD5E1, theme.font_caption());
                let presets = [0xFF0284C7, 0xFF8B5CF6, 0xFFEC4899, 0xFF10B981, 0xFFF59E0B, 0xFFEF4444];
                for (i, &c) in presets.iter().enumerate() {
                    let bx = x + pad + theme.pt(10.0) + (i as i32 * theme.pt(34.0));
                    canvas.draw_rect(bx, sw_y + theme.pt(20.0), theme.pt(26.0), theme.pt(26.0), c, theme.pt(13.0) as usize);
                }

                // Right: ImageWidget + SmoothScrollBar
                let rx = x + pad + half_w + pad;
                canvas.draw_rect(rx, cy, half_w, ch, 0xFF1E293B, theme.pt(8.0) as usize);
                canvas.draw_text(rx + theme.pt(10.0), cy + theme.pt(8.0), "ImageWidget (Syscall 510) + SmoothScrollBar", 0xFF38BDF8, theme.font_caption());
                canvas.draw_text(rx + theme.pt(10.0), cy + theme.pt(26.0), &self.decode_status, 0xFF34D399, theme.font_caption());

                if let Some(ref img) = self.image_widget {
                    let ix = rx + theme.pt(12.0);
                    let iy = cy + theme.pt(50.0);
                    let box_sz = theme.pt(120.0);
                    for dy in 0..box_sz {
                        let sy = ((dy as usize) * img.img_h) / (box_sz as usize).max(1);
                        let py = iy + dy;
                        if py < 0 || py >= canvas.height as i32 { continue; }
                        for dx in 0..box_sz {
                            let sx = ((dx as usize) * img.img_w) / (box_sz as usize).max(1);
                            let px = ix + dx;
                            if px < 0 || px >= canvas.width as i32 { continue; }
                            canvas.buffer[(py as usize) * canvas.width + (px as usize)] = img.pixels[sy * img.img_w + sx];
                        }
                    }
                    canvas.draw_rect_outline(ix, iy, box_sz, box_sz, 0xFFFFFFFF, 0);
                }
                self.scrollbar.paint(canvas, rx + half_w - theme.pt(16.0), cy + theme.pt(40.0), ch - theme.pt(52.0));
            }
            TestTab::Color10BitFx => {
                // 8-bit vs 10-bit Dithered Gradient Banding Test + Aero Blur
                canvas.draw_rect(x + pad, cy, cw, ch, 0xFF1E293B, theme.pt(8.0) as usize);
                canvas.draw_text(x + pad + theme.pt(12.0), cy + theme.pt(10.0), "Standard 8-bit Gradient (Noticeable Banding on Dark Tones):", 0xFFCBD5E1, theme.font_caption());
                canvas.draw_gradient_h(x + pad + theme.pt(12.0), cy + theme.pt(30.0), cw - theme.pt(24.0), theme.pt(54.0), 0xFF0A1424, 0xFF1E3A5F, false);

                canvas.draw_text(x + pad + theme.pt(12.0), cy + theme.pt(96.0), "10-bit Simulated Bayer Ordered Dithered Gradient (Smooth):", 0xFF38BDF8, theme.font_caption());
                canvas.draw_gradient_h(x + pad + theme.pt(12.0), cy + theme.pt(116.0), cw - theme.pt(24.0), theme.pt(54.0), 0xFF0A1424, 0xFF1E3A5F, true);

                // Frosted Glass over Gradient
                let gx = x + pad + theme.pt(36.0) + ((self.anim_phase * 220.0) as i32);
                canvas.draw_frosted_glass_rect(gx, cy + theme.pt(45.0), theme.pt(180.0), theme.pt(110.0), 0x77163252, self.aa_radius);
                canvas.draw_rect_outline(gx, cy + theme.pt(45.0), theme.pt(180.0), theme.pt(110.0), theme.border_frost_highlight, self.aa_radius);
                canvas.draw_text(gx + theme.pt(14.0), cy + theme.pt(88.0), "Zero-Alloc Aero Blur", 0xFFFFFFFF, theme.font_body());
            }
            TestTab::DamageStress => {
                // Violent Damage Tracker Stress Test (24 Bouncing AA Rects)
                canvas.draw_rect(x + pad, cy, cw, ch, 0xFF0B1120, theme.pt(8.0) as usize);
                canvas.draw_text(x + pad + theme.pt(10.0), cy + theme.pt(8.0), &format!("Violent Damage Stress (24 Moving Rects) | Avg Frame: {} us | Click to Pause/Play", SmartLogger::average_frame_us()), 0xFFF59E0B, theme.font_caption());
                for b in &self.stress_boxes {
                    canvas.draw_rect(x + pad + 10 + b.x as i32, cy + theme.pt(30.0) + b.y as i32, theme.pt(26.0), theme.pt(26.0), b.color, theme.pt(6.0) as usize);
                }
            }
            TestTab::DebugBeta => {
                canvas.draw_rect(x + pad, cy, cw, ch, 0xFF1E293B, theme.pt(8.0) as usize);
                canvas.draw_text(x + pad + theme.pt(12.0), cy + theme.pt(10.0), &format!("Framework v{} Upstream Feature Gates (Applies to ALL Apps)", FRAMEWORK_VERSION), 0xFF38BDF8, theme.font_body());

                let toggles = [
                    (format!("Channel: {:?} (Switch Stable / Beta)", feats.channel), feats.channel == FrameworkChannel::Beta),
                    (format!("Upstream: Aero Frosted Blur [{}]", if feats.aero_blur { "ON" } else { "OFF" }), feats.aero_blur),
                    (format!("Upstream: AA Rounded Corners [{}]", if feats.aa_corners { "ON" } else { "OFF" }), feats.aa_corners),
                    (format!("Upstream: Smooth Animations [{}]", if feats.smooth_animations { "ON" } else { "OFF" }), feats.smooth_animations),
                    (format!("Debug: Damage Rects Red Overlay [{}]", if feats.debug_damage_overlay { "ON" } else { "OFF" }), feats.debug_damage_overlay),
                ];
                let row_h = theme.pt(28.0);
                for (i, (txt, on)) in toggles.iter().enumerate() {
                    let ry = cy + theme.pt(38.0) + (i as i32 * (row_h + theme.pt(6.0)));
                    canvas.draw_rect(x + pad + theme.pt(12.0), ry, cw - theme.pt(24.0), row_h, if *on { 0xFF0284C7 } else { 0xFF334155 }, theme.pt(5.0) as usize);
                    canvas.draw_text(x + pad + theme.pt(20.0), ry + theme.pt(6.0), txt, 0xFFFFFFFF, theme.font_caption());
                }

                // EOS Portal Bus Live Inspector (XDG-Style)
                let py = cy + theme.pt(214.0);
                canvas.draw_text(x + pad + theme.pt(12.0), py, "EOS Portal Bus (Exclusive vs Unlimited Providers):", 0xFF38BDF8, theme.font_caption());
                let portals = crate::framework::portal::list_portals();
                for (i, p) in portals.iter().enumerate() {
                    let ry = py + theme.pt(18.0) + (i as i32 * theme.pt(18.0));
                    if ry + theme.pt(18.0) > y + h - theme.pt(36.0) { break; }
                    let acc = if p.access == PortalAccess::Exclusive { "EXCL" } else { "UNLIM" };
                    let prov = p.providers.join(", ");
                    let line = format!("[{}] {:<28} -> {} (reqs: {})", acc, p.kind.name(), prov, p.request_count);
                    canvas.draw_text_clipped(x + pad + theme.pt(16.0), ry, cw - theme.pt(32.0), &line, 0xFFCBD5E1, theme.font_caption());
                }

                let btn_y = y + h - theme.pt(36.0);
                canvas.draw_rect(x + pad + theme.pt(12.0), btn_y, theme.pt(145.0), theme.pt(24.0), 0xFF059669, 4);
                canvas.draw_text(x + pad + theme.pt(20.0), btn_y + theme.pt(4.0), "Test FilePicker Portal", 0xFFFFFFFF, theme.font_caption());
                canvas.draw_text_clipped(x + pad + theme.pt(166.0), btn_y + theme.pt(4.0), cw - theme.pt(180.0), &crate::framework::portal::last_portal_log(), 0xFF34D399, theme.font_caption());
            }
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        let theme = get_theme();
        let pad = theme.pt(10.0);
        let tb_h = theme.pt(34.0);
        let tab_w = theme.pt(112.0);

        if !pressed {
            self.scrollbar.hovered = mx >= self.bounds.x + self.bounds.w - theme.pt(30.0);
            return true;
        }

        if my >= self.bounds.y && my <= self.bounds.y + tb_h {
            for i in 0..5 {
                let tx = self.bounds.x + pad + (i as i32 * (tab_w + theme.pt(4.0)));
                if mx >= tx && mx <= tx + tab_w {
                    self.tab = match i {
                        0 => TestTab::GpuProcs,
                        1 => TestTab::UiAnim,
                        2 => TestTab::Color10BitFx,
                        3 => TestTab::DamageStress,
                        _ => TestTab::DebugBeta,
                    };
                    return true;
                }
            }
            return true;
        }

        let cy = self.bounds.y + tb_h + pad;
        self.click_count += 1;

        match self.tab {
            TestTab::UiAnim => {
                let sw_y = cy + theme.pt(200.0);
                let presets = [0xFF0284C7, 0xFF8B5CF6, 0xFFEC4899, 0xFF10B981, 0xFFF59E0B, 0xFFEF4444];
                if my >= sw_y && my <= sw_y + theme.pt(30.0) {
                    for (i, &c) in presets.iter().enumerate() {
                        let bx = self.bounds.x + pad + theme.pt(10.0) + (i as i32 * theme.pt(34.0));
                        if mx >= bx && mx <= bx + theme.pt(28.0) {
                            set_theme_accent(c);
                            redraw_fullscreen();
                            break;
                        }
                    }
                }
            }
            TestTab::DamageStress => {
                self.stress_active = !self.stress_active;
            }
            TestTab::DebugBeta => {
                let row_h = theme.pt(28.0);
                for i in 0..5 {
                    let ry = cy + theme.pt(38.0) + (i as i32 * (row_h + theme.pt(6.0)));
                    if my >= ry && my <= ry + row_h {
                        match i {
                            0 => set_channel(if get_features().channel == FrameworkChannel::Stable { FrameworkChannel::Beta } else { FrameworkChannel::Stable }),
                            1 => { toggle_aero_blur(); }
                            2 => { toggle_aa_corners(); }
                            3 => { toggle_smooth_animations(); }
                            4 => { toggle_debug_damage_overlay(); }
                            _ => {}
                        }
                        return true;
                    }
                }
                let btn_y = self.bounds.y + self.bounds.h - theme.pt(36.0);
                if my >= btn_y && my <= btn_y + theme.pt(24.0) {
                    let _ = crate::framework::portal::request_portal(PortalRequest {
                        kind: PortalKind::FilePicker,
                        caller: "TestApp".into(),
                        payload: "open_file".into(),
                    });
                }
            }
            _ => {}
        }
        true
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        self.scrollbar.scroll_by(if dy < 0 { 0.12 } else { -0.12 });
        true
    }
}
