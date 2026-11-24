use std::fs;
use crate::framework::*;
use crate::sdk;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Appearance,
    Wallpapers,
    SystemHw,
}

struct WallpaperItem {
    name: String,
    thumb: ImageWidget,
    src_w: usize,
    src_h: usize,
}

pub struct SettingsApp {
    bounds: Rect,
    tab: SettingsTab,
    style: ThemeStyle,
    dark_mode: bool,
    accent: u32,
    scale: f32,
    ui_font: String,
    mono_font: String,
    active_wallpaper: String,
    wallpapers: Vec<WallpaperItem>,
    wallpapers_loaded: bool,
    wp_scroll: i32,
    status_msg: String,
}

impl SettingsApp {
    pub fn new() -> Self {
        let mut dark_mode = true;
        let mut style = ThemeStyle::AeroGlass;
        let mut accent = 0xFF0284C7u32;
        let mut scale = 2.0f32;
        let mut ui_font = crate::framework::fonts::active_font_name(FontRole::Ui);
        let mut mono_font = crate::framework::fonts::active_font_name(FontRole::Mono);
        let mut active_wallpaper = "none".to_string();

        if let Ok(c) = fs::read_to_string("RootFS/settings.ini").or_else(|_| fs::read_to_string("settings.ini")) {
            for line in c.lines() {
                let l = line.trim();
                if let Some(v) = l.strip_prefix("dark_mode=") {
                    dark_mode = v.trim() == "1" || v.trim().eq_ignore_ascii_case("true");
                } else if let Some(v) = l.strip_prefix("style=") {
                    if v.trim().eq_ignore_ascii_case("modern") { style = ThemeStyle::CleanModern; }
                } else if let Some(v) = l.strip_prefix("accent=") {
                    let hex = v.trim().trim_start_matches("0x").trim_start_matches('#');
                    if let Ok(col) = u32::from_str_radix(hex, 16) {
                        accent = if col <= 0xFFFFFF { 0xFF000000 | col } else { col };
                    }
                } else if let Some(v) = l.strip_prefix("scale=") {
                    if let Ok(s) = v.trim().parse::<f32>() { scale = s; }
                } else if let Some(v) = l.strip_prefix("wallpaper=") {
                    active_wallpaper = v.trim().to_string();
                } else if let Some(v) = l.strip_prefix("ui_font=") {
                    if !v.trim().is_empty() { ui_font = v.trim().to_string(); }
                } else if let Some(v) = l.strip_prefix("mono_font=") {
                    if !v.trim().is_empty() { mono_font = v.trim().to_string(); }
                }
            }
        }

        Self {
            bounds: Rect::default(),
            tab: SettingsTab::Appearance,
            style,
            dark_mode,
            accent,
            scale,
            ui_font,
            mono_font,
            active_wallpaper,
            wallpapers: Vec::new(),
            wallpapers_loaded: false,
            wp_scroll: 0,
            status_msg: "Instant launch — Wallpapers load lazily when opened.".into(),
        }
    }

    fn scan_wallpapers(&mut self) {
        self.wallpapers.clear();
        let dirs = ["EOS SHARE", "/EOS SHARE"];
        for d in &dirs {
            if let Ok(rd) = fs::read_dir(d) {
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let lower = name.to_ascii_lowercase();
                    if !(lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")) {
                        continue;
                    }
                    let full = format!("{}/{}", d, name);
                    if let Ok(bytes) = fs::read(&full) {
                        if let Ok((px, w, h, _)) = sdk::media::decode_image(&bytes) {
                            // Aspect-preserving pre-scale (max 200px) so ImageWidget renders crisp thumbnails
                            let scale = (200.0 / (w.max(h) as f32)).min(1.0);
                            let tw = ((w as f32 * scale).round() as usize).max(16);
                            let th = ((h as f32 * scale).round() as usize).max(16);
                            let mut thumb_px = vec![0xFF0F172A; tw * th];
                            for ty in 0..th {
                                let sy = (ty * h) / th.max(1);
                                for tx in 0..tw {
                                    let sx = (tx * w) / tw.max(1);
                                    thumb_px[ty * tw + tx] = px[sy.min(h - 1) * w + sx.min(w - 1)];
                                }
                            }
                            self.wallpapers.push(WallpaperItem {
                                name,
                                thumb: ImageWidget::new_cover(thumb_px, tw, th),
                                src_w: w,
                                src_h: h,
                            });
                        }
                    }
                }
                if !self.wallpapers.is_empty() { break; }
            }
        }
    }

    fn persist_and_apply(&mut self) {
        set_theme_style(self.style, self.dark_mode);
        set_theme_accent(self.accent);
        set_theme_scale(self.scale);

        let _ = crate::framework::fonts::set_role_font(FontRole::Ui, &self.ui_font);
        let _ = crate::framework::fonts::set_role_font(FontRole::Mono, &self.mono_font);
        get_theme_mut().ui_font = self.ui_font.clone();
        get_theme_mut().mono_font = self.mono_font.clone();

        let style_str = if self.style == ThemeStyle::CleanModern { "modern" } else { "aero" };
        let ini = format!(
            "style={}\ndark_mode={}\naccent={:#010x}\nscale={:.1}\nui_font={}\nmono_font={}\nwallpaper={}\n",
            style_str,
            if self.dark_mode { 1 } else { 0 },
            self.accent,
            self.scale,
            self.ui_font,
            self.mono_font,
            self.active_wallpaper
        );
        let _ = fs::write("RootFS/settings.ini", &ini);
        let _ = fs::write("settings.ini", &ini);
        redraw_fullscreen();
    }
}

