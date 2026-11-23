use std::time::{Duration, Instant};
use crate::framework::*;
use crate::ffi_media::FfmpegDecoder;
use crate::sdk;

#[derive(Clone, Debug)]
struct SubCue {
    start_ms: i64,
    end_ms: i64,
    text: String,
}

fn parse_srt_ts(s: &str) -> Option<i64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 { return None; }
    let h: i64 = parts[0].trim().parse().ok()?;
    let m: i64 = parts[1].trim().parse().ok()?;
    let sec_parts: Vec<&str> = parts[2].split('.').collect();
    let sec: i64 = sec_parts[0].trim().parse().ok()?;
    let ms: i64 = if sec_parts.len() > 1 {
        let raw = sec_parts[1].trim();
        let val: i64 = raw.parse().unwrap_or(0);
        if raw.len() == 2 { val * 10 } else if raw.len() == 1 { val * 100 } else { val }
    } else { 0 };
    Some(((h * 3600 + m * 60 + sec) * 1000) + ms)
}

fn strip_ass_tags(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_brace = false;
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' { in_brace = true; continue; }
        if c == '}' { in_brace = false; continue; }
        if in_brace { continue; }
        if c == '\\' {
            if let Some(&n) = chars.peek() {
                if n == 'N' || n == 'n' {
                    chars.next();
                    out.push('\n');
                    continue;
                }
            }
        }
        out.push(c);
    }
    out.trim().to_string()
}

fn parse_subtitles(content: &str, is_ass: bool) -> Vec<SubCue> {
    let mut cues = Vec::new();
    if is_ass {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("Dialogue:") {
                let mut fields = rest.splitn(10, ',');
                let _layer = fields.next();
                let start = fields.next().and_then(parse_srt_ts);
                let end = fields.next().and_then(parse_srt_ts);
                for _ in 0..6 { let _ = fields.next(); }
                let txt = fields.next().map(strip_ass_tags).unwrap_or_default();
                if let (Some(st), Some(en)) = (start, end) {
                    if !txt.is_empty() {
                        cues.push(SubCue { start_ms: st, end_ms: en, text: txt });
                    }
                }
            }
        }
    } else {
        let mut lines = content.lines().peekable();
        while let Some(line) = lines.next() {
            if line.contains("-->") {
                let mut parts = line.split("-->");
                if let (Some(s1), Some(s2)) = (parts.next(), parts.next()) {
                    if let (Some(st), Some(en)) = (parse_srt_ts(s1), parse_srt_ts(s2)) {
                        let mut text = String::new();
                        while let Some(tl) = lines.next() {
                            let t = tl.trim();
                            if t.is_empty() { break; }
                            if !text.is_empty() { text.push('\n'); }
                            text.push_str(t);
                        }
                        if !text.is_empty() {
                            cues.push(SubCue { start_ms: st, end_ms: en, text });
                        }
                    }
                }
            }
        }
    }
    cues
}

pub struct VideoPlayerApp {
    bounds: Rect,
    pub playing: bool,
    pub frame_idx: u64,
    pub current_ms: i64,
    pub duration_ms: i64,
    pub speed_idx: usize,
    pub volume_pct: u8,
    pub show_stats: bool,
    pub status: String,
    pub hw_backend: &'static str,
    pub audio_ready: bool,
    pub loaded_file: Option<String>,
    pub decoder: Option<FfmpegDecoder>,
    pub frame_buf: Vec<u32>,
    pub audio_scratch: Vec<i16>,
    pub frame_w: usize,
    pub frame_h: usize,
    pub last_decode_ms: f32,
    pub measured_fps: f32,
    pub dropped_frames: u64,
    fps_window_start: Instant,
    fps_window_frames: u32,
    pub next_frame_at: Instant,
    subtitles: Vec<SubCue>,
    sub_label: String,
    picking_sub: bool,
    pub is_fullscreen: bool,
    pub always_on_top: bool,
    last_mouse_move: Instant,
}

const SPEEDS: [f64; 4] = [0.5, 1.0, 1.5, 2.0];

