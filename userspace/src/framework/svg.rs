#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, Debug)]
struct Transform2D {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Transform2D {
    fn identity() -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
    }

    fn multiply(&self, o: &Self) -> Self {
        Self {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }

    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }
}

struct SvgShape {
    subpaths: Vec<Vec<(f32, f32)>>,
    color: u32, // 0xAARRGGBB
    fill_rule: FillRule,
}

struct NumLexer<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> NumLexer<'a> {
    fn new(s: &'a str) -> Self {
        Self { bytes: s.as_bytes(), pos: 0 }
    }

    fn skip_sep(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b',' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek_is_number(&mut self) -> bool {
        self.skip_sep();
        if self.pos >= self.bytes.len() { return false; }
        let b = self.bytes[self.pos];
        b.is_ascii_digit() || b == b'-' || b == b'+' || b == b'.'
    }

    fn next_f32(&mut self) -> Option<f32> {
        self.skip_sep();
        if self.pos >= self.bytes.len() { return None; }
        let start = self.pos;
        if self.bytes[self.pos] == b'-' || self.bytes[self.pos] == b'+' {
            self.pos += 1;
        }
        let mut seen_dot = false;
        let mut seen_digit = false;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_digit() {
                seen_digit = true;
                self.pos += 1;
            } else if b == b'.' && !seen_dot {
                seen_dot = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        if !seen_digit {
            self.pos = start;
            return None;
        }
        if self.pos < self.bytes.len() && (self.bytes[self.pos] == b'e' || self.bytes[self.pos] == b'E') {
            let exp_start = self.pos;
            self.pos += 1;
            if self.pos < self.bytes.len() && (self.bytes[self.pos] == b'-' || self.bytes[self.pos] == b'+') {
                self.pos += 1;
            }
            let mut exp_digits = false;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                exp_digits = true;
                self.pos += 1;
            }
            if !exp_digits {
                self.pos = exp_start;
            }
        }
        core::str::from_utf8(&self.bytes[start..self.pos]).ok()?.parse::<f32>().ok()
    }

    fn next_flag(&mut self) -> Option<bool> {
        self.skip_sep();
        if self.pos >= self.bytes.len() { return None; }
        let b = self.bytes[self.pos];
        if b == b'0' {
            self.pos += 1;
            Some(false)
        } else if b == b'1' {
            self.pos += 1;
            Some(true)
        } else {
            None
        }
    }
}

fn parse_hex_color(raw: &str) -> Option<u32> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                let a = u8::from_str_radix(&hex[3..4], 16).ok()? * 17;
                Some(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
            }
            6 => {
                let rgb = u32::from_str_radix(hex, 16).ok()?;
                Some(0xFF000000 | rgb)
            }
            8 => {
                let rgba = u32::from_str_radix(hex, 16).ok()?;
                let r = (rgba >> 24) & 0xFF;
                let g = (rgba >> 16) & 0xFF;
                let b = (rgba >> 8) & 0xFF;
                let a = rgba & 0xFF;
                Some((a << 24) | (r << 16) | (g << 8) | b)
            }
            _ => None,
        }
    } else if s.eq_ignore_ascii_case("white") {
        Some(0xFFFFFFFF)
    } else if s.eq_ignore_ascii_case("black") {
        Some(0xFF000000)
    } else if let Some(inner) = s.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')')) {
        let mut lex = NumLexer::new(inner);
        let r = lex.next_f32()?.clamp(0.0, 255.0) as u32;
        let g = lex.next_f32()?.clamp(0.0, 255.0) as u32;
        let b = lex.next_f32()?.clamp(0.0, 255.0) as u32;
        Some(0xFF000000 | (r << 16) | (g << 8) | b)
    } else {
        None
    }
}