impl Widget for SettingsApp {
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
        let pad = theme.pt(12.0);

        canvas.draw_rect(x, y, w, h, theme.bg_app, 0);

        // Clean 3-Tab Header
        let tb_h = theme.pt(36.0);
        canvas.draw_rect(x, y, w, tb_h, theme.bg_sidebar, 0);
        canvas.draw_line_h(x, y + tb_h, w, theme.border_window);

        let tabs = [
            (SettingsTab::Appearance, "Appearance & Theme", "settings"),
            (SettingsTab::Wallpapers, "Wallpapers", "image"),
            (SettingsTab::SystemHw, "System & Hardware", "lighthouse"),
        ];
        let tab_w = theme.pt(160.0);
        for (i, (t, lbl, ic)) in tabs.iter().enumerate() {
            let tx = x + pad + (i as i32 * (tab_w + theme.pt(6.0)));
            let ty = y + theme.pt(4.0);
            let th = tb_h - theme.pt(8.0);
            let bg = if self.tab == *t { theme.accent } else { theme.bg_panel };
            canvas.draw_rect(tx, ty, tab_w, th, bg, 5);
            let ic_sz = theme.pt(16.0);
            crate::framework::icons::draw_svg_icon(canvas, ic, tx + theme.pt(8.0), ty + (th - ic_sz) / 2, ic_sz, ic_sz);
            canvas.draw_text(tx + theme.pt(30.0), ty + theme.pt(5.0), lbl, 0xFFFFFFFF, theme.font_caption());
        }

        let cy = y + tb_h + pad;
        let cw = w - pad * 2;
        let ch = (h - tb_h - pad * 2 - theme.pt(26.0)).max(80);

