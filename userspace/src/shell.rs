use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub struct ShellOutput {
    pub lines: Vec<(String, u32)>,
    pub clear_screen: bool,
    pub exit_session: bool,
}

impl ShellOutput {
    fn empty() -> Self {
        Self { lines: Vec::new(), clear_screen: false, exit_session: false }
    }
}

pub struct ShellSession {
    pub cwd: String,
    pub cmd_history: Vec<String>,
    pub env_vars: BTreeMap<String, String>,
}

impl ShellSession {
    pub fn new() -> Self {
        let mut env_vars = BTreeMap::new();
        env_vars.insert("USER".into(), "root".into());
        env_vars.insert("HOME".into(), "/".into());
        env_vars.insert("PATH".into(), "/EOS SHARE/bin:/r/bin:/bin:/usr/bin".into());
        env_vars.insert("SHELL".into(), "/bin/eos-sh".into());
        env_vars.insert("TERM".into(), "xterm-256color".into());

        let mut s = Self {
            cwd: "/".into(),
            cmd_history: Vec::new(),
            env_vars,
        };
        s.reload_history();
        s
    }

    pub fn prompt_prefix(&self, is_ssh: bool) -> String {
        let disp = if self.cwd == "/" { "~" } else { self.cwd.as_str() };
        if is_ssh {
            format!("root@eos:{}# ", disp)
        } else {
            format!("root@eos {}> ", disp)
        }
    }

    pub fn reload_history(&mut self) {
        self.cmd_history.clear();
        for p in ["/home/march/.bash_history", "home/march/.bash_history", ".bash_history"] {
            if let Ok(content) = fs::read_to_string(p) {
                for line in content.lines() {
                    let tr = line.trim();
                    if !tr.is_empty() && !self.cmd_history.contains(&tr.to_string()) {
                        self.cmd_history.push(tr.to_string());
                    }
                }
                break;
            }
        }
    }

    pub fn record_history(&mut self, raw_cmd: &str) {
        let tr = raw_cmd.trim();
        if tr.is_empty() { return; }
        self.reload_history();
        self.cmd_history.retain(|x| x != tr);
        self.cmd_history.push(tr.to_string());
        let payload = self.cmd_history.join("\n") + "\n";
        for p in ["/home/march/.bash_history", "home/march/.bash_history", ".bash_history"] {
            let _ = fs::write(p, payload.as_bytes());
        }
    }

    pub fn resolve_path(&self, arg: &str) -> String {
        let a = arg.trim();
        if a.is_empty() || a == "." {
            self.cwd.clone()
        } else if a == "~" {
            "/".into()
        } else if a.starts_with('/') {
            a.to_string()
        } else if self.cwd == "/" {
            format!("/{}", a)
        } else {
            format!("{}/{}", self.cwd.trim_end_matches('/'), a)
        }
    }

    fn read_file_flex(&self, path: &str) -> Result<Vec<u8>, String> {
        let full = self.resolve_path(path);
        let clean = full.trim_start_matches('/').to_string();
        for cand in [full.as_str(), clean.as_str(), path] {
            if let Ok(b) = fs::read(cand) {
                return Ok(b);
            }
        }
        Err(format!("{}: No such file", path))
    }

    fn read_text_flex(&self, path: &str) -> Result<String, String> {
        let bytes = self.read_file_flex(path)?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    pub fn list_dir_entries(&self, target: &str) -> Result<Vec<(String, bool, usize)>, String> {
        let full = self.resolve_path(target);
        let clean = full.trim_matches('/').to_string();
        let cands = if clean.is_empty() {
            vec![".".to_string(), "/".to_string()]
        } else {
            vec![clean.clone(), format!("/{}", clean)]
        };

        for p in &cands {
            if let Ok(entries) = fs::read_dir(p) {
                let mut items = Vec::new();
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name == "." || name == ".." { continue; }
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let item_path = if clean.is_empty() { name.clone() } else { format!("{}/{}", clean, name) };
                    let sz = if is_dir { 0 } else { fs::metadata(&item_path).map(|m| m.len() as usize).unwrap_or(0) };
                    items.push((name, is_dir, sz));
                }
                items.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                return Ok(items);
            }
        }
        Err(format!("ls: '{}': No such directory", target))
    }

    pub fn split_pipeline(line: &str) -> Vec<String> {
        let mut stages = Vec::new();
        let mut cur = String::new();
        let mut in_q = false;
        let mut qch = '"';
        for ch in line.chars() {
            if in_q {
                if ch == qch { in_q = false; }
                cur.push(ch);
            } else if ch == '"' || ch == '\'' {
                in_q = true;
                qch = ch;
                cur.push(ch);
            } else if ch == '|' {
                if !cur.trim().is_empty() {
                    stages.push(core::mem::take(&mut cur));
                }
            } else {
                cur.push(ch);
            }
        }
        if !cur.trim().is_empty() {
            stages.push(cur);
        }
        stages
    }

