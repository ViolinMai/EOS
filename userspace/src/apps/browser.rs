use crate::framework::*;
use crate::net_manager::{NET, poll_network, TcpState};

#[derive(Clone, Debug)]
pub struct SearchResultItem {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

pub struct BrowserApp {
    bounds: Rect,
    url_input: String,
    input_active: bool,
    results: Vec<SearchResultItem>,
    status_text: String,
    scroll_y: usize,
    active_sock: Option<u16>,
    loading: bool,
}

impl BrowserApp {
    pub fn new() -> Self {
        let mut app = Self {
            bounds: Rect::default(),
            url_input: "google: rust programming".to_string(),
            input_active: false,
            results: Vec::new(),
            status_text: "Ready. Enter a search query and press Enter.".to_string(),
            scroll_y: 0,
            active_sock: None,
            loading: false,
        };
        app.results.push(SearchResultItem {
            title: "Google Search on EOS".to_string(),
            snippet: "Experimental lightweight web browser running on Ring 3 Userspace.".to_string(),
            url: "http://www.google.com".to_string(),
        });
        app
    }

    fn perform_search(&mut self) {
        let query = self.url_input.trim().to_string();
        if query.is_empty() { return; }

        self.loading = true;
        self.scroll_y = 0;
        self.results.clear();
        self.status_text = format!("Searching for '{}' via network gateway...", query);

        let clean_q = query.strip_prefix("google:").unwrap_or(&query).trim();
        let path = format!("/search?q={}", clean_q.replace(' ', "+"));

        let target_ip = [10, 0, 2, 2];
        let host = "10.0.2.2";

        let sock_port = {
            let mut lock = NET.lock().unwrap();
            if let Some(net) = lock.as_mut() {
                net.http_get(target_ip, host, &path)
            } else {
                0
            }
        };

        if sock_port != 0 {
            self.active_sock = Some(sock_port);
        } else {
            self.loading = false;
            self.status_text = "Network subsystem unavailable.".to_string();
        }
    }

    fn parse_html_results(&mut self, raw_html: &str) {
        self.results.clear();
        let mut in_title = false;
        let mut cur_title = String::new();

        for line in raw_html.lines() {
            let trimmed = line.trim();
            if trimmed.contains("<h3>") || trimmed.contains("<title>") {
                in_title = true;
                cur_title = trimmed.replace("<h3>", "").replace("</h3>", "")
                                   .replace("<title>", "").replace("</title>", "");
            } else if in_title && (trimmed.contains("<p>") || trimmed.contains("<span>")) {
                let cur_snippet = trimmed.replace("<p>", "").replace("</p>", "")
                                         .replace("<span>", "").replace("</span>", "");
                self.results.push(SearchResultItem {
                    title: cur_title.clone(),
                    snippet: cur_snippet,
                    url: format!("http://google.com/search?q={}", cur_title.replace(' ', "+")),
                });
                in_title = false;
            }
        }

        if self.results.is_empty() {
            let non_empty: Vec<&str> = raw_html.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
            for chunk in non_empty.chunks(2) {
                let title = chunk[0].to_string();
                let snippet = if chunk.len() > 1 { chunk[1].to_string() } else { "No description available.".to_string() };
                self.results.push(SearchResultItem {
                    title,
                    snippet,
                    url: "http://google.com".to_string(),
                });
            }
        }

        if self.results.is_empty() {
            self.results.push(SearchResultItem {
                title: "No Results Found".to_string(),
                snippet: "Check network connection or try a different search keyword.".to_string(),
                url: "".to_string(),
            });
        }
    }

    fn check_network_response(&mut self) {
        if !self.loading { return; }
        if let Some(port) = self.active_sock {
            poll_network();
            let mut lock = NET.lock().unwrap();
            if let Some(net) = lock.as_mut() {
                if let Some(sock) = net.tcp_sockets.get_mut(&port) {
                    if !sock.rx_buffer.is_empty() {
                        let content = String::from_utf8_lossy(&sock.rx_buffer).to_string();
                        drop(lock);
                        self.parse_html_results(&content);
                        self.loading = false;
                        self.status_text = format!("Search completed. Found {} items.", self.results.len());
                        return;
                    }
                    if sock.state == TcpState::Closed {
                        self.loading = false;
                        self.status_text = "Connection closed by peer.".to_string();
                    }
                }
            }
        }
    }
}

impl Widget for BrowserApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: usize, y: usize, w: usize, h: usize) -> Rect {
        self.bounds = Rect { x, y, w, h };
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        let _tb_h = theme.pt(46.0);
        let status_h = theme.pt(26.0);