        match self.tab {
            SettingsTab::Appearance => {
                canvas.draw_rect(x + pad, cy, cw, ch, theme.bg_panel, 8);
                canvas.draw_rect_outline(x + pad, cy, cw, ch, theme.border_window, 8);

                // 1. Theme Mode (Dark / Light)
                canvas.draw_text(x + pad + theme.pt(14.0), cy + theme.pt(12.0), "System Theme Mode (Global):", theme.text_primary, theme.font_body());
                let m_y = cy + theme.pt(38.0);
                let btn_w = theme.pt(130.0);
                let btn_h = theme.pt(30.0);
                canvas.draw_rect(x + pad + theme.pt(14.0), m_y, btn_w, btn_h, if self.dark_mode { theme.accent } else { 0xFF334155 }, 6);
                canvas.draw_text(x + pad + theme.pt(36.0), m_y + theme.pt(7.0), "Dark Mode", 0xFFFFFFFF, theme.font_caption());

                canvas.draw_rect(x + pad + theme.pt(154.0), m_y, btn_w, btn_h, if !self.dark_mode { theme.accent } else { 0xFF334155 }, 6);
                canvas.draw_text(x + pad + theme.pt(176.0), m_y + theme.pt(7.0), "Light Mode", 0xFFFFFFFF, theme.font_caption());

                // Theme Style Switcher: Aero Glass vs Clean Modern
                canvas.draw_rect(x + pad + theme.pt(310.0), m_y, btn_w, btn_h, if self.style == ThemeStyle::AeroGlass { theme.accent } else { 0xFF334155 }, 6);
                canvas.draw_text(x + pad + theme.pt(332.0), m_y + theme.pt(7.0), "Aero Glass", 0xFFFFFFFF, theme.font_caption());

                canvas.draw_rect(x + pad + theme.pt(450.0), m_y, btn_w, btn_h, if self.style == ThemeStyle::CleanModern { theme.accent } else { 0xFF334155 }, 6);
                canvas.draw_text(x + pad + theme.pt(468.0), m_y + theme.pt(7.0), "Clean Modern", 0xFFFFFFFF, theme.font_caption());

                // 2. Global Accent Color Swatches
                let ac_y = cy + theme.pt(86.0);
                canvas.draw_text(x + pad + theme.pt(14.0), ac_y, "System Accent Color (Applies to All Windows & Controls):", theme.text_primary, theme.font_body());
                let palette = [
                    ("Aero Blue", 0xFF0284C7u32),
                    ("Royal Violet", 0xFF7C3AED),
                    ("Crimson Rose", 0xFFE11D48),
                    ("Emerald", 0xFF059669),
                    ("Amber Gold", 0xFFD97706),
                    ("Cyan Sky", 0xFF0891B2),
                    ("Indigo", 0xFF4F46E5),
                    ("Slate", 0xFF475569),
                ];
                let sw_sz = theme.pt(34.0);
                for (i, (_, col)) in palette.iter().enumerate() {
                    let sx = x + pad + theme.pt(14.0) + (i as i32 * (sw_sz + theme.pt(10.0)));
                    let sy = ac_y + theme.pt(28.0);
                    canvas.draw_rect(sx, sy, sw_sz, sw_sz, *col, (sw_sz / 2) as usize);
                    if self.accent == *col {
                        canvas.draw_rect_outline(sx - 3, sy - 3, sw_sz + 6, sw_sz + 6, 0xFFFFFFFF, ((sw_sz + 6) / 2) as usize);
                    }
                }

                // 3. UI Scaling (2K Display)
                let sc_y = ac_y + theme.pt(82.0);
                canvas.draw_text(x + pad + theme.pt(14.0), sc_y, &format!("Display UI Scale: {:.1}x (2560x1440 Native)", self.scale), theme.text_primary, theme.font_body());
                let scales = [1.5f32, 2.0, 2.5];
                for (i, &sc) in scales.iter().enumerate() {
                    let bx = x + pad + theme.pt(14.0) + (i as i32 * theme.pt(96.0));
                    let by = sc_y + theme.pt(26.0);
                    let active = (self.scale - sc).abs() < 0.05;
                    canvas.draw_rect(bx, by, theme.pt(86.0), theme.pt(28.0), if active { theme.accent } else { 0xFF334155 }, 5);
                    canvas.draw_text(bx + theme.pt(26.0), by + theme.pt(6.0), &format!("{:.1}x", sc), 0xFFFFFFFF, theme.font_caption());
                }

                // 4. Live Multi-Font Selector (Milestone 09)
                let fn_y = sc_y + theme.pt(70.0);
                canvas.draw_text(x + pad + theme.pt(14.0), fn_y, "System Font Families (Click to Cycle Available Fonts):", theme.text_primary, theme.font_body());
                let fbtn_y = fn_y + theme.pt(26.0);
                let fbtn_w = theme.pt(240.0);
                let fbtn_h = theme.pt(28.0);
                canvas.draw_rect(x + pad + theme.pt(14.0), fbtn_y, fbtn_w, fbtn_h, theme.bg_sidebar, 5);
                canvas.draw_rect_outline(x + pad + theme.pt(14.0), fbtn_y, fbtn_w, fbtn_h, theme.accent, 5);
                canvas.draw_text_clipped(x + pad + theme.pt(22.0), fbtn_y + theme.pt(6.0), fbtn_w - theme.pt(16.0), &format!("UI Font: {}", self.ui_font), theme.text_primary, theme.font_caption());

                canvas.draw_rect(x + pad + theme.pt(266.0), fbtn_y, fbtn_w, fbtn_h, theme.bg_sidebar, 5);
                canvas.draw_rect_outline(x + pad + theme.pt(266.0), fbtn_y, fbtn_w, fbtn_h, theme.accent, 5);
                canvas.draw_text_clipped(x + pad + theme.pt(274.0), fbtn_y + theme.pt(6.0), fbtn_w - theme.pt(16.0), &format!("Mono Font: {}", self.mono_font), theme.text_primary, theme.font_caption());
            }
            SettingsTab::Wallpapers => {
                canvas.draw_rect(x + pad, cy, cw, ch, theme.bg_panel, 8);
                canvas.draw_rect_outline(x + pad, cy, cw, ch, theme.border_window, 8);

                // Default Gradient Option
                let def_y = cy + theme.pt(10.0);
                let def_active = self.active_wallpaper == "none";
                canvas.draw_rect(x + pad + theme.pt(12.0), def_y, cw - theme.pt(24.0), theme.pt(32.0), if def_active { theme.accent } else { 0xFF1E293B }, 6);
                canvas.draw_text(x + pad + theme.pt(24.0), def_y + theme.pt(8.0), "Default EOS Harmony Blue Wallpaper", 0xFFFFFFFF, theme.font_caption());

                // Visual Image Cards (Rendered via Framework ImageWidget)
                let card_h = theme.pt(76.0);
                let thumb_w = theme.pt(120.0);
                let thumb_h = theme.pt(64.0);
                let list_top = def_y + theme.pt(40.0);
                canvas.push_clip(Rect::new(x + pad + 4, list_top, cw - 8, (cy + ch - list_top - 4).max(20)));

                if self.wallpapers.is_empty() {
                    canvas.draw_text(x + pad + theme.pt(16.0), list_top + theme.pt(12.0), "No PNG/JPG wallpapers found in /EOS SHARE.", theme.text_secondary, theme.font_caption());
                } else {
                    for (i, item) in self.wallpapers.iter().enumerate() {
                        let ry = list_top + (i as i32 * (card_h + theme.pt(8.0))) - self.wp_scroll;
                        if ry + card_h < list_top || ry > cy + ch { continue; }
                        let is_sel = self.active_wallpaper == item.name;
                        let card_bg = if is_sel { 0xFF1D4ED8 } else { 0xFF0F172A };
                        canvas.draw_rect(x + pad + theme.pt(12.0), ry, cw - theme.pt(24.0), card_h, card_bg, 6);
                        canvas.draw_rect_outline(x + pad + theme.pt(12.0), ry, cw - theme.pt(24.0), card_h, if is_sel { 0xFFFFFFFF } else { theme.border_window }, 6);

                        // Blit live aspect-preserved thumbnail directly without cloning pixel buffers
                        let tx = x + pad + theme.pt(18.0);
                        let ty = ry + (card_h - thumb_h) / 2;
                        let iw = item.thumb.img_w as f32;
                        let ih = item.thumb.img_h as f32;
                        let scale = (thumb_w as f32 / iw).max(thumb_h as f32 / ih);
                        let crop_w = (thumb_w as f32 / scale).min(iw);
                        let crop_h = (thumb_h as f32 / scale).min(ih);
                        let sx0 = (iw - crop_w) * 0.5;
                        let sy0 = (ih - crop_h) * 0.5;
                        for py in 0..thumb_h {
                            let sy = (sy0 + (py as f32 / thumb_h as f32) * crop_h) as usize;
                            let srow = sy.min(item.thumb.img_h - 1) * item.thumb.img_w;
                            let drow = ((ty + py) as usize) * canvas.width;
                            if ty + py < 0 || ty + py >= canvas.height as i32 { continue; }
                            for px in 0..thumb_w {
                                if tx + px < 0 || tx + px >= canvas.width as i32 { continue; }
                                let sx = (sx0 + (px as f32 / thumb_w as f32) * crop_w) as usize;
                                canvas.buffer[drow + (tx + px) as usize] = item.thumb.pixels[srow + sx.min(item.thumb.img_w - 1)];
                            }
                        }
                        canvas.draw_rect_outline(tx, ty, thumb_w, thumb_h, 0x88FFFFFF, 2);

                        // Filename & Resolution
                        let info_x = tx + thumb_w + theme.pt(14.0);
                        canvas.draw_text_clipped(info_x, ry + theme.pt(14.0), cw - thumb_w - theme.pt(60.0), &item.name, 0xFFFFFFFF, theme.font_body());
                        canvas.draw_text(info_x, ry + theme.pt(38.0), &format!("Resolution: {}x{} px  |  Click to Apply", item.src_w, item.src_h), 0xFFCBD5E1, theme.font_caption());
                    }
                }
                canvas.pop_clip();
            }
            SettingsTab::SystemHw => {
                canvas.draw_rect(x + pad, cy, cw, ch, theme.bg_panel, 8);
                canvas.draw_rect_outline(x + pad, cy, cw, ch, theme.border_window, 8);
                let gpu = sdk::gpu::query_info();
                let aud = if sdk::audio::is_available() { "Intel AC'97 Hardware Audio (48kHz Stereo ACTIVE)" } else { "OFF" };
                let heap_mb = sdk::agent::query_kernel_heap_used() as f32 / (1024.0 * 1024.0);

                canvas.draw_text(x + pad + theme.pt(14.0), cy + theme.pt(14.0), "EOS Hardware & Kernel Configuration", 0xFF38BDF8, theme.font_title());
                canvas.draw_text(x + pad + theme.pt(14.0), cy + theme.pt(44.0), &format!("Graphics Adapter : {} ({}x{})", gpu.backend_name(), gpu.width, gpu.height), theme.text_primary, theme.font_body());
                canvas.draw_text(x + pad + theme.pt(14.0), cy + theme.pt(68.0), &format!("Audio Subsystem  : {}", aud), theme.text_primary, theme.font_body());
                canvas.draw_text(x + pad + theme.pt(14.0), cy + theme.pt(92.0), &format!("Kernel Heap Used : {:.1} MB  |  Architecture: Multi-Process Isolated ELF", heap_mb), theme.text_secondary, theme.font_caption());
            }
        }