impl VideoPlayerApp {
    pub fn new() -> Self {
        let gpu = sdk::gpu::query_info();
        let audio_ready = sdk::audio::is_available();
        if audio_ready { sdk::audio::set_volume(85); }
        let _ = portal::request_portal(PortalRequest {
            kind: PortalKind::AudioOutput,
            caller: "VideoPlayer".into(),
            payload: "attach_ac97".into(),
        });
        Self {
            bounds: Rect::default(),
            playing: false,
            frame_idx: 0,
            current_ms: 0,
            duration_ms: 0,
            speed_idx: 1,
            volume_pct: 85,
            show_stats: false,
            status: "Drop or open a video file (.mp4 / .mkv)".into(),
            hw_backend: gpu.backend_name(),
            audio_ready,
            loaded_file: None,
            decoder: None,
            frame_buf: Vec::new(),
            audio_scratch: vec![0i16; 8192],
            frame_w: 0,
            frame_h: 0,
            last_decode_ms: 0.0,
            measured_fps: 0.0,
            dropped_frames: 0,
            fps_window_start: Instant::now(),
            fps_window_frames: 0,
            next_frame_at: Instant::now(),
            subtitles: Vec::new(),
            sub_label: "None".into(),
            picking_sub: false,
            is_fullscreen: false,
            always_on_top: false,
            last_mouse_move: Instant::now(),
        }
    }

    fn load_subtitle_file(&mut self, path: &str) -> bool {
        let attempts = [format!("/{}", path.trim_start_matches('/')), path.to_string()];
        for p in &attempts {
            if let Ok(text) = std::fs::read_to_string(p) {
                let is_ass = p.to_ascii_lowercase().ends_with(".ass");
                let cues = parse_subtitles(&text, is_ass);
                if !cues.is_empty() {
                    self.sub_label = format!("{} ({} cues)", path.rsplit('/').next().unwrap_or(path), cues.len());
                    self.subtitles = cues;
                    return true;
                }
            }
        }
        false
    }

    fn try_autoload_subtitles(&mut self, video_path: &str) {
        self.subtitles.clear();
        self.sub_label = "None".into();
        if let Some(dot) = video_path.rfind('.') {
            let base = &video_path[..dot];
            if self.load_subtitle_file(&format!("{}.ass", base)) { return; }
            if self.load_subtitle_file(&format!("{}.srt", base)) { return; }
        }
    }

    pub fn open_media_file(&mut self, path: &str) {
        let lower = path.to_ascii_lowercase();
        if self.picking_sub || lower.ends_with(".srt") || lower.ends_with(".ass") {
            self.picking_sub = false;
            if self.load_subtitle_file(path) {
                self.status = format!("Loaded subtitles: {}", self.sub_label);
            }
            return;
        }

        let attempts = [format!("/{}", path.trim_start_matches('/')), path.to_string()];
        for p in &attempts {
            if let Ok(mut dec) = FfmpegDecoder::open(p) {
                self.duration_ms = dec.meta.duration_ms.max(1000);
                self.current_ms = 0;
                self.frame_idx = 0;
                self.loaded_file = Some(path.to_string());
                self.try_autoload_subtitles(path);
                self.status = format!(
                    "{} [{} {}x{} @ {:.1}fps]",
                    path.rsplit('/').next().unwrap_or(path),
                    dec.meta.codec,
                    dec.meta.width,
                    dec.meta.height,
                    dec.meta.fps
                );
                let (tw, th) = self.compute_target_dims(dec.meta.width, dec.meta.height);
                self.frame_w = tw;
                self.frame_h = th;
                self.frame_buf.resize(tw * th, 0xFF000000);
                let t0 = SmartLogger::read_tsc();
                if let Ok(Some(pts)) = dec.next_frame(&mut self.frame_buf, tw, th) {
                    self.current_ms = pts;
                }
                let us = SmartLogger::cycles_to_us(SmartLogger::read_tsc().saturating_sub(t0));
                self.last_decode_ms = us as f32 / 1000.0;
                self.measured_fps = dec.meta.fps as f32;
                self.dropped_frames = 0;
                self.fps_window_start = Instant::now();
                self.fps_window_frames = 0;
                self.next_frame_at = Instant::now();
                self.decoder = Some(dec);
                self.playing = true;
                return;
            }
        }
        self.status = format!("Could not decode '{}'", path);
    }