    pub fn parse_tokens(stage: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut cur = String::new();
        let mut in_q = false;
        let mut qch = '"';
        let mut chars = stage.chars().peekable();
        while let Some(c) = chars.next() {
            if in_q {
                if c == '\\' {
                    if let Some(n) = chars.next() { cur.push(n); }
                } else if c == qch {
                    in_q = false;
                } else {
                    cur.push(c);
                }
            } else if c == '"' || c == '\'' {
                in_q = true;
                qch = c;
            } else if c.is_whitespace() {
                if !cur.is_empty() {
                    tokens.push(core::mem::take(&mut cur));
                }
            } else {
                cur.push(c);
            }
        }
        if !cur.is_empty() {
            tokens.push(cur);
        }
        tokens
    }

    pub fn execute_line(&mut self, raw_cmd: &str, record_hist: bool) -> ShellOutput {
        let trimmed = raw_cmd.trim();
        if trimmed.is_empty() {
            return ShellOutput::empty();
        }
        if record_hist {
            self.record_history(trimmed);
        }

        let stages = Self::split_pipeline(trimmed);
        if stages.is_empty() {
            return ShellOutput::empty();
        }

        let mut pipe_in: Option<Vec<String>> = None;
        let mut final_out = ShellOutput::empty();

        for (idx, stage_str) in stages.iter().enumerate() {
            let raw_tokens = Self::parse_tokens(stage_str);
            if raw_tokens.is_empty() { continue; }

            let mut redir: Option<(String, bool)> = None;
            let mut tokens = Vec::new();
            let mut ti = 0;
            while ti < raw_tokens.len() {
                if raw_tokens[ti] == ">" || raw_tokens[ti] == ">>" {
                    let append = raw_tokens[ti] == ">>";
                    if let Some(t) = raw_tokens.get(ti + 1) {
                        redir = Some((self.resolve_path(t), append));
                    }
                    break;
                }
                tokens.push(raw_tokens[ti].clone());
                ti += 1;
            }
            if tokens.is_empty() { continue; }

            let stage_res = self.execute_stage(&tokens[0], &tokens[1..], pipe_in.as_deref());
            if stage_res.clear_screen || stage_res.exit_session {
                return stage_res;
            }

            if let Some((target_path, append)) = redir {
                let mut payload = stage_res.lines.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>().join("\n");
                payload.push('\n');
                let clean = target_path.trim_start_matches('/');
                if append {
                    if let Ok(existing) = fs::read_to_string(clean).or_else(|_| fs::read_to_string(&target_path)) {
                        payload = existing + &payload;
                    }
                }
                match fs::write(clean, payload.as_bytes()).or_else(|_| fs::write(&target_path, payload.as_bytes())) {
                    Ok(_) => final_out.lines.push((format!("-> {}", target_path), 0xFF10B981)),
                    Err(e) => final_out.lines.push((format!("redirect error: {:?}", e), 0xFFEF4444)),
                }
                pipe_in = Some(Vec::new());
                continue;
            }

            if idx + 1 < stages.len() {
                pipe_in = Some(stage_res.lines.into_iter().map(|(s, _)| s).collect());
            } else {
                final_out = stage_res;
            }
        }

        final_out
    }