fn parse_transform(attr: &str) -> Transform2D {
    let mut t = Transform2D::identity();
    let mut rest = attr.trim();
    while !rest.is_empty() {
        if let Some(open) = rest.find('(') {
            if let Some(close) = rest[open..].find(')') {
                let name = rest[..open].trim();
                let args = &rest[open + 1..open + close];
                let mut lex = NumLexer::new(args);
                if name.ends_with("translate") {
                    let tx = lex.next_f32().unwrap_or(0.0);
                    let ty = lex.next_f32().unwrap_or(0.0);
                    t = t.multiply(&Transform2D { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: tx, f: ty });
                } else if name.ends_with("scale") {
                    let sx = lex.next_f32().unwrap_or(1.0);
                    let sy = lex.next_f32().unwrap_or(sx);
                    t = t.multiply(&Transform2D { a: sx, b: 0.0, c: 0.0, d: sy, e: 0.0, f: 0.0 });
                } else if name.ends_with("matrix") {
                    let a = lex.next_f32().unwrap_or(1.0);
                    let b = lex.next_f32().unwrap_or(0.0);
                    let c = lex.next_f32().unwrap_or(0.0);
                    let d = lex.next_f32().unwrap_or(1.0);
                    let e = lex.next_f32().unwrap_or(0.0);
                    let f = lex.next_f32().unwrap_or(0.0);
                    t = t.multiply(&Transform2D { a, b, c, d, e, f });
                }
                rest = rest[open + close + 1..].trim();
                continue;
            }
        }
        break;
    }
    t
}

fn flatten_cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), out: &mut Vec<(f32, f32)>) {
    let steps = 12;
    for i in 1..=steps {
        let t = (i as f32) / (steps as f32);
        let u = 1.0 - t;
        let tt = t * t;
        let uu = u * u;
        let uuu = uu * u;
        let ttt = tt * t;
        let x = uuu * p0.0 + 3.0 * uu * t * p1.0 + 3.0 * u * tt * p2.0 + ttt * p3.0;
        let y = uuu * p0.1 + 3.0 * uu * t * p1.1 + 3.0 * u * tt * p2.1 + ttt * p3.1;
        out.push((x, y));
    }
}

fn flatten_quad(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), out: &mut Vec<(f32, f32)>) {
    let steps = 10;
    for i in 1..=steps {
        let t = (i as f32) / (steps as f32);
        let u = 1.0 - t;
        let x = u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0;
        let y = u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1;
        out.push((x, y));
    }
}

fn flatten_arc(
    p0: (f32, f32),
    mut rx: f32,
    mut ry: f32,
    x_rot_deg: f32,
    large_arc: bool,
    sweep: bool,
    p1: (f32, f32),
    out: &mut Vec<(f32, f32)>,
) {
    if (p0.0 - p1.0).abs() < 1e-4 && (p0.1 - p1.1).abs() < 1e-4 {
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    if rx < 1e-4 || ry < 1e-4 {
        out.push(p1);
        return;
    }

    let phi = x_rot_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();

    let dx = (p0.0 - p1.0) * 0.5;
    let dy = (p0.1 - p1.1) * 0.5;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }

    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let num = (rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2).max(0.0);
    let den = (rx2 * y1p2 + ry2 * x1p2).max(1e-6);
    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let coef = sign * (num / den).sqrt();

    let cxp = coef * (rx * y1p / ry);
    let cyp = coef * (-ry * x1p / rx);

    let cx = cos_phi * cxp - sin_phi * cyp + (p0.0 + p1.0) * 0.5;
    let cy = sin_phi * cxp + cos_phi * cyp + (p0.1 + p1.1) * 0.5;

    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;

    let theta1 = uy.atan2(ux);
    let dot = (ux * vx + uy * vy).clamp(-1.0, 1.0);
    let cross = ux * vy - uy * vx;
    let mut dtheta = cross.atan2(dot);
    if !sweep && dtheta > 0.0 {
        dtheta -= core::f32::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += core::f32::consts::TAU;
    }

    let steps = ((dtheta.abs() / (core::f32::consts::PI / 8.0)).ceil() as usize).max(4);
    for i in 1..=steps {
        let ang = theta1 + dtheta * (i as f32 / steps as f32);
        let (sa, ca) = ang.sin_cos();
        let ex = rx * ca;
        let ey = ry * sa;
        let x = cos_phi * ex - sin_phi * ey + cx;
        let y = sin_phi * ex + cos_phi * ey + cy;
        out.push((x, y));
    }
}