        // Bottom Status Bar
        canvas.draw_text(x + pad, y + h - theme.pt(22.0), &self.status_msg, theme.text_secondary, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if !pressed { return true; }

        let theme = get_theme();
        let pad = theme.pt(12.0);
        let tb_h = theme.pt(36.0);
        let tab_w = theme.pt(160.0);

        if my >= self.bounds.y && my <= self.bounds.y + tb_h {
            for i in 0..3 {
                let tx = self.bounds.x + pad + (i as i32 * (tab_w + theme.pt(6.0)));
                if mx >= tx && mx <= tx + tab_w {
                    self.tab = match i {
                        0 => SettingsTab::Appearance,
                        1 => {
                            if !self.wallpapers_loaded {
                                self.wallpapers_loaded = true;
                                self.scan_wallpapers();
                            }
                            SettingsTab::Wallpapers
                        }
                        _ => SettingsTab::SystemHw,
                    };
                    return true;
                }
            }
            return true;
        }

        let cy = self.bounds.y + tb_h + pad;
        let cw = self.bounds.w - pad * 2;

        match self.tab {
            SettingsTab::Appearance => {
                let m_y = cy + theme.pt(38.0);
                let btn_w = theme.pt(130.0);
                let btn_h = theme.pt(30.0);
                if my >= m_y && my <= m_y + btn_h {
                    if mx >= self.bounds.x + pad + theme.pt(14.0) && mx <= self.bounds.x + pad + theme.pt(14.0) + btn_w {
                        self.dark_mode = true;
                        self.persist_and_apply();
                        self.status_msg = "Applied Dark Mode across all windows.".into();
                        return true;
                    }
                    if mx >= self.bounds.x + pad + theme.pt(154.0) && mx <= self.bounds.x + pad + theme.pt(154.0) + btn_w {
                        self.dark_mode = false;
                        self.persist_and_apply();
                        self.status_msg = "Applied Light Mode across all windows.".into();
                        return true;
                    }
                    if mx >= self.bounds.x + pad + theme.pt(310.0) && mx <= self.bounds.x + pad + theme.pt(310.0) + btn_w {
                        self.style = ThemeStyle::AeroGlass;
                        self.persist_and_apply();
                        self.status_msg = "Switched to Aero Glass Theme.".into();
                        return true;
                    }
                    if mx >= self.bounds.x + pad + theme.pt(450.0) && mx <= self.bounds.x + pad + theme.pt(450.0) + btn_w {
                        self.style = ThemeStyle::CleanModern;
                        self.persist_and_apply();
                        self.status_msg = "Switched to Clean Modern Theme.".into();
                        return true;
                    }
                }

                let ac_y = cy + theme.pt(86.0);
                let palette = [0xFF0284C7u32, 0xFF7C3AED, 0xFFE11D48, 0xFF059669, 0xFFD97706, 0xFF0891B2, 0xFF4F46E5, 0xFF475569];
                let sw_sz = theme.pt(34.0);
                let sy = ac_y + theme.pt(28.0);
                if my >= sy && my <= sy + sw_sz {
                    for (i, &col) in palette.iter().enumerate() {
                        let sx = self.bounds.x + pad + theme.pt(14.0) + (i as i32 * (sw_sz + theme.pt(10.0)));
                        if mx >= sx && mx <= sx + sw_sz {
                            self.accent = col;
                            self.persist_and_apply();
                            self.status_msg = format!("Applied system accent color {:#010x}.", col);
                            return true;
                        }
                    }
                }

                let sc_y = ac_y + theme.pt(82.0);
                let by = sc_y + theme.pt(26.0);
                let scales = [1.5f32, 2.0, 2.5];
                if my >= by && my <= by + theme.pt(28.0) {
                    for (i, &sc) in scales.iter().enumerate() {
                        let bx = self.bounds.x + pad + theme.pt(14.0) + (i as i32 * theme.pt(96.0));
                        if mx >= bx && mx <= bx + theme.pt(86.0) {
                            self.scale = sc;
                            self.persist_and_apply();
                            self.status_msg = format!("Updated UI Scale to {:.1}x.", sc);
                            return true;
                        }
                    }
                }

                let fn_y = sc_y + theme.pt(70.0);
                let fbtn_y = fn_y + theme.pt(26.0);
                let fbtn_w = theme.pt(240.0);
                let fbtn_h = theme.pt(28.0);
                if my >= fbtn_y && my <= fbtn_y + fbtn_h {
                    let fonts = crate::framework::fonts::list_available_fonts();
                    if !fonts.is_empty() {
                        if mx >= self.bounds.x + pad + theme.pt(14.0) && mx <= self.bounds.x + pad + theme.pt(14.0) + fbtn_w {
                            let idx = fonts.iter().position(|f| f == &self.ui_font).unwrap_or(0);
                            self.ui_font = fonts[(idx + 1) % fonts.len()].clone();
                            self.persist_and_apply();
                            self.status_msg = format!("Switched UI Font to '{}'.", self.ui_font);
                            return true;
                        }
                        if mx >= self.bounds.x + pad + theme.pt(266.0) && mx <= self.bounds.x + pad + theme.pt(266.0) + fbtn_w {
                            let idx = fonts.iter().position(|f| f == &self.mono_font).unwrap_or(0);
                            self.mono_font = fonts[(idx + 1) % fonts.len()].clone();
                            self.persist_and_apply();
                            self.status_msg = format!("Switched Mono Font to '{}'.", self.mono_font);
                            return true;
                        }
                    }
                }
            }
            SettingsTab::Wallpapers => {
                let def_y = cy + theme.pt(10.0);
                if my >= def_y && my <= def_y + theme.pt(32.0) {
                    self.active_wallpaper = "none".into();
                    self.persist_and_apply();
                    self.status_msg = "Applied Default EOS Harmony Wallpaper.".into();
                    return true;
                }

                let card_h = theme.pt(76.0);
                let list_top = def_y + theme.pt(40.0);
                if my >= list_top && mx >= self.bounds.x + pad + theme.pt(12.0) && mx <= self.bounds.x + pad + cw - theme.pt(12.0) {
                    let rel = my - list_top + self.wp_scroll;
                    if rel >= 0 {
                        let idx = (rel / (card_h + theme.pt(8.0))) as usize;
                        if idx < self.wallpapers.len() {
                            self.active_wallpaper = self.wallpapers[idx].name.clone();
                            self.persist_and_apply();
                            self.status_msg = format!("Applied Wallpaper: {}", self.active_wallpaper);
                            return true;
                        }
                    }
                }
            }
            SettingsTab::SystemHw => {}
        }
        true
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) || self.tab != SettingsTab::Wallpapers { return false; }
        let theme = get_theme();
        let card_h = theme.pt(76.0) + theme.pt(8.0);
        let max_s = ((self.wallpapers.len() as i32) * card_h - theme.pt(240.0)).max(0);
        if dy > 0 {
            self.wp_scroll = (self.wp_scroll + card_h).min(max_s);
        } else if dy < 0 {
            self.wp_scroll = (self.wp_scroll - card_h).max(0);
        }
        true
    }
}