    fn execute_stage(&mut self, prog: &str, args: &[String], stdin: Option<&[String]>) -> ShellOutput {
        let mut out = ShellOutput::empty();

        match prog {
            "help" => {
                out.lines.push(("POSIX : ls [-l|-a] cd pwd cat head tail wc grep sort uniq cut tr find stat hexdump".into(), 0xFFE2E8F0));
                out.lines.push(("FILES : touch mkdir cp mv rm echo env export history clear exit | pipes '|' & '>' '>>'".into(), 0xFF38BDF8));
                out.lines.push(("SYSTEM: ps kill <pid> free lspci uname whoami audit python reboot shutdown".into(), 0xFF34D399));
                out.lines.push(("NET   : ifconfig dhcp arp [-scan] ping <ip> ssh <user@ip> [port] sshd".into(), 0xFFFCD34D));
            }
            "clear" => { out.clear_screen = true; }
            "exit" | "logout" => { out.exit_session = true; }
            "whoami" => out.lines.push(("root".into(), 0xFF34D399)),
            "uname" => out.lines.push(("EOS 0.1.0 x86_64 SMP (musl / POSIX)".into(), 0xFFE2E8F0)),
            "pwd" => out.lines.push((self.cwd.clone(), 0xFFE2E8F0)),
            "echo" => {
                let expanded: Vec<String> = args.iter().map(|a| {
                    if let Some(k) = a.strip_prefix('$') {
                        self.env_vars.get(k).cloned().unwrap_or_default()
                    } else {
                        a.clone()
                    }
                }).collect();
                out.lines.push((expanded.join(" "), 0xFFE2E8F0));
            }
            "env" => {
                for (k, v) in &self.env_vars {
                    out.lines.push((format!("{}={}", k, v), 0xFFE2E8F0));
                }
            }
            "export" => {
                for a in args {
                    if let Some((k, v)) = a.split_once('=') {
                        self.env_vars.insert(k.to_string(), v.to_string());
                    }
                }
            }
            "history" => {
                self.reload_history();
                for (i, h) in self.cmd_history.iter().enumerate() {
                    out.lines.push((format!("{:3}  {}", i + 1, h), 0xFFE2E8F0));
                }
            }
            "cd" => {
                let target = args.join(" ");
                if target.is_empty() || target == "~" || target == "/" {
                    self.cwd = "/".into();
                } else if target == ".." {
                    if let Some(pos) = self.cwd.trim_end_matches('/').rfind('/') {
                        let p = &self.cwd[..pos];
                        self.cwd = if p.is_empty() { "/".into() } else { p.into() };
                    } else {
                        self.cwd = "/".into();
                    }
                } else {
                    let next = self.resolve_path(&target);
                    if self.list_dir_entries(&next).is_ok() {
                        self.cwd = next;
                    } else {
                        out.lines.push((format!("cd: no such directory: {}", target), 0xFFEF4444));
                    }
                }
            }
            "mount" => {
                if args.is_empty() {
                    for m in crate::net_manager::list_net_mounts() {
                        out.lines.push((
                            format!("/net/{} -> {}://{}.{}.{}.{}:{}", m.name, m.proto, m.ip[0], m.ip[1], m.ip[2], m.ip[3], m.port),
                            0xFF38BDF8,
                        ));
                    }
                } else if args.len() >= 2 {
                    if let Some(ip) = crate::net_manager::parse_ipv4(&args[1]) {
                        let port = args.get(2).and_then(|s| s.parse::<u16>().ok()).unwrap_or(8000);
                        out.lines.push((crate::net_manager::mount_net_share(&args[0], ip, port), 0xFF34D399));
                    } else {
                        out.lines.push(("usage: mount <name> <ip> [port]".into(), 0xFFFCD34D));
                    }
                } else {
                    out.lines.push(("usage: mount [name ip [port]]".into(), 0xFFFCD34D));
                }
            }
            "ls" => {
                let mut long_fmt = false;
                let mut show_all = false;
                let mut path_parts = Vec::new();
                for a in args {
                    if a.starts_with('-') {
                        if a.contains('l') { long_fmt = true; }
                        if a.contains('a') { show_all = true; }
                    } else {
                        path_parts.push(a.as_str());
                    }
                }
                let target = if path_parts.is_empty() { self.cwd.clone() } else { path_parts.join(" ") };
                let clean_net = target.trim_matches('/');
                if clean_net == "net" {
                    for m in crate::net_manager::list_net_mounts() {
                        out.lines.push((format!("{}/", m.name), 0xFF38BDF8));
                    }
                    return out;
                } else if let Some(rest) = clean_net.strip_prefix("net/") {
                    let (share, sub) = rest.split_once('/').unwrap_or((rest, ""));
                    match crate::net_manager::fetch_net_share_file(share, sub) {
                        Ok(body) => {
                            if body.contains("<a href=\"") {
                                for line in body.lines() {
                                    if let Some(pos) = line.find("<a href=\"") {
                                        let after = &line[pos + 9..];
                                        if let Some(end) = after.find('"') {
                                            let href = &after[..end];
                                            if !href.is_empty() && href != "../" {
                                                let col = if href.ends_with('/') { 0xFF38BDF8 } else { 0xFFE2E8F0 };
                                                out.lines.push((href.to_string(), col));
                                            }
                                        }
                                    }
                                }
                            } else {
                                for l in body.lines().take(40) { out.lines.push((l.to_string(), 0xFFE2E8F0)); }
                            }
                        }
                        Err(e) => out.lines.push((format!("ls /net/{}: {}", share, e), 0xFFEF4444)),
                    }
                    return out;
                }
                match self.list_dir_entries(&target) {
                    Ok(items) => {
                        for (name, is_dir, sz) in items {
                            if !show_all && name.starts_with('.') { continue; }
                            let lower = name.to_ascii_lowercase();
                            let col = if is_dir {
                                0xFF38BDF8
                            } else if lower.ends_with(".elf") || lower.ends_with(".sh") || lower.ends_with(".py") {
                                0xFF34D399
                            } else {
                                0xFFE2E8F0
                            };
                            if long_fmt {
                                let perm = if is_dir { "drwxr-xr-x" } else { "-rw-r--r--" };
                                out.lines.push((format!("{}  root  {:>10}  {}{}", perm, sz, name, if is_dir { "/" } else { "" }), col));
                            } else {
                                out.lines.push((format!("{}{}", name, if is_dir { "/" } else { "" }), col));
                            }
                        }
                    }
                    Err(e) => out.lines.push((e, 0xFFEF4444)),
                }
            }
            "cat" => {
                if args.is_empty() {
                    if let Some(lines) = stdin {
                        for l in lines { out.lines.push((l.clone(), 0xFFE2E8F0)); }
                    } else {
                        out.lines.push(("cat: missing operand".into(), 0xFFEF4444));
                    }
                } else {
                    let target = args.join(" ");
                    let full_t = self.resolve_path(&target);
                    if let Some(rest) = full_t.trim_matches('/').strip_prefix("net/") {
                        let (share, sub) = rest.split_once('/').unwrap_or((rest, ""));
                        match crate::net_manager::fetch_net_share_file(share, sub) {
                            Ok(txt) => for l in txt.lines() { out.lines.push((l.to_string(), 0xFFE2E8F0)); },
                            Err(e) => out.lines.push((format!("cat: {}", e), 0xFFEF4444)),
                        }
                        return out;
                    }
                    match self.read_text_flex(&target) {
                        Ok(txt) => for l in txt.lines() { out.lines.push((l.to_string(), 0xFFE2E8F0)); },
                        Err(e) => out.lines.push((format!("cat: {}", e), 0xFFEF4444)),
                    }
                }
            }
            "head" => {
                let mut n = 10usize;
                let mut file_parts = Vec::new();
                let mut i = 0;
                while i < args.len() {
                    if args[i] == "-n" && i + 1 < args.len() {
                        n = args[i + 1].parse().unwrap_or(10);
                        i += 2;
                    } else {
                        file_parts.push(args[i].as_str());
                        i += 1;
                    }
                }
                let src = self.load_lines_or_stdin(&file_parts.join(" "), stdin);
                for l in src.into_iter().take(n) { out.lines.push((l, 0xFFE2E8F0)); }
            }
            "tail" => {
                let mut n = 10usize;
                let mut file_parts = Vec::new();
                let mut i = 0;
                while i < args.len() {
                    if args[i] == "-n" && i + 1 < args.len() {
                        n = args[i + 1].parse().unwrap_or(10);
                        i += 2;
                    } else {
                        file_parts.push(args[i].as_str());
                        i += 1;
                    }
                }
                let src = self.load_lines_or_stdin(&file_parts.join(" "), stdin);
                let skip = src.len().saturating_sub(n);
                for l in src.into_iter().skip(skip) { out.lines.push((l, 0xFFE2E8F0)); }
            }
            "wc" => {
                let mut only_l = false;
                let mut only_w = false;
                let mut only_c = false;
                let mut file_parts = Vec::new();
                for a in args {
                    if a == "-l" { only_l = true; }
                    else if a == "-w" { only_w = true; }
                    else if a == "-c" { only_c = true; }
                    else { file_parts.push(a.as_str()); }
                }
                let src = self.load_lines_or_stdin(&file_parts.join(" "), stdin);
                let lc = src.len();
                let wc: usize = src.iter().map(|l| l.split_whitespace().count()).sum();
                let bc: usize = src.iter().map(|l| l.len() + 1).sum();
                let msg = if only_l { format!("{}", lc) }
                    else if only_w { format!("{}", wc) }
                    else if only_c { format!("{}", bc) }
                    else { format!("{:>6} {:>6} {:>6}", lc, wc, bc) };
                out.lines.push((msg, 0xFFE2E8F0));
            }
            "grep" => {
                let mut icase = false;
                let mut invert = false;
                let mut show_num = false;
                let mut pos = Vec::new();
                for a in args {
                    if a.starts_with('-') {
                        if a.contains('i') { icase = true; }
                        if a.contains('v') { invert = true; }
                        if a.contains('n') { show_num = true; }
                    } else {
                        pos.push(a.as_str());
                    }
                }
                if pos.is_empty() {
                    out.lines.push(("usage: grep [-i|-n|-v] <pattern> [file]".into(), 0xFFFCD34D));
                } else {
                    let pat = pos[0];
                    let farg = if pos.len() > 1 { pos[1..].join(" ") } else { String::new() };
                    let src = self.load_lines_or_stdin(&farg, stdin);
                    let pat_l = pat.to_ascii_lowercase();
                    for (idx, l) in src.into_iter().enumerate() {
                        let matched = if icase { l.to_ascii_lowercase().contains(&pat_l) } else { l.contains(pat) };
                        if matched ^ invert {
                            let s = if show_num { format!("{}:{}", idx + 1, l) } else { l };
                            out.lines.push((s, 0xFF34D399));
                        }
                    }
                }
            }
            "sort" => {
                let rev = args.iter().any(|a| a == "-r");
                let farg = args.iter().filter(|a| !a.starts_with('-')).cloned().collect::<Vec<_>>().join(" ");
                let mut src = self.load_lines_or_stdin(&farg, stdin);
                src.sort();
                if rev { src.reverse(); }
                for l in src { out.lines.push((l, 0xFFE2E8F0)); }
            }
            "uniq" => {
                let farg = args.join(" ");
                let src = self.load_lines_or_stdin(&farg, stdin);
                let mut last: Option<String> = None;
                for l in src {
                    if last.as_deref() != Some(l.as_str()) {
                        out.lines.push((l.clone(), 0xFFE2E8F0));
                        last = Some(l);
                    }
                }
            }
            "tr" => {
                if args.len() >= 2 {
                    let from_c: Vec<char> = args[0].chars().collect();
                    let to_c: Vec<char> = args[1].chars().collect();
                    let src = self.load_lines_or_stdin("", stdin);
                    for l in src {
                        let mapped: String = l.chars().map(|ch| {
                            if let Some(p) = from_c.iter().position(|&c| c == ch) {
                                *to_c.get(p).or_else(|| to_c.last()).unwrap_or(&ch)
                            } else { ch }
                        }).collect();
                        out.lines.push((mapped, 0xFFE2E8F0));
                    }
                } else {
                    out.lines.push(("usage: ... | tr <set1> <set2>".into(), 0xFFFCD34D));
                }
            }
            "cut" => {
                let mut delim = ' ';
                let mut field = 1usize;
                let mut i = 0;
                while i < args.len() {
                    if args[i] == "-d" && i + 1 < args.len() {
                        delim = args[i + 1].chars().next().unwrap_or(' ');
                        i += 2;
                    } else if args[i] == "-f" && i + 1 < args.len() {
                        field = args[i + 1].parse().unwrap_or(1).max(1);
                        i += 2;
                    } else { i += 1; }
                }
                let src = self.load_lines_or_stdin("", stdin);
                for l in src {
                    let seg = l.split(delim).nth(field - 1).unwrap_or("").to_string();
                    out.lines.push((seg, 0xFFE2E8F0));
                }
            }
            "find" => {
                let mut root = self.cwd.clone();
                let mut name_filter: Option<String> = None;
                let mut i = 0;
                while i < args.len() {
                    if args[i] == "-name" && i + 1 < args.len() {
                        name_filter = Some(args[i + 1].replace('*', "").to_ascii_lowercase());
                        i += 2;
                    } else if !args[i].starts_with('-') {
                        root = self.resolve_path(&args[i]);
                        i += 1;
                    } else { i += 1; }
                }
                self.walk_find(&root, name_filter.as_deref(), 0, &mut out.lines);
            }
            "stat" => {
                if args.is_empty() {
                    out.lines.push(("usage: stat <path>".into(), 0xFFFCD34D));
                } else {
                    let full = self.resolve_path(&args.join(" "));
                    let clean = full.trim_start_matches('/');
                    match fs::metadata(clean).or_else(|_| fs::metadata(&full)) {
                        Ok(m) => {
                            let kind = if m.is_dir() { "directory" } else { "regular file" };
                            out.lines.push((format!("  File: {}", full), 0xFF38BDF8));
                            out.lines.push((format!("  Size: {} bytes    Type: {}    Owner: root (0:0)", m.len(), kind), 0xFFE2E8F0));
                        }
                        Err(_) => out.lines.push((format!("stat: cannot stat '{}': No such file", full), 0xFFEF4444)),
                    }
                }
            }
            "hexdump" => {
                if args.is_empty() {
                    out.lines.push(("usage: hexdump <file>".into(), 0xFFFCD34D));
                } else {
                    match self.read_file_flex(&args.join(" ")) {
                        Ok(bytes) => {
                            for (off, chunk) in bytes.iter().take(256).collect::<Vec<_>>().chunks(16).enumerate() {
                                let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
                                let asc: String = chunk.iter().map(|&&b| if b >= 0x20 && b <= 0x7E { b as char } else { '.' }).collect();
                                out.lines.push((format!("{:08x}  {:<47}  |{}|", off * 16, hex.join(" "), asc), 0xFFE2E8F0));
                            }
                        }
                        Err(e) => out.lines.push((format!("hexdump: {}", e), 0xFFEF4444)),
                    }
                }
            }
            "touch" => {
                if let Some(t) = args.first() {
                    let full = self.resolve_path(t);
                    let clean = full.trim_start_matches('/');
                    if fs::metadata(clean).is_err() {
                        let _ = fs::write(clean, b"");
                    }
                    out.lines.push((format!("Touched {}", full), 0xFF10B981));
                } else { out.lines.push(("touch: missing operand".into(), 0xFFEF4444)); }
            }
            "mkdir" => {
                if let Some(t) = args.first() {
                    let full = self.resolve_path(t);
                    let clean = full.trim_start_matches('/');
                    match fs::create_dir_all(clean) {
                        Ok(_) => out.lines.push((format!("Created directory {}", full), 0xFF10B981)),
                        Err(e) => out.lines.push((format!("mkdir: {:?}", e), 0xFFEF4444)),
                    }
                } else { out.lines.push(("mkdir: missing operand".into(), 0xFFEF4444)); }
            }
            "cp" => {
                if args.len() >= 2 {
                    let src = self.resolve_path(&args[0]);
                    let dst = self.resolve_path(&args[1]);
                    match self.read_file_flex(&src) {
                        Ok(bytes) => match fs::write(dst.trim_start_matches('/'), &bytes) {
                            Ok(_) => out.lines.push((format!("Copied {} -> {} ({} B)", src, dst, bytes.len()), 0xFF10B981)),
                            Err(e) => out.lines.push((format!("cp write error: {:?}", e), 0xFFEF4444)),
                        },
                        Err(e) => out.lines.push((format!("cp: {}", e), 0xFFEF4444)),
                    }
                } else { out.lines.push(("usage: cp <src> <dst>".into(), 0xFFFCD34D)); }
            }
            "mv" => {
                if args.len() >= 2 {
                    let src = self.resolve_path(&args[0]);
                    let dst = self.resolve_path(&args[1]);
                    match self.read_file_flex(&src) {
                        Ok(bytes) => {
                            if fs::write(dst.trim_start_matches('/'), &bytes).is_ok() {
                                let _ = fs::remove_file(src.trim_start_matches('/'));
                                out.lines.push((format!("Moved {} -> {}", src, dst), 0xFF10B981));
                            } else {
                                out.lines.push((format!("mv: failed writing {}", dst), 0xFFEF4444));
                            }
                        }
                        Err(e) => out.lines.push((format!("mv: {}", e), 0xFFEF4444)),
                    }
                } else { out.lines.push(("usage: mv <src> <dst>".into(), 0xFFFCD34D)); }
            }
            "rm" => {
                if !args.is_empty() {
                    let full = self.resolve_path(&args.join(" "));
                    match fs::remove_file(&full).or_else(|_| fs::remove_file(full.trim_start_matches('/'))) {
                        Ok(_) => out.lines.push((format!("Removed '{}'", full), 0xFF10B981)),
                        Err(e) => out.lines.push((format!("rm: '{}': {:?}", full, e), 0xFFEF4444)),
                    }
                } else { out.lines.push(("rm: missing operand".into(), 0xFFEF4444)); }
            }
            "ps" => {
                out.lines.push(("PID   CORE   MEM(KB)   SYSCALLS   NAME".into(), 0xFFFCD34D));
                for p in crate::sdk::sys::query_processes() {
                    out.lines.push((format!("{:<5} {:<6} {:<9} {:<10} {}", p.pid, p.core_id, p.mem_kb, p.syscalls, p.name), 0xFFE2E8F0));
                }
            }
            "kill" => {
                if let Some(pid_s) = args.first() {
                    if let Ok(pid) = pid_s.parse::<u32>() {
                        if crate::syscall::sys_kill_proc(pid) {
                            out.lines.push((format!("Terminated PID {}", pid), 0xFF10B981));
                        } else {
                            out.lines.push((format!("kill: failed or protected PID {}", pid), 0xFFEF4444));
                        }
                    } else { out.lines.push(("kill: invalid PID".into(), 0xFFEF4444)); }
                } else { out.lines.push(("usage: kill <pid>".into(), 0xFFFCD34D)); }
            }
            "free" | "mem" => {
                let used_kb = crate::sdk::agent::query_kernel_heap_used() / 1024;
                let total_kb = 256 * 1024usize;
                out.lines.push(("              total        used        free".into(), 0xFFFCD34D));
                out.lines.push((format!("KernelHeap: {:>7} KB  {:>7} KB  {:>7} KB", total_kb, used_kb, total_kb.saturating_sub(used_kb)), 0xFFE2E8F0));
            }
            "lspci" => {
                let gpu = crate::sdk::gpu::query_info();
                let aud = if crate::sdk::audio::is_available() { "Active" } else { "Offline" };
                for d in [
                    "00:00.0 Host bridge: Intel Q35 / ICH9".to_string(),
                    format!("00:02.0 Display: {} ({}x{})", gpu.backend_name(), gpu.width, gpu.height),
                    "00:03.0 Ethernet: Intel E1000 Gigabit".to_string(),
                    format!("00:04.0 Audio: Intel AC'97 ({})", aud),
                    "00:1f.2 Storage: PIIX3 IDE/SATA".to_string(),
                ] { out.lines.push((d, 0xFFE2E8F0)); }
            }
            "audit" => {
                let passed = crate::sdk::agent::trigger_kernel_audit();
                out.lines.push((format!("Kernel self-audit completed: {}/6 checks passed", passed), 0xFF34D399));
            }
            "ifconfig" | "ip" => {
                for l in crate::net_manager::get_ifconfig_lines() { out.lines.push((l, 0xFF38BDF8)); }
            }
            "dhcp" => {
                out.lines.push(("Sending DHCPDISCOVER on eth0...".into(), 0xFFFCD34D));
                for l in crate::net_manager::renew_dhcp_sync() { out.lines.push((l, 0xFF34D399)); }
            }
            "arp" => {
                let scan = args.iter().any(|a| a == "-scan" || a == "scan");
                for l in crate::net_manager::get_arp_lines(scan) { out.lines.push((l, 0xFFE2E8F0)); }
            }
            "ping" => {
                let target = args.first().map(|s| s.as_str()).unwrap_or("10.0.2.2");
                if let Some(ip) = crate::net_manager::parse_ipv4(target) {
                    for l in crate::net_manager::run_ping_sync(ip, 4) {
                        let c = if l.contains("timeout") { 0xFFF87171 } else { 0xFF34D399 };
                        out.lines.push((l, c));
                    }
                } else { out.lines.push((format!("ping: invalid IPv4 '{}'", target), 0xFFEF4444)); }
            }
            "ssh" => {
                if let Some(t) = args.first() {
                    let port = args.get(1).and_then(|s| s.parse::<u16>().ok()).unwrap_or(22);
                    for l in crate::ssh::run_ssh_client(t, port) { out.lines.push((l, 0xFF34D399)); }
                } else { out.lines.push(("usage: ssh <user@ip> [port]".into(), 0xFFFCD34D)); }
            }
            "sshd" => {
                for l in crate::ssh::sshd_status_lines() { out.lines.push((l, 0xFF38BDF8)); }
            }
            "python" | "python3" => {
                let has_dyn = fs::metadata("/EOS SHARE/bin/python3").is_ok() || fs::metadata("EOS SHARE/bin/python3").is_ok();
                if has_dyn && !args.is_empty() {
                    let mut qa = vec!["python3".to_string()];
                    for a in args {
                        let resolved = if !a.starts_with('-') && a.ends_with(".py") {
                            self.resolve_path(a)
                        } else { a.clone() };
                        if resolved.contains(' ') { qa.push(format!("\"{}\"", resolved)); } else { qa.push(resolved); }
                    }
                    let ret = crate::syscall::sys_spawn_elf("EOS SHARE/bin/python3", &qa.join(" "));
                    if ret >= 0 {
                        let captured = crate::syscall::sys_wait_elf_output(ret as usize);
                        if captured.trim().is_empty() {
                            out.lines.push(("(python3 exited with no output)".into(), 0xFF94A3B8));
                        } else {
                            for l in captured.lines() {
                                out.lines.push((l.to_string(), 0xFFE2E8F0));
                            }
                        }
                    } else {
                        out.lines.push(("Failed to spawn dynamic python3".into(), 0xFFEF4444));
                    }
                } else if args.is_empty() {
                    out.lines.push(("Python 3.12 (EOS musl + dlopen) — usage: python -c '<code>' | python <script.py>".into(), 0xFF34D399));
                } else if args[0] == "-c" && args.len() >= 2 {
                    match crate::ffi_python::run_code(&args[1..].join(" ")) {
                        Ok(res) => for l in res.lines() { out.lines.push((l.to_string(), 0xFFE2E8F0)); },
                        Err(e) => for l in e.lines() { out.lines.push((l.to_string(), 0xFFEF4444)); },
                    }
                } else {
                    let full = self.resolve_path(&args.join(" "));
                    match crate::ffi_python::run_file(&full) {
                        Ok(res) => for l in res.lines() { out.lines.push((l.to_string(), 0xFFE2E8F0)); },
                        Err(e) => for l in e.lines() { out.lines.push((l.to_string(), 0xFFEF4444)); },
                    }
                }
            }
            "eos-update" => {
                use sha2::{Digest, Sha256};
                if args.is_empty() {
                    out.lines.push(("EOS Delta Updater (Network + USB + Limine Rollback)".into(), 0xFF38BDF8));
                    out.lines.push(("Usage: eos-update --usb <dir>   (e.g. eos-update --usb /EOS SHARE/update)".into(), 0xFFE2E8F0));
                    out.lines.push(("       eos-update --net <share> (e.g. eos-update --net host)".into(), 0xFFE2E8F0));
                } else if args[0] == "--usb" {
                    let src_dir = args.get(1).map(|s| s.trim_matches('/')).unwrap_or("EOS SHARE/update");
                    let targets = ["EOS", "INITRD.TAR"];
                    let mut updated = 0usize;
                    for fname in targets {
                        let src_path = format!("{}/{}", src_dir, fname);
                        let cur_path = format!("EOS SHARE/{}", fname);
                        let bak_path = format!("EOS SHARE/{}.BAK", fname);
                        if let Ok(new_bytes) = fs::read(&src_path) {
                            let new_hash = format!("{:x}", Sha256::digest(&new_bytes));
                            let old_hash = fs::read(&cur_path).map(|b| format!("{:x}", Sha256::digest(&b))).unwrap_or_default();
                            if new_hash == old_hash {
                                out.lines.push((format!("[SKIP] {} unchanged (SHA256: {}..)", fname, &new_hash[..12]), 0xFF94A3B8));
                            } else {
                                if let Ok(old_b) = fs::read(&cur_path) {
                                    let _ = fs::write(&bak_path, &old_b);
                                }
                                match fs::write(&cur_path, &new_bytes) {
                                    Ok(_) => {
                                        out.lines.push((format!("[UPDATED] {} ({} KB) | Backup -> {}.BAK", fname, new_bytes.len() / 1024, fname), 0xFF34D399));
                                        updated += 1;
                                    }
                                    Err(e) => out.lines.push((format!("[ERR] Failed writing {}: {:?}", fname, e), 0xFFEF4444)),
                                }
                            }
                        }
                    }
                    if updated > 0 {
                        out.lines.push(("Delta update applied! Run 'reboot' to boot new kernel (Rollback available in Limine).".into(), 0xFFFCD34D));
                    } else {
                        out.lines.push((format!("No newer files found in '/{}'.", src_dir), 0xFF94A3B8));
                    }
                } else if args[0] == "--net" {
                    let share = args.get(1).map(|s| s.as_str()).unwrap_or("host");
                    match crate::net_manager::fetch_net_share_file(share, "eos_manifest.json") {
                        Ok(manifest) => {
                            out.lines.push((format!("Fetched manifest from /net/{} ({} bytes):", share, manifest.len()), 0xFF34D399));
                            for l in manifest.lines().take(12) {
                                out.lines.push((l.to_string(), 0xFFE2E8F0));
                            }
                        }
                        Err(e) => out.lines.push((format!("eos-update --net error: {}", e), 0xFFEF4444)),
                    }
                }
            }
            "reboot" => crate::syscall::sys_power(1),
            "shutdown" => crate::syscall::sys_power(2),
            _ => {
                // PATH lookup: /EOS SHARE/bin/<prog>, /r/bin/<prog>, or /EOS SHARE/bin/busybox <prog>
                let clean_p = prog.trim_start_matches('/');
                let path_cands = [
                    clean_p.to_string(),
                    format!("EOS SHARE/bin/{}", clean_p),
                    format!("r/bin/{}", clean_p),
                ];
                let mut spawned = false;
                for cand in &path_cands {
                    if fs::metadata(cand).is_ok() {
                        let mut argv = vec![clean_p.to_string()];
                        argv.extend(args.iter().cloned());
                        let ret = crate::syscall::sys_spawn_elf(cand, &argv.join(" "));
                        if ret >= 0 {
                            let captured = crate::syscall::sys_wait_elf_output(ret as usize);
                            for l in captured.lines() {
                                out.lines.push((l.to_string(), 0xFFE2E8F0));
                            }
                            spawned = true;
                            break;
                        }
                    }
                }
                if !spawned && fs::metadata("EOS SHARE/bin/busybox").is_ok() {
                    let mut argv = vec![clean_p.to_string()];
                    argv.extend(args.iter().cloned());
                    let ret = crate::syscall::sys_spawn_elf("EOS SHARE/bin/busybox", &argv.join(" "));
                    if ret >= 0 {
                        let captured = crate::syscall::sys_wait_elf_output(ret as usize);
                        for l in captured.lines() {
                            out.lines.push((l.to_string(), 0xFFE2E8F0));
                        }
                        spawned = true;
                    }
                }
                if !spawned {
                    out.lines.push((format!("eos-sh: command not found: {}", prog), 0xFFEF4444));
                }
            }
        }
        out
    }