    fn compute_target_dims(&self, src_w: usize, src_h: usize) -> (usize, usize) {
        let theme = get_theme();
        let max_w = (self.bounds.w.max(320) as usize).min(2560);
        let reserved_h = if self.is_fullscreen { 0 } else { theme.pt(54.0) };
        let max_h = ((self.bounds.h - reserved_h).max(240) as usize).min(1440);
        let scale = (max_w as f32 / src_w.max(1) as f32).min(max_h as f32 / src_h.max(1) as f32);
        let w = ((src_w as f32 * scale) as usize).max(16) & !1;
        let h = ((src_h as f32 * scale) as usize).max(16) & !1;
        (w, h)
    }

    pub fn seek_relative(&mut self, delta_ms: i64) {
        let target = (self.current_ms + delta_ms).clamp(0, self.duration_ms);
        if let Some(ref mut dec) = self.decoder {
            if dec.seek_ms(target) {
                self.current_ms = target;
                self.next_frame_at = Instant::now();
                if self.frame_w > 0 && self.frame_h > 0 {
                    let _ = dec.next_frame(&mut self.frame_buf, self.frame_w, self.frame_h);
                }
            }
        }
    }

    pub fn tick_frame(&mut self) -> bool {
        if let Some(path) = portal::take_completed_file_pick("VideoPlayer") {
            self.open_media_file(&path);
            return true;
        }
        if !self.playing || self.decoder.is_none() {
            return false;
        }

        // VSync / Accumulator Frame Pacing (prevents 1x drift)
        let now = Instant::now();
        if now < self.next_frame_at {
            return false;
        }

        let (src_w, src_h, fps) = {
            let m = &self.decoder.as_ref().unwrap().meta;
            (m.width, m.height, m.fps)
        };
        let eff_fps = (fps * SPEEDS[self.speed_idx]).clamp(1.0, 120.0);
        let interval = Duration::from_secs_f64(1.0 / eff_fps);

        let lag = now.saturating_duration_since(self.next_frame_at);
        if lag > Duration::from_millis(200) {
            self.dropped_frames += (lag.as_secs_f64() / interval.as_secs_f64()).floor() as u64;
            self.next_frame_at = now + interval;
        } else {
            self.next_frame_at += interval;
        }

        let (tw, th) = self.compute_target_dims(src_w, src_h);
        if tw != self.frame_w || th != self.frame_h {
            self.frame_w = tw;
            self.frame_h = th;
            self.frame_buf.resize(tw * th, 0xFF000000);
        }

        let t0 = SmartLogger::read_tsc();
        let res = self.decoder.as_mut().unwrap().next_frame(&mut self.frame_buf, tw, th);
        let us = SmartLogger::cycles_to_us(SmartLogger::read_tsc().saturating_sub(t0));
        self.last_decode_ms = us as f32 / 1000.0;

        self.fps_window_frames += 1;
        let win_elapsed = now.saturating_duration_since(self.fps_window_start).as_secs_f32();
        if win_elapsed >= 0.5 {
            self.measured_fps = self.fps_window_frames as f32 / win_elapsed;
            self.fps_window_frames = 0;
            self.fps_window_start = now;
        }

        // Drain decoded PCM audio to AC'97 Hardware Audio
        if self.audio_ready && self.volume_pct > 0 {
            let n = self.decoder.as_mut().unwrap().pop_audio(&mut self.audio_scratch);
            if n > 0 {
                sdk::audio::write_pcm_i16(&self.audio_scratch[..n]);
            }
        }

        match res {
            Ok(Some(pts)) => {
                self.current_ms = pts;
                self.frame_idx += 1;
                true
            }
            Ok(None) => {
                if let Some(ref mut dec) = self.decoder {
                    let _ = dec.seek_ms(0);
                }
                self.current_ms = 0;
                self.next_frame_at = Instant::now();
                true
            }
            Err(_) => {
                self.playing = false;
                false
            }
        }
    }