        unsafe {
            let app_mut = self as *const Self as *mut Self;
            (*app_mut).check_network_response();
        }

        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFFFFFFFF, 0);

        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, _tb_h, 0xFFF1F5F9, 0);
        canvas.draw_line_h(self.bounds.x, self.bounds.y + _tb_h, self.bounds.w, 0xFFCBD5E1);

        canvas.draw_text(self.bounds.x + theme.pt(16.0), self.bounds.y + theme.pt(14.0), "G", 0xFF4285F4, theme.font_title());
        canvas.draw_text(self.bounds.x + theme.pt(28.0), self.bounds.y + theme.pt(14.0), "o", 0xFFEA4335, theme.font_title());
        canvas.draw_text(self.bounds.x + theme.pt(38.0), self.bounds.y + theme.pt(14.0), "o", 0xFFFBBC05, theme.font_title());
        canvas.draw_text(self.bounds.x + theme.pt(48.0), self.bounds.y + theme.pt(14.0), "g", 0xFF4285F4, theme.font_title());
        canvas.draw_text(self.bounds.x + theme.pt(58.0), self.bounds.y + theme.pt(14.0), "l", 0xFF34A853, theme.font_title());
        canvas.draw_text(self.bounds.x + theme.pt(64.0), self.bounds.y + theme.pt(14.0), "e", 0xFFEA4335, theme.font_title());

        let bar_x = self.bounds.x + theme.pt(84.0);
        let btn_w = theme.pt(70.0);
        let bar_w = self.bounds.w.saturating_sub(theme.pt(170.0));
        let bar_h = theme.pt(32.0);
        let bar_y = self.bounds.y + theme.pt(7.0);

        canvas.draw_rect(bar_x, bar_y, bar_w, bar_h, 0xFFFFFFFF, theme.pt(16.0));
        let border_col = if self.input_active { 0xFF4285F4 } else { 0xFFCBD5E1 };
        canvas.draw_rect_outline(bar_x, bar_y, bar_w, bar_h, border_col, theme.pt(16.0));

        let display_input = if self.url_input.is_empty() && !self.input_active {
            "Search Google or type a URL..."
        } else {
            &self.url_input
        };
        let text_col = if self.url_input.is_empty() && !self.input_active { 0xFF94A3B8 } else { 0xFF0F172A };
        canvas.draw_text_clipped(bar_x + theme.pt(14.0), bar_y + theme.pt(7.0), bar_w.saturating_sub(theme.pt(28.0)), display_input, text_col, theme.font_body());

        let btn_x = bar_x + bar_w + theme.pt(10.0);
        canvas.draw_rect(btn_x, bar_y, btn_w, bar_h, 0xFF4285F4, theme.pt(16.0));
        canvas.draw_text(btn_x + theme.pt(14.0), bar_y + theme.pt(7.0), "Search", 0xFFFFFFFF, theme.font_body());

        let content_y = self.bounds.y + _tb_h + 1;
        let content_h = self.bounds.h.saturating_sub(_tb_h + status_h + 1);
        let mut cur_y = content_y + theme.pt(16.0).saturating_sub(self.scroll_y);

        if self.loading {
            canvas.draw_text(self.bounds.x + theme.pt(32.0), content_y + theme.pt(30.0), "🔍 Sending HTTP request & querying Google search index...", 0xFF4285F4, theme.font_title());
        } else {
            for (idx, res) in self.results.iter().enumerate() {
                if cur_y + theme.pt(60.0) > content_y && cur_y < content_y + content_h {
                    if !res.url.is_empty() {
                        canvas.draw_text_clipped(self.bounds.x + theme.pt(32.0), cur_y, self.bounds.w.saturating_sub(theme.pt(64.0)), &res.url, 0xFF202124, theme.font_caption());
                    }
                    canvas.draw_text_clipped(self.bounds.x + theme.pt(32.0), cur_y + theme.pt(16.0), self.bounds.w.saturating_sub(theme.pt(64.0)), &format!("{}. {}", idx + 1, res.title), 0xFF1A0DAB, theme.font_title());
                    canvas.draw_text_clipped(self.bounds.x + theme.pt(32.0), cur_y + theme.pt(40.0), self.bounds.w.saturating_sub(theme.pt(64.0)), &res.snippet, 0xFF4D5156, theme.font_body());
                }
                cur_y += theme.pt(70.0);
            }
        }

        let sb_y = self.bounds.y + self.bounds.h - status_h;
        canvas.draw_rect(self.bounds.x, sb_y, self.bounds.w, status_h, 0xFFF8FAFC, 0);
        canvas.draw_line_h(self.bounds.x, sb_y, self.bounds.w, 0xFFE2E8F0);
        canvas.draw_text(self.bounds.x + theme.pt(16.0), sb_y + theme.pt(5.0), &self.status_text, 0xFF64748B, theme.font_caption());
    }

    fn handle_mouse(&mut self, mx: usize, my: usize, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) { return false; }
        if !pressed { return true; }

        let theme = get_theme();
        let bar_x = self.bounds.x + theme.pt(84.0);
        let btn_w = theme.pt(70.0);
        let bar_w = self.bounds.w.saturating_sub(theme.pt(170.0));
        let bar_h = theme.pt(32.0);
        let bar_y = self.bounds.y + theme.pt(7.0);
        let btn_x = bar_x + bar_w + theme.pt(10.0);

        if mx >= bar_x && mx <= bar_x + bar_w && my >= bar_y && my <= bar_y + bar_h {
            self.input_active = true;
            return true;
        }

        if mx >= btn_x && mx <= btn_x + btn_w && my >= bar_y && my <= bar_y + bar_h {
            self.input_active = false;
            self.perform_search();
            return true;
        }

        self.input_active = false;
        true
    }

    fn handle_char(&mut self, c: char) -> bool {
        if !self.input_active { return false; }
        if c == '\n' {
            self.input_active = false;
            self.perform_search();
            return true;
        } else if c == '\x08' {
            self.url_input.pop();
            return true;
        } else if c >= ' ' && c <= '~' {
            self.url_input.push(c);
            return true;
        }
        false
    }
}