    fn load_lines_or_stdin(&self, file_arg: &str, stdin: Option<&[String]>) -> Vec<String> {
        if !file_arg.trim().is_empty() {
            if let Ok(txt) = self.read_text_flex(file_arg) {
                return txt.lines().map(|s| s.to_string()).collect();
            }
            return Vec::new();
        }
        stdin.map(|s| s.to_vec()).unwrap_or_default()
    }

    fn walk_find(&self, dir: &str, filter: Option<&str>, depth: usize, out: &mut Vec<(String, u32)>) {
        if depth > 3 || out.len() > 80 { return; }
        if let Ok(items) = self.list_dir_entries(dir) {
            for (name, is_dir, _) in items {
                let full = if dir == "/" { format!("/{}", name) } else { format!("{}/{}", dir.trim_end_matches('/'), name) };
                let matched = match filter {
                    Some(f) => name.to_ascii_lowercase().contains(f),
                    None => true,
                };
                if matched {
                    out.push((full.clone(), if is_dir { 0xFF38BDF8 } else { 0xFFE2E8F0 }));
                }
                if is_dir && depth < 2 {
                    self.walk_find(&full, filter, depth + 1, out);
                }
            }
        }
    }
}

/// Milestone 36: Verify unified POSIX shell pipeline (`echo | grep | wc -l`) & emit marker
pub fn init_posix_shell() {
    let mut sh = ShellSession::new();
    let res = sh.execute_line("echo eos_posix_ready | grep posix | wc -l", false);
    if res.lines.first().map(|(s, _)| s.trim()) == Some("1") {
        crate::sdk::agent::audit_log("UNIX_COREUTILS:PASS:BusyBox_Ready");
    }
}