fn parse_path_d(d: &str) -> Vec<Vec<(f32, f32)>> {
    let mut subpaths: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur_path: Vec<(f32, f32)> = Vec::new();
    let mut cur = (0.0f32, 0.0f32);
    let mut start_pt = (0.0f32, 0.0f32);
    let mut last_ctrl = (0.0f32, 0.0f32);
    let mut last_cmd = b' ';

    let mut lex = NumLexer::new(d);
    while lex.pos < lex.bytes.len() {
        lex.skip_sep();
        if lex.pos >= lex.bytes.len() { break; }
        let b = lex.bytes[lex.pos];
        let cmd = if b.is_ascii_alphabetic() {
            lex.pos += 1;
            b
        } else if last_cmd != b' ' {
            if last_cmd == b'M' { b'L' } else if last_cmd == b'm' { b'l' } else { last_cmd }
        } else {
            lex.pos += 1;
            continue;
        };

        match cmd {
            b'M' | b'm' => {
                if cur_path.len() >= 3 {
                    subpaths.push(core::mem::take(&mut cur_path));
                } else {
                    cur_path.clear();
                }
                if let (Some(x), Some(y)) = (lex.next_f32(), lex.next_f32()) {
                    cur = if cmd == b'm' { (cur.0 + x, cur.1 + y) } else { (x, y) };
                    start_pt = cur;
                    last_ctrl = cur;
                    cur_path.push(cur);
                }
            }
            b'L' | b'l' => {
                while lex.peek_is_number() {
                    if let (Some(x), Some(y)) = (lex.next_f32(), lex.next_f32()) {
                        cur = if cmd == b'l' { (cur.0 + x, cur.1 + y) } else { (x, y) };
                        last_ctrl = cur;
                        cur_path.push(cur);
                    } else {
                        break;
                    }
                }
            }
            b'H' | b'h' => {
                while lex.peek_is_number() {
                    if let Some(x) = lex.next_f32() {
                        cur.0 = if cmd == b'h' { cur.0 + x } else { x };
                        last_ctrl = cur;
                        cur_path.push(cur);
                    } else {
                        break;
                    }
                }
            }
            b'V' | b'v' => {
                while lex.peek_is_number() {
                    if let Some(y) = lex.next_f32() {
                        cur.1 = if cmd == b'v' { cur.1 + y } else { y };
                        last_ctrl = cur;
                        cur_path.push(cur);
                    } else {
                        break;
                    }
                }
            }
            b'C' | b'c' => {
                while lex.peek_is_number() {
                    if let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                        lex.next_f32(), lex.next_f32(), lex.next_f32(),
                        lex.next_f32(), lex.next_f32(), lex.next_f32(),
                    ) {
                        let (p1, p2, p3) = if cmd == b'c' {
                            ((cur.0 + x1, cur.1 + y1), (cur.0 + x2, cur.1 + y2), (cur.0 + x, cur.1 + y))
                        } else {
                            ((x1, y1), (x2, y2), (x, y))
                        };
                        flatten_cubic(cur, p1, p2, p3, &mut cur_path);
                        cur = p3;
                        last_ctrl = p2;
                    } else {
                        break;
                    }
                }
            }
            b'S' | b's' => {
                while lex.peek_is_number() {
                    if let (Some(x2), Some(y2), Some(x), Some(y)) = (
                        lex.next_f32(), lex.next_f32(), lex.next_f32(), lex.next_f32(),
                    ) {
                        let p1 = if matches!(last_cmd, b'C' | b'c' | b'S' | b's') {
                            (2.0 * cur.0 - last_ctrl.0, 2.0 * cur.1 - last_ctrl.1)
                        } else {
                            cur
                        };
                        let (p2, p3) = if cmd == b's' {
                            ((cur.0 + x2, cur.1 + y2), (cur.0 + x, cur.1 + y))
                        } else {
                            ((x2, y2), (x, y))
                        };
                        flatten_cubic(cur, p1, p2, p3, &mut cur_path);
                        cur = p3;
                        last_ctrl = p2;
                    } else {
                        break;
                    }
                }
            }
            b'Q' | b'q' => {
                while lex.peek_is_number() {
                    if let (Some(x1), Some(y1), Some(x), Some(y)) = (
                        lex.next_f32(), lex.next_f32(), lex.next_f32(), lex.next_f32(),
                    ) {
                        let (p1, p2) = if cmd == b'q' {
                            ((cur.0 + x1, cur.1 + y1), (cur.0 + x, cur.1 + y))
                        } else {
                            ((x1, y1), (x, y))
                        };
                        flatten_quad(cur, p1, p2, &mut cur_path);
                        cur = p2;
                        last_ctrl = p1;
                    } else {
                        break;
                    }
                }
            }
            b'T' | b't' => {
                while lex.peek_is_number() {
                    if let (Some(x), Some(y)) = (lex.next_f32(), lex.next_f32()) {
                        let p1 = if matches!(last_cmd, b'Q' | b'q' | b'T' | b't') {
                            (2.0 * cur.0 - last_ctrl.0, 2.0 * cur.1 - last_ctrl.1)
                        } else {
                            cur
                        };
                        let p2 = if cmd == b't' { (cur.0 + x, cur.1 + y) } else { (x, y) };
                        flatten_quad(cur, p1, p2, &mut cur_path);
                        cur = p2;
                        last_ctrl = p1;
                    } else {
                        break;
                    }
                }
            }
            b'A' | b'a' => {
                while lex.peek_is_number() {
                    if let (Some(rx), Some(ry), Some(rot), Some(laf), Some(sf), Some(x), Some(y)) = (
                        lex.next_f32(), lex.next_f32(), lex.next_f32(),
                        lex.next_flag(), lex.next_flag(), lex.next_f32(), lex.next_f32(),
                    ) {
                        let p1 = if cmd == b'a' { (cur.0 + x, cur.1 + y) } else { (x, y) };
                        flatten_arc(cur, rx, ry, rot, laf, sf, p1, &mut cur_path);
                        cur = p1;
                        last_ctrl = cur;
                    } else {
                        break;
                    }
                }
            }
            b'Z' | b'z' => {
                cur = start_pt;
                last_ctrl = cur;
                if cur_path.len() >= 3 {
                    subpaths.push(core::mem::take(&mut cur_path));
                } else {
                    cur_path.clear();
                }
            }
            _ => {}
        }
        last_cmd = cmd;
    }

    if cur_path.len() >= 3 {
        subpaths.push(cur_path);
    }
    subpaths
}