    fn active_subtitle(&self) -> Option<&str> {
        for c in &self.subtitles {
            if self.current_ms >= c.start_ms && self.current_ms <= c.end_ms {
                return Some(&c.text);
            }
        }
        None
    }
}

impl Widget for VideoPlayerApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let x = self.bounds.x;
        let y = self.bounds.y;
        let w = self.bounds.w;
        let h = self.bounds.h;
        let show_osd = !self.is_fullscreen || self.last_mouse_move.elapsed() < Duration::from_millis(2500);
        let osd_h = if show_osd { theme.pt(54.0) } else { 0 };
        let vid_h = if self.is_fullscreen { h } else { (h - osd_h).max(theme.pt(80.0)) };

        // 1. Cinema Black Viewport
        canvas.draw_rect(x, y, w, h, 0xFF05070B, 0);

        if self.decoder.is_some() && self.frame_w > 0 && self.frame_h > 0 && self.frame_buf.len() >= self.frame_w * self.frame_h {
            let off_x = x + (w - self.frame_w as i32) / 2;
            let off_y = y + (vid_h - self.frame_h as i32) / 2;
            let clip = canvas.current_clip();
            let x1 = off_x.max(clip.x).max(0);
            let y1 = off_y.max(clip.y).max(0);
            let x2 = (off_x + self.frame_w as i32).min(clip.x + clip.w).min(canvas.width as i32);
            let y2 = (off_y + self.frame_h as i32).min(clip.y + clip.h).min(canvas.height as i32);
            if x2 > x1 && y2 > y1 {
                let cw = (x2 - x1) as usize;
                let sx0 = (x1 - off_x) as usize;
                for py in y1..y2 {
                    let sy = (py - off_y) as usize;
                    let src_off = sy * self.frame_w + sx0;
                    let dst_off = (py as usize) * canvas.width + (x1 as usize);
                    canvas.buffer[dst_off..dst_off + cw].copy_from_slice(&self.frame_buf[src_off..src_off + cw]);
                }
            }

            // Subtitle Overlay (.srt / .ass)
            if let Some(sub_txt) = self.active_subtitle() {
                let lines: Vec<&str> = sub_txt.lines().collect();
                let line_h = theme.pt(22.0);
                let total_h = (lines.len() as i32) * line_h + theme.pt(10.0);
                let sy0 = y + vid_h - total_h - theme.pt(14.0);
                for (i, line) in lines.iter().enumerate() {
                    let (tw, _) = canvas.measure_text(line, theme.font_body());
                    let pill_w = (tw as i32 + theme.pt(20.0)).min(w - theme.pt(32.0));
                    let px = x + (w - pill_w) / 2;
                    let py = sy0 + (i as i32 * line_h);
                    canvas.draw_rect(px, py, pill_w, line_h, 0xBB000000, 4);
                    canvas.draw_text(px + theme.pt(10.0), py + theme.pt(2.0), line, 0xFFFFFFFF, theme.font_body());
                }
            }
        } else {
            // Clean VLC-Style Idle Start Screen (Zero CPU when idle)
            let ic_sz = theme.pt(56.0);
            let cx = x + w / 2;
            let cy = y + vid_h / 2;
            crate::framework::icons::draw_svg_icon(canvas, "video", cx - ic_sz / 2, cy - theme.pt(68.0), ic_sz, ic_sz);
            let title = "EOS Media Player";
            let (tw, _) = canvas.measure_text(title, theme.font_title());
            canvas.draw_text(cx - tw as i32 / 2, cy - theme.pt(4.0), title, 0xFFF8FAFC, theme.font_title());

            let btn_w = theme.pt(140.0);
            let btn_h = theme.pt(34.0);
            canvas.draw_rect(cx - btn_w / 2, cy + theme.pt(24.0), btn_w, btn_h, theme.accent, theme.pt(6.0) as usize);
            let open_lbl = "Open Video";
            let (ow, _) = canvas.measure_text(open_lbl, theme.font_body());
            canvas.draw_text(cx - ow as i32 / 2, cy + theme.pt(31.0), open_lbl, 0xFFFFFFFF, theme.font_body());
        }

        // 2. mpv-Style Telemetry Stats HUD (100% Real Stream & Render Metrics)
        if self.show_stats {
            let hud_w = theme.pt(440.0).min(w - theme.pt(24.0));
            let hud_h = theme.pt(156.0);
            let hx = x + theme.pt(12.0);
            let hy = y + theme.pt(12.0);
            canvas.draw_frosted_glass_rect(hx, hy, hud_w, hud_h, 0xCC090D16, theme.pt(6.0) as usize);
            canvas.draw_rect_outline(hx, hy, hud_w, hud_h, 0x7738BDF8, theme.pt(6.0) as usize);

            let line_h = theme.pt(18.0);
            let tx = hx + theme.pt(10.0);
            let max_tw = hud_w - theme.pt(20.0);
            let mut ty = hy + theme.pt(8.0);

            if let Some(ref dec) = self.decoder {
                let fname = self.loaded_file.as_deref().and_then(|p| p.rsplit('/').next()).unwrap_or("Stream");
                canvas.draw_text_clipped(tx, ty, max_tw, &format!("File : {}", fname), 0xFF38BDF8, theme.font_caption());
                ty += line_h;

                let br_str = if dec.meta.video_bitrate_kbps > 0 {
                    format!("{} kbps", dec.meta.video_bitrate_kbps)
                } else {
                    "VBR".to_string()
                };
                canvas.draw_text_clipped(
                    tx, ty, max_tw,
                    &format!("Video: {} | {}x{} -> {}x{} | {}", dec.meta.codec, dec.meta.width, dec.meta.height, self.frame_w, self.frame_h, br_str),
                    0xFFF8FAFC, theme.font_caption()
                );
                ty += line_h;

                canvas.draw_text_clipped(
                    tx, ty, max_tw,
                    &format!("Color: {} ({}-bit) -> BGRA8888 (8-bit) | Matrix: {} ({})", dec.meta.pix_fmt, dec.meta.bit_depth, dec.meta.color_space, dec.meta.color_range),
                    0xFFFBBF24, theme.font_caption()
                );
                ty += line_h;

                canvas.draw_text_clipped(
                    tx, ty, max_tw,
                    &format!("Sync : {:.2} / {:.2} fps (Speed {:.1}x) | Decode: {:.2} ms | Dropped: {}", self.measured_fps, dec.meta.fps, SPEEDS[self.speed_idx], self.last_decode_ms, self.dropped_frames),
                    0xFF34D399, theme.font_caption()
                );
                ty += line_h;

                let aud_line = if dec.meta.audio_codec != "NONE" && dec.meta.audio_sample_rate > 0 {
                    let abr = if dec.meta.audio_bitrate_kbps > 0 { format!("{} kbps", dec.meta.audio_bitrate_kbps) } else { "VBR".into() };
                    format!("Audio: {} {} Hz {}ch ({}) -> {}", dec.meta.audio_codec, dec.meta.audio_sample_rate, dec.meta.audio_channels, abr, if self.audio_ready { "AC97 48kHz" } else { "Muted" })
                } else {
                    "Audio: No audio track in container".to_string()
                };
                canvas.draw_text_clipped(tx, ty, max_tw, &aud_line, 0xFFE2E8F0, theme.font_caption());
                ty += line_h;

                canvas.draw_text_clipped(
                    tx, ty, max_tw,
                    &format!("VO   : {} | Subs: {}", self.hw_backend, self.sub_label),
                    0xFF94A3B8, theme.font_caption()
                );
            } else {
                canvas.draw_text_clipped(tx, ty, max_tw, "EOS Media Player — No Active Stream", 0xFF38BDF8, theme.font_caption());
                ty += line_h;
                canvas.draw_text_clipped(tx, ty, max_tw, &format!("VO   : {} | Audio Out: {}", self.hw_backend, if self.audio_ready { "Intel AC'97 (48kHz)" } else { "OFF" }), 0xFFE2E8F0, theme.font_caption());
                ty += line_h;
                canvas.draw_text_clipped(tx, ty, max_tw, "Keys : [Space] Play/Pause  [<-/->] Seek  [F] Fullscreen  [I] Stats", 0xFF94A3B8, theme.font_caption());
            }
        }

        if !show_osd { return; }

        // 3. Floating / Docked Frosted Glass OSD Control Bar
        let osd_y = if self.is_fullscreen { y + h - osd_h } else { y + vid_h };
        canvas.draw_frosted_glass_rect(x, osd_y, w, osd_h, 0xDD0F172A, 0);
        canvas.draw_line_h(x, osd_y, w, theme.border_frost_highlight);

        let seek_x = x + theme.pt(12.0);
        let seek_y = osd_y + theme.pt(7.0);
        let seek_w = (w - theme.pt(24.0)).max(20);
        let seek_h = theme.pt(6.0);
        canvas.draw_rect(seek_x, seek_y, seek_w, seek_h, 0xFF334155, 3);
        let prog = if self.duration_ms > 0 { (self.current_ms as f32 / self.duration_ms as f32).clamp(0.0, 1.0) } else { 0.0 };
        let fill_w = (seek_w as f32 * prog) as i32;
        canvas.draw_rect(seek_x, seek_y, fill_w, seek_h, theme.accent_hover, 3);

        let btn_y = osd_y + theme.pt(20.0);
        let btn_h = theme.pt(26.0);
        let gap = theme.pt(5.0);
        let mut bx = seek_x;
        let btns = [
            (if self.playing { "Pause" } else { "Play" }, theme.pt(50.0), theme.accent),
            ("-5s", theme.pt(32.0), 0xFF334155),
            ("+5s", theme.pt(32.0), 0xFF334155),
            ("Open", theme.pt(48.0), 0xFF059669),
            ("Sub", theme.pt(38.0), 0xFF334155),
            ("Speed", theme.pt(48.0), 0xFF334155),
            ("Full", theme.pt(40.0), if self.is_fullscreen { 0xFF0284C7 } else { 0xFF334155 }),
            ("Pin", theme.pt(36.0), if self.always_on_top { 0xFFD97706 } else { 0xFF334155 }),
            ("Stats", theme.pt(44.0), if self.show_stats { 0xFF0284C7 } else { 0xFF334155 }),
        ];

        for (label, bw, col) in btns {
            canvas.draw_rect(bx, btn_y, bw, btn_h, col, theme.pt(5.0) as usize);
            let (tw, _) = canvas.measure_text(label, theme.font_caption());
            canvas.draw_text(bx + (bw - tw as i32) / 2, btn_y + theme.pt(5.0), label, 0xFFFFFFFF, theme.font_caption());
            bx += bw + gap;
        }

        let cur_s = (self.current_ms / 1000).max(0);
        let dur_s = (self.duration_ms / 1000).max(0);
        let tc = format!("{:02}:{:02}/{:02}:{:02} {:.1}x Vol:{}%", cur_s / 60, cur_s % 60, dur_s / 60, dur_s % 60, SPEEDS[self.speed_idx], self.volume_pct);
        canvas.draw_text_clipped(bx + theme.pt(6.0), btn_y + theme.pt(5.0), (x + w - bx - theme.pt(12.0)).max(20), &tc, 0xFFCBD5E1, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        self.last_mouse_move = Instant::now();
        if !pressed { return true; }

        let theme = get_theme();
        let osd_h = theme.pt(54.0);
        let vid_h = if self.is_fullscreen { self.bounds.h } else { (self.bounds.h - osd_h).max(theme.pt(80.0)) };
        let osd_y = if self.is_fullscreen { self.bounds.y + self.bounds.h - osd_h } else { self.bounds.y + vid_h };

        // Center "Open Video" button on Idle Screen
        if self.decoder.is_none() && my < osd_y {
            let cx = self.bounds.x + self.bounds.w / 2;
            let cy = self.bounds.y + vid_h / 2;
            let btn_w = theme.pt(140.0);
            let btn_h = theme.pt(34.0);
            if mx >= cx - btn_w / 2 && mx <= cx + btn_w / 2 && my >= cy + theme.pt(24.0) && my <= cy + theme.pt(24.0) + btn_h {
                self.picking_sub = false;
                let _ = portal::request_portal(PortalRequest { kind: PortalKind::FilePicker, caller: "VideoPlayer".into(), payload: "mp4,mkv,webm,avi,mov".into() });
                return true;
            }
        }

        let seek_x = self.bounds.x + theme.pt(12.0);
        let seek_w = (self.bounds.w - theme.pt(24.0)).max(20);
        if my >= osd_y && my <= osd_y + theme.pt(18.0) && self.duration_ms > 0 {
            let ratio = ((mx - seek_x) as f32 / seek_w as f32).clamp(0.0, 1.0);
            let target = (ratio * self.duration_ms as f32) as i64;
            if let Some(ref mut dec) = self.decoder {
                if dec.seek_ms(target) {
                    self.current_ms = target;
                    self.next_frame_at = Instant::now();
                    if self.frame_w > 0 && self.frame_h > 0 {
                        let _ = dec.next_frame(&mut self.frame_buf, self.frame_w, self.frame_h);
                    }
                }
            }
            return true;
        }

        let btn_y = osd_y + theme.pt(20.0);
        let btn_h = theme.pt(26.0);
        let gap = theme.pt(5.0);
        if my >= btn_y && my <= btn_y + btn_h {
            let widths = [theme.pt(50.0), theme.pt(32.0), theme.pt(32.0), theme.pt(48.0), theme.pt(38.0), theme.pt(48.0), theme.pt(40.0), theme.pt(36.0), theme.pt(44.0)];
            let mut bx = seek_x;
            for (idx, bw) in widths.iter().enumerate() {
                if mx >= bx && mx <= bx + *bw {
                    match idx {
                        0 => { self.playing = !self.playing; self.next_frame_at = Instant::now(); }
                        1 => self.seek_relative(-5000),
                        2 => self.seek_relative(5000),
                        3 => { self.picking_sub = false; let _ = portal::request_portal(PortalRequest { kind: PortalKind::FilePicker, caller: "VideoPlayer".into(), payload: "mp4,mkv,webm,avi,mov".into() }); }
                        4 => { self.picking_sub = true; let _ = portal::request_portal(PortalRequest { kind: PortalKind::FilePicker, caller: "VideoPlayer".into(), payload: "ass,srt".into() }); }
                        5 => self.speed_idx = (self.speed_idx + 1) % SPEEDS.len(),
                        6 => { self.is_fullscreen = !self.is_fullscreen; portal::request_fullscreen_toggle(); }
                        7 => { self.always_on_top = !self.always_on_top; portal::request_always_on_top_toggle(); }
                        8 => self.show_stats = !self.show_stats,
                        _ => {}
                    }
                    return true;
                }
                bx += *bw + gap;
            }
        }
        true
    }

    fn handle_key(&mut self, keycode: u8, _mods: u8) -> bool {
        match keycode {
            0x39 => { self.playing = !self.playing; self.next_frame_at = Instant::now(); true } // Space
            0x4B => { self.seek_relative(-5000); true } // Left
            0x4D => { self.seek_relative(5000); true }  // Right
            0x21 => { self.is_fullscreen = !self.is_fullscreen; portal::request_fullscreen_toggle(); true } // 'F'
            0x01 if self.is_fullscreen => { self.is_fullscreen = false; portal::request_fullscreen_toggle(); true } // 'Esc'
            0x17 => { self.show_stats = !self.show_stats; true }  // 'I'
            _ => false,
        }
    }
}