fn get_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut pos = 0;
    let bytes = tag.as_bytes();
    while pos < bytes.len() {
        if let Some(found) = tag[pos..].find(name) {
            let idx = pos + found;
            let before_ok = idx == 0 || bytes[idx - 1].is_ascii_whitespace() || bytes[idx - 1] == b';' || bytes[idx - 1] == b'"';
            let after_idx = idx + name.len();
            if before_ok && after_idx < bytes.len() {
                let rest = tag[after_idx..].trim_start();
                if let Some(after_eq) = rest.strip_prefix('=') {
                    let val_part = after_eq.trim_start();
                    if let Some(q) = val_part.chars().next() {
                        if q == '"' || q == '\'' {
                            let inner = &val_part[1..];
                            if let Some(end_q) = inner.find(q) {
                                return Some(&inner[..end_q]);
                            }
                        }
                    }
                }
            }
            pos = idx + name.len();
        } else {
            break;
        }
    }
    None
}

fn get_style_prop<'a>(style: &'a str, prop: &str) -> Option<&'a str> {
    for part in style.split(';') {
        if let Some((k, v)) = part.split_once(':') {
            if k.trim().eq_ignore_ascii_case(prop) {
                return Some(v.trim());
            }
        }
    }
    None
}

fn build_circle_poly(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<(f32, f32)> {
    let steps = 28;
    let mut pts = Vec::with_capacity(steps);
    for i in 0..steps {
        let ang = (i as f32 / steps as f32) * core::f32::consts::TAU;
        let (sa, ca) = ang.sin_cos();
        pts.push((cx + rx * ca, cy + ry * sa));
    }
    pts
}

fn build_rect_poly(x: f32, y: f32, w: f32, h: f32, mut rx: f32, mut ry: f32) -> Vec<(f32, f32)> {
    rx = rx.min(w * 0.5).max(0.0);
    ry = ry.min(h * 0.5).max(0.0);
    if rx < 0.1 && ry < 0.1 {
        return vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    }
    let mut pts = Vec::with_capacity(24);
    let corners = [
        (x + w - rx, y + ry, -core::f32::consts::FRAC_PI_2, 0.0),
        (x + w - rx, y + h - ry, 0.0, core::f32::consts::FRAC_PI_2),
        (x + rx, y + h - ry, core::f32::consts::FRAC_PI_2, core::f32::consts::PI),
        (x + rx, y + ry, core::f32::consts::PI, core::f32::consts::PI * 1.5),
    ];
    for (cx, cy, a0, a1) in corners {
        for s in 0..=4 {
            let a = a0 + (a1 - a0) * (s as f32 / 4.0);
            let (sa, ca) = a.sin_cos();
            pts.push((cx + rx * ca, cy + ry * sa));
        }
    }
    pts
}

fn point_in_subpaths(px: f32, py: f32, subpaths: &[Vec<(f32, f32)>], rule: FillRule) -> bool {
    let mut winding = 0i32;
    let mut odd = false;

    for poly in subpaths {
        let n = poly.len();
        if n < 3 { continue; }
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = poly[i];
            let (xj, yj) = poly[j];
            if (yi > py) != (yj > py) {
                let x_int = xj + (py - yj) * (xi - xj) / (yi - yj);
                if px < x_int {
                    odd = !odd;
                }
                if yj <= py {
                    if yi > py && (xi - xj) * (py - yj) - (px - xj) * (yi - yj) > 0.0 {
                        winding += 1;
                    }
                } else if yi <= py && (xi - xj) * (py - yj) - (px - xj) * (yi - yj) < 0.0 {
                    winding -= 1;
                }
            }
            j = i;
        }
    }

    match rule {
        FillRule::EvenOdd => odd,
        FillRule::NonZero => winding != 0,
    }
}

#[inline(always)]
fn alpha_over(dst: u32, src: u32) -> u32 {
    let sa = (src >> 24) & 0xFF;
    if sa == 0 { return dst; }
    if sa == 255 { return src; }
    let da = (dst >> 24) & 0xFF;
    if da == 0 { return src; }

    let inv_sa = 255 - sa;
    let out_a = sa + ((da * inv_sa + 127) / 255);
    if out_a == 0 { return 0; }

    let sr = (src >> 16) & 0xFF;
    let sg = (src >> 8) & 0xFF;
    let sb = src & 0xFF;

    let dr = (dst >> 16) & 0xFF;
    let dg = (dst >> 8) & 0xFF;
    let db = dst & 0xFF;

    let r = (sr * sa + ((dr * da * inv_sa + 127) / 255)) / out_a;
    let g = (sg * sa + ((dg * da * inv_sa + 127) / 255)) / out_a;
    let b = (sb * sa + ((db * da * inv_sa + 127) / 255)) / out_a;

    (out_a.min(255) << 24) | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}

/// تصيير أي ملف SVG إلى مصفوفة RGBA8888 (0xAARRGGBB) مع قناة Alpha كاملة وتنعيم 4x4 SSAA
pub fn rasterize_svg(svg_xml: &str, out_w: usize, out_h: usize) -> Vec<u32> {
    if out_w == 0 || out_h == 0 { return Vec::new(); }
    let mut out = vec![0x00000000u32; out_w * out_h];

    let mut vb_x = 0.0f32;
    let mut vb_y = 0.0f32;
    let mut vb_w = 24.0f32;
    let mut vb_h = 24.0f32;

    if let Some(svg_start) = svg_xml.find("<svg") {
        if let Some(svg_end) = svg_xml[svg_start..].find('>') {
            let svg_tag = &svg_xml[svg_start..svg_start + svg_end + 1];
            if let Some(vb) = get_attr(svg_tag, "viewBox") {
                let mut lex = NumLexer::new(vb);
                if let (Some(x), Some(y), Some(w), Some(h)) = (lex.next_f32(), lex.next_f32(), lex.next_f32(), lex.next_f32()) {
                    if w > 0.0 && h > 0.0 {
                        vb_x = x;
                        vb_y = y;
                        vb_w = w;
                        vb_h = h;
                    }
                }
            } else if let (Some(ws), Some(hs)) = (get_attr(svg_tag, "width"), get_attr(svg_tag, "height")) {
                if let (Some(w), Some(h)) = (NumLexer::new(ws).next_f32(), NumLexer::new(hs).next_f32()) {
                    if w > 0.0 && h > 0.0 {
                        vb_w = w;
                        vb_h = h;
                    }
                }
            }
        }
    }

    let scale_x = (out_w as f32) / vb_w;
    let scale_y = (out_h as f32) / vb_h;
    let base_tf = Transform2D {
        a: scale_x,
        b: 0.0,
        c: 0.0,
        d: scale_y,
        e: -vb_x * scale_x,
        f: -vb_y * scale_y,
    };

    let mut group_stack: Vec<(Transform2D, Option<u32>, f32, FillRule)> = vec![(base_tf, None, 1.0, FillRule::NonZero)];
    let mut shapes: Vec<SvgShape> = Vec::new();

    let mut pos = 0;
    while pos < svg_xml.len() {
        let Some(lt) = svg_xml[pos..].find('<') else { break };
        let tag_start = pos + lt;
        let Some(gt) = svg_xml[tag_start..].find('>') else { break };
        let tag_end = tag_start + gt + 1;
        let tag = &svg_xml[tag_start..tag_end];
        pos = tag_end;

        if tag.starts_with("<!--") || tag.starts_with("<?") || tag.starts_with("<!") {
            continue;
        }
        if tag.starts_with("</g") {
            if group_stack.len() > 1 {
                group_stack.pop();
            }
            continue;
        }

        let is_self_closing = tag.ends_with("/>");
        let name_end = tag[1..].find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/').unwrap_or(0) + 1;
        let elem_name = &tag[1..name_end];

        let (parent_tf, parent_fill, parent_op, parent_rule) = *group_stack.last().unwrap();

        let tf = if let Some(tr_str) = get_attr(tag, "transform") {
            parent_tf.multiply(&parse_transform(tr_str))
        } else {
            parent_tf
        };

        let style = get_attr(tag, "style").unwrap_or("");
        let fill_attr = get_attr(tag, "fill").or_else(|| get_style_prop(style, "fill"));
        let mut fill_col = parent_fill;
        let mut skip_fill = false;
        if let Some(f) = fill_attr {
            if f.trim().eq_ignore_ascii_case("none") {
                skip_fill = true;
            } else if let Some(c) = parse_hex_color(f) {
                fill_col = Some(c);
            }
        }

        let op_attr = get_attr(tag, "opacity").or_else(|| get_style_prop(style, "opacity"));
        let fop_attr = get_attr(tag, "fill-opacity").or_else(|| get_style_prop(style, "fill-opacity"));
        let op = parent_op
            * op_attr.and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(1.0).clamp(0.0, 1.0)
            * fop_attr.and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(1.0).clamp(0.0, 1.0);

        let rule_attr = get_attr(tag, "fill-rule").or_else(|| get_style_prop(style, "fill-rule"));
        let rule = match rule_attr {
            Some(r) if r.trim().eq_ignore_ascii_case("evenodd") => FillRule::EvenOdd,
            Some(_) => FillRule::NonZero,
            None => parent_rule,
        };

        if elem_name == "g" {
            if !is_self_closing {
                group_stack.push((tf, fill_col, op, rule));
            }
            continue;
        }

        if skip_fill || op <= 0.001 {
            continue;
        }

        let mut raw_subpaths: Vec<Vec<(f32, f32)>> = Vec::new();
        match elem_name {
            "path" => {
                if let Some(d) = get_attr(tag, "d") {
                    raw_subpaths = parse_path_d(d);
                }
            }
            "rect" => {
                let x = get_attr(tag, "x").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let y = get_attr(tag, "y").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let w = get_attr(tag, "width").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let h = get_attr(tag, "height").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let rx = get_attr(tag, "rx").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let ry = get_attr(tag, "ry").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(rx);
                if w > 0.0 && h > 0.0 {
                    raw_subpaths.push(build_rect_poly(x, y, w, h, rx, ry));
                }
            }
            "circle" => {
                let cx = get_attr(tag, "cx").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let cy = get_attr(tag, "cy").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let r = get_attr(tag, "r").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                if r > 0.0 {
                    raw_subpaths.push(build_circle_poly(cx, cy, r, r));
                }
            }
            "ellipse" => {
                let cx = get_attr(tag, "cx").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let cy = get_attr(tag, "cy").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let rx = get_attr(tag, "rx").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                let ry = get_attr(tag, "ry").and_then(|s| NumLexer::new(s).next_f32()).unwrap_or(0.0);
                if rx > 0.0 && ry > 0.0 {
                    raw_subpaths.push(build_circle_poly(cx, cy, rx, ry));
                }
            }
            "polygon" | "polyline" => {
                if let Some(pts_str) = get_attr(tag, "points") {
                    let mut lex = NumLexer::new(pts_str);
                    let mut pts = Vec::new();
                    while lex.peek_is_number() {
                        if let (Some(x), Some(y)) = (lex.next_f32(), lex.next_f32()) {
                            pts.push((x, y));
                        } else {
                            break;
                        }
                    }
                    if pts.len() >= 3 {
                        raw_subpaths.push(pts);
                    }
                }
            }
            _ => {}
        }

        if !raw_subpaths.is_empty() {
            let base_col = fill_col.unwrap_or(0xFFCBD5E1);
            let a = (((base_col >> 24) & 0xFF) as f32 * op).round().clamp(0.0, 255.0) as u32;
            let final_col = (a << 24) | (base_col & 0x00FFFFFF);
            for poly in &mut raw_subpaths {
                for pt in poly.iter_mut() {
                    *pt = tf.apply(pt.0, pt.1);
                }
            }
            shapes.push(SvgShape { subpaths: raw_subpaths, color: final_col, fill_rule: rule });
        }
    }

    // 4x4 Subpixel Supersampling Rasterization
    let sub_offsets: [f32; 4] = [0.125, 0.375, 0.625, 0.875];
    for shape in &shapes {
        let base_a = (shape.color >> 24) & 0xFF;
        if base_a == 0 { continue; }
        let rgb = shape.color & 0x00FFFFFF;

        let mut min_x = out_w as f32;
        let mut min_y = out_h as f32;
        let mut max_x = 0.0f32;
        let mut max_y = 0.0f32;
        for poly in &shape.subpaths {
            for &(px, py) in poly {
                if px < min_x { min_x = px; }
                if py < min_y { min_y = py; }
                if px > max_x { max_x = px; }
                if py > max_y { max_y = py; }
            }
        }

        let x0 = (min_x.floor() as isize).max(0) as usize;
        let y0 = (min_y.floor() as isize).max(0) as usize;
        let x1 = ((max_x.ceil() as isize) + 1).clamp(0, out_w as isize) as usize;
        let y1 = ((max_y.ceil() as isize) + 1).clamp(0, out_h as isize) as usize;

        for py in y0..y1 {
            let row = py * out_w;
            for px in x0..x1 {
                let mut hits = 0u32;
                for &oy in &sub_offsets {
                    let sy = (py as f32) + oy;
                    for &ox in &sub_offsets {
                        let sx = (px as f32) + ox;
                        if point_in_subpaths(sx, sy, &shape.subpaths, shape.fill_rule) {
                            hits += 1;
                        }
                    }
                }
                if hits > 0 {
                    let eff_a = (base_a * hits + 8) / 16;
                    if eff_a > 0 {
                        let src = (eff_a << 24) | rgb;
                        out[row + px] = alpha_over(out[row + px], src);
                    }
                }
            }
        }
    }

    out
}
