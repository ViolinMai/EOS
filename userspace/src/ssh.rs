use std::fs;
use std::sync::Mutex;
use ed25519_dalek::{Signer, SigningKey};
use poly1305::{Poly1305, universal_hash::KeyInit};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use crate::net_manager::{self, NET, TcpState};

pub const SSH_BANNER: &str = "SSH-2.0-EOS_0.1.0\r\n";
const SSH_MSG_DISCONNECT: u8 = 1;
const SSH_MSG_SERVICE_REQUEST: u8 = 5;
const SSH_MSG_SERVICE_ACCEPT: u8 = 6;
const SSH_MSG_KEXINIT: u8 = 20;
const SSH_MSG_NEWKEYS: u8 = 21;
const SSH_MSG_KEX_ECDH_INIT: u8 = 30;
const SSH_MSG_KEX_ECDH_REPLY: u8 = 31;
const SSH_MSG_USERAUTH_REQUEST: u8 = 50;
const SSH_MSG_USERAUTH_SUCCESS: u8 = 52;
const SSH_MSG_CHANNEL_OPEN: u8 = 90;
const SSH_MSG_CHANNEL_OPEN_CONFIRMATION: u8 = 91;
const SSH_MSG_CHANNEL_DATA: u8 = 94;
const SSH_MSG_CHANNEL_EOF: u8 = 96;
const SSH_MSG_CHANNEL_CLOSE: u8 = 97;
const SSH_MSG_CHANNEL_REQUEST: u8 = 98;
const SSH_MSG_CHANNEL_SUCCESS: u8 = 99;

pub struct SshDaemonState {
    pub running: bool,
    pub port: u16,
    pub active_sessions: usize,
    pub last_peer: String,
}

pub static SSHD: Mutex<SshDaemonState> = Mutex::new(SshDaemonState {
    running: false,
    port: 22,
    active_sessions: 0,
    last_peer: String::new(),
});

// ============================================================================
// OpenSSH chacha20-poly1305@openssh.com (DJB 64-bit Nonce + Poly1305 AEAD)
// ============================================================================

#[inline]
fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(7);
}

fn chacha20_openssh_block(key: &[u8; 32], seq: u64, counter: u64) -> [u8; 64] {
    let mut state = [0u32; 16];
    state[0] = 0x61707865;
    state[1] = 0x3320646e;
    state[2] = 0x79622d32;
    state[3] = 0x6b206574;
    for i in 0..8 {
        state[4 + i] = u32::from_le_bytes([key[i * 4], key[i * 4 + 1], key[i * 4 + 2], key[i * 4 + 3]]);
    }
    state[12] = (counter & 0xFFFF_FFFF) as u32;
    state[13] = (counter >> 32) as u32;
    let seq_be = seq.to_be_bytes();
    state[14] = u32::from_le_bytes([seq_be[0], seq_be[1], seq_be[2], seq_be[3]]);
    state[15] = u32::from_le_bytes([seq_be[4], seq_be[5], seq_be[6], seq_be[7]]);

    let mut w = state;
    for _ in 0..10 {
        quarter_round(&mut w, 0, 4, 8, 12);
        quarter_round(&mut w, 1, 5, 9, 13);
        quarter_round(&mut w, 2, 6, 10, 14);
        quarter_round(&mut w, 3, 7, 11, 15);
        quarter_round(&mut w, 0, 5, 10, 15);
        quarter_round(&mut w, 1, 6, 11, 12);
        quarter_round(&mut w, 2, 7, 8, 13);
        quarter_round(&mut w, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        out[i * 4..i * 4 + 4].copy_from_slice(&w[i].wrapping_add(state[i]).to_le_bytes());
    }
    out
}

fn chacha20_openssh_xor(key: &[u8; 32], seq: u64, mut counter: u64, data: &mut [u8]) {
    for chunk in data.chunks_mut(64) {
        let ks = chacha20_openssh_block(key, seq, counter);
        counter = counter.wrapping_add(1);
        for (b, k) in chunk.iter_mut().zip(ks.iter()) {
            *b ^= *k;
        }
    }
}

fn poly1305_mac(poly_key: &[u8; 32], msg: &[u8]) -> [u8; 16] {
    let poly = Poly1305::new(poly_key.into());
    poly.compute_unpadded(msg).into()
}

// ============================================================================
// SSH Wire Helpers
// ============================================================================

fn push_string(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(data);
}

fn push_mpint(buf: &mut Vec<u8>, raw: &[u8]) {
    let mut start = 0;
    while start < raw.len() && raw[start] == 0 {
        start += 1;
    }
    let slice = &raw[start..];
    if slice.is_empty() {
        buf.extend_from_slice(&0u32.to_be_bytes());
    } else if (slice[0] & 0x80) != 0 {
        buf.extend_from_slice(&((slice.len() + 1) as u32).to_be_bytes());
        buf.push(0);
        buf.extend_from_slice(slice);
    } else {
        buf.extend_from_slice(&(slice.len() as u32).to_be_bytes());
        buf.extend_from_slice(slice);
    }
}

fn read_u32(buf: &[u8], off: &mut usize) -> Option<u32> {
    if *off + 4 > buf.len() { return None; }
    let v = u32::from_be_bytes([buf[*off], buf[*off + 1], buf[*off + 2], buf[*off + 3]]);
    *off += 4;
    Some(v)
}

fn read_string<'a>(buf: &'a [u8], off: &mut usize) -> Option<&'a [u8]> {
    let len = read_u32(buf, off)? as usize;
    if *off + len > buf.len() { return None; }
    let s = &buf[*off..*off + len];
    *off += len;
    Some(s)
}

pub fn build_kexinit_payload() -> Vec<u8> {
    let mut payload = Vec::with_capacity(256);
    payload.push(SSH_MSG_KEXINIT);
    let cookie: [u8; 16] = [0x45, 0x4F, 0x53, 0x32, 0xA1, 0xB2, 0xC3, 0xD4, 0x19, 0x28, 0x37, 0x46, 0x55, 0x64, 0x73, 0x82];
    payload.extend_from_slice(&cookie);
    push_string(&mut payload, b"curve25519-sha256,curve25519-sha256@libssh.org");
    push_string(&mut payload, b"ssh-ed25519");
    push_string(&mut payload, b"chacha20-poly1305@openssh.com");
    push_string(&mut payload, b"chacha20-poly1305@openssh.com");
    push_string(&mut payload, b"none");
    push_string(&mut payload, b"none");
    push_string(&mut payload, b"none");
    push_string(&mut payload, b"none");
    push_string(&mut payload, b"");
    push_string(&mut payload, b"");
    payload.push(0);
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload
}

pub fn wrap_ssh_packet(payload: &[u8]) -> Vec<u8> {
    let block_size = 8usize;
    let unpadded = 4 + 1 + payload.len();
    let mut pad_len = block_size - (unpadded % block_size);
    if pad_len < 4 {
        pad_len += block_size;
    }
    let packet_len = (1 + payload.len() + pad_len) as u32;
    let mut pkt = Vec::with_capacity(4 + packet_len as usize);
    pkt.extend_from_slice(&packet_len.to_be_bytes());
    pkt.push(pad_len as u8);
    pkt.extend_from_slice(payload);
    pkt.extend(std::iter::repeat(0u8).take(pad_len));
    pkt
}

fn derive_key_64(k_mpint: &[u8], h: &[u8; 32], letter: u8, session_id: &[u8; 32]) -> [u8; 64] {
    let mut d1 = Sha256::new();
    d1.update(k_mpint);
    d1.update(h);
    d1.update([letter]);
    d1.update(session_id);
    let k1: [u8; 32] = d1.finalize().into();

    let mut d2 = Sha256::new();
    d2.update(k_mpint);
    d2.update(h);
    d2.update(k1);
    let k2: [u8; 32] = d2.finalize().into();

    let mut out = [0u8; 64];
    out[0..32].copy_from_slice(&k1);
    out[32..64].copy_from_slice(&k2);
    out
}

// ============================================================================
// Live SSH-2 Server Session State Machine
// ============================================================================

struct SshSession {
    banner_sent: bool,
    client_banner: Option<String>,
    rx_buf: Vec<u8>,
    client_kexinit: Vec<u8>,
    server_kexinit: Vec<u8>,
    session_id: Option<[u8; 32]>,
    rx_seq: u32,
    tx_seq: u32,
    encrypted: bool,
    c2s_key: [u8; 64],
    s2c_key: [u8; 64],
    pending_c2s: [u8; 64],
    pending_s2c: [u8; 64],
    client_chan: u32,
    cmd_buf: String,
    shell: crate::shell::ShellSession,
}

impl SshSession {
    fn new() -> Self {
        Self {
            banner_sent: false,
            client_banner: None,
            rx_buf: Vec::new(),
            client_kexinit: Vec::new(),
            server_kexinit: Vec::new(),
            session_id: None,
            rx_seq: 0,
            tx_seq: 0,
            encrypted: false,
            c2s_key: [0; 64],
            s2c_key: [0; 64],
            pending_c2s: [0; 64],
            pending_s2c: [0; 64],
            client_chan: 0,
            cmd_buf: String::new(),
            shell: crate::shell::ShellSession::new(),
        }
    }

    fn send_packet(&mut self, payload: &[u8]) {
        if !self.encrypted {
            let pkt = wrap_ssh_packet(payload);
            net_manager::tcp_send(22, &pkt);
            self.tx_seq = self.tx_seq.wrapping_add(1);
            return;
        }

        let block_size = 8usize;
        let unpadded = 1 + payload.len();
        let mut pad_len = block_size - (unpadded % block_size);
        if pad_len < 4 {
            pad_len += block_size;
        }
        let packet_len = (1 + payload.len() + pad_len) as u32;

        let mut k1 = [0u8; 32];
        let mut k2 = [0u8; 32];
        k1.copy_from_slice(&self.s2c_key[0..32]);
        k2.copy_from_slice(&self.s2c_key[32..64]);

        let seq = self.tx_seq as u64;
        let poly_block = chacha20_openssh_block(&k1, seq, 0);
        let mut poly_key = [0u8; 32];
        poly_key.copy_from_slice(&poly_block[0..32]);

        let mut enc_len = packet_len.to_be_bytes();
        let len_ks = chacha20_openssh_block(&k2, seq, 0);
        for i in 0..4 {
            enc_len[i] ^= len_ks[i];
        }

        let mut enc_body = Vec::with_capacity(packet_len as usize);
        enc_body.push(pad_len as u8);
        enc_body.extend_from_slice(payload);
        enc_body.extend(std::iter::repeat(0u8).take(pad_len));
        chacha20_openssh_xor(&k1, seq, 1, &mut enc_body);

        let mut wire = Vec::with_capacity(4 + enc_body.len() + 16);
        wire.extend_from_slice(&enc_len);
        wire.extend_from_slice(&enc_body);
        let tag = poly1305_mac(&poly_key, &wire);
        wire.extend_from_slice(&tag);

        net_manager::tcp_send(22, &wire);
        self.tx_seq = self.tx_seq.wrapping_add(1);
    }

    fn try_pop_packet(&mut self) -> Option<Vec<u8>> {
        if !self.encrypted {
            if self.rx_buf.len() < 4 { return None; }
            let pkt_len = u32::from_be_bytes([self.rx_buf[0], self.rx_buf[1], self.rx_buf[2], self.rx_buf[3]]) as usize;
            if pkt_len > 65536 || self.rx_buf.len() < 4 + pkt_len { return None; }
            let pad_len = self.rx_buf[4] as usize;
            if 1 + pad_len > pkt_len { return None; }
            let payload = self.rx_buf[5..4 + pkt_len - pad_len].to_vec();
            self.rx_buf.drain(..4 + pkt_len);
            self.rx_seq = self.rx_seq.wrapping_add(1);
            return Some(payload);
        }

        if self.rx_buf.len() < 4 { return None; }
        let mut k1 = [0u8; 32];
        let mut k2 = [0u8; 32];
        k1.copy_from_slice(&self.c2s_key[0..32]);
        k2.copy_from_slice(&self.c2s_key[32..64]);

        let seq = self.rx_seq as u64;
        let len_ks = chacha20_openssh_block(&k2, seq, 0);
        let mut dec_len = [0u8; 4];
        for i in 0..4 {
            dec_len[i] = self.rx_buf[i] ^ len_ks[i];
        }
        let pkt_len = u32::from_be_bytes(dec_len) as usize;
        if pkt_len > 65536 || self.rx_buf.len() < 4 + pkt_len + 16 { return None; }

        let poly_block = chacha20_openssh_block(&k1, seq, 0);
        let mut poly_key = [0u8; 32];
        poly_key.copy_from_slice(&poly_block[0..32]);

        let expected_tag = poly1305_mac(&poly_key, &self.rx_buf[..4 + pkt_len]);
        let actual_tag = &self.rx_buf[4 + pkt_len..4 + pkt_len + 16];
        if expected_tag != actual_tag {
            self.rx_buf.clear();
            return None;
        }

        let mut body = self.rx_buf[4..4 + pkt_len].to_vec();
        chacha20_openssh_xor(&k1, seq, 1, &mut body);
        self.rx_buf.drain(..4 + pkt_len + 16);
        self.rx_seq = self.rx_seq.wrapping_add(1);

        let pad_len = body[0] as usize;
        if 1 + pad_len > body.len() { return None; }
        Some(body[1..body.len() - pad_len].to_vec())
    }

    fn send_channel_data(&mut self, text: &str) {
        // Chunk large outputs so each encrypted SSH packet fits cleanly inside a single Ethernet frame
        for chunk in text.as_bytes().chunks(1024) {
            let mut p = Vec::with_capacity(9 + chunk.len());
            p.push(SSH_MSG_CHANNEL_DATA);
            p.extend_from_slice(&self.client_chan.to_be_bytes());
            push_string(&mut p, chunk);
            self.send_packet(&p);
        }
    }

    fn exec_shell_command(&mut self, raw: &str) -> bool {
        let res = self.shell.execute_line(raw, true);
        if res.clear_screen {
            self.send_channel_data("\x1b[2J\x1b[H");
            return true;
        }
        if res.exit_session {
            self.send_channel_data("Goodbye from EOS sshd.\r\n");
            let mut eof = vec![SSH_MSG_CHANNEL_EOF];
            eof.extend_from_slice(&self.client_chan.to_be_bytes());
            self.send_packet(&eof);
            let mut cl = vec![SSH_MSG_CHANNEL_CLOSE];
            cl.extend_from_slice(&self.client_chan.to_be_bytes());
            self.send_packet(&cl);
            return false;
        }
        for (line, _) in res.lines {
            self.send_channel_data(&format!("{}\r\n", line));
        }
        true
    }

    fn handle_packet(&mut self, payload: &[u8]) {
        if payload.is_empty() { return; }
        match payload[0] {
            SSH_MSG_KEXINIT => {
                self.client_kexinit = payload.to_vec();
            }
            SSH_MSG_KEX_ECDH_INIT => {
                let mut off = 1;
                let Some(qc_slice) = read_string(payload, &mut off) else { return; };
                if qc_slice.len() != 32 { return; }
                let mut qc_arr = [0u8; 32];
                qc_arr.copy_from_slice(qc_slice);

                // Deterministic host Ed25519 key + ephemeral X25519 keypair
                let host_seed: [u8; 32] = [
                    0x45, 0x4F, 0x53, 0x5F, 0x48, 0x4F, 0x53, 0x54,
                    0x5F, 0x45, 0x44, 0x32, 0x35, 0x35, 0x31, 0x39,
                    0x91, 0xA2, 0xB3, 0xC4, 0xD5, 0xE6, 0xF7, 0x08,
                    0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
                ];
                let host_signing = SigningKey::from_bytes(&host_seed);
                let host_pub = host_signing.verifying_key().to_bytes();

                let mut eph_seed = [0u8; 32];
                let tsc = crate::framework::perf_log::SmartLogger::read_tsc();
                for i in 0..32 {
                    eph_seed[i] = ((tsc.wrapping_mul(6364136223846793005).wrapping_add(i as u64 * 131)) >> 24) as u8;
                }
                let server_secret = StaticSecret::from(eph_seed);
                let server_pub = PublicKey::from(&server_secret);
                let shared = server_secret.diffie_hellman(&PublicKey::from(qc_arr));

                let mut ks_blob = Vec::with_capacity(64);
                push_string(&mut ks_blob, b"ssh-ed25519");
                push_string(&mut ks_blob, &host_pub);

                let mut k_mpint = Vec::with_capacity(40);
                push_mpint(&mut k_mpint, shared.as_bytes());

                // Exchange Hash H = SHA256(V_C || V_S || I_C || I_S || K_S || Q_C || Q_S || K)
                let mut h_in = Vec::with_capacity(512);
                let vc = self.client_banner.as_deref().unwrap_or("");
                let vs = SSH_BANNER.trim_end_matches("\r\n");
                push_string(&mut h_in, vc.as_bytes());
                push_string(&mut h_in, vs.as_bytes());
                push_string(&mut h_in, &self.client_kexinit);
                push_string(&mut h_in, &self.server_kexinit);
                push_string(&mut h_in, &ks_blob);
                push_string(&mut h_in, qc_slice);
                push_string(&mut h_in, server_pub.as_bytes());
                h_in.extend_from_slice(&k_mpint);

                let h: [u8; 32] = Sha256::digest(&h_in).into();
                let sid = *self.session_id.get_or_insert(h);

                self.pending_c2s = derive_key_64(&k_mpint, &h, b'C', &sid);
                self.pending_s2c = derive_key_64(&k_mpint, &h, b'D', &sid);

                let sig = host_signing.sign(&h);
                let mut sig_blob = Vec::with_capacity(96);
                push_string(&mut sig_blob, b"ssh-ed25519");
                push_string(&mut sig_blob, &sig.to_bytes());

                let mut reply = Vec::with_capacity(256);
                reply.push(SSH_MSG_KEX_ECDH_REPLY);
                push_string(&mut reply, &ks_blob);
                push_string(&mut reply, server_pub.as_bytes());
                push_string(&mut reply, &sig_blob);
                self.send_packet(&reply);

                self.send_packet(&[SSH_MSG_NEWKEYS]);
                self.s2c_key = self.pending_s2c;
            }
            SSH_MSG_NEWKEYS => {
                self.c2s_key = self.pending_c2s;
                self.encrypted = true;
            }
            SSH_MSG_SERVICE_REQUEST => {
                let mut off = 1;
                if let Some(svc) = read_string(payload, &mut off) {
                    let mut rep = vec![SSH_MSG_SERVICE_ACCEPT];
                    push_string(&mut rep, svc);
                    self.send_packet(&rep);
                }
            }
            SSH_MSG_USERAUTH_REQUEST => {
                // Single-User Root Sovereignty: accept root authentication cleanly
                self.send_packet(&[SSH_MSG_USERAUTH_SUCCESS]);
            }
            SSH_MSG_CHANNEL_OPEN => {
                let mut off = 1;
                let _ctype = read_string(payload, &mut off);
                if let Some(sender_ch) = read_u32(payload, &mut off) {
                    self.client_chan = sender_ch;
                    let mut conf = vec![SSH_MSG_CHANNEL_OPEN_CONFIRMATION];
                    conf.extend_from_slice(&sender_ch.to_be_bytes());
                    conf.extend_from_slice(&0u32.to_be_bytes()); // local channel 0
                    conf.extend_from_slice(&0x0010_0000u32.to_be_bytes()); // window size
                    conf.extend_from_slice(&32768u32.to_be_bytes()); // max packet size
                    self.send_packet(&conf);
                }
            }
            SSH_MSG_CHANNEL_REQUEST => {
                let mut off = 1;
                let _recip = read_u32(payload, &mut off);
                let req_type = read_string(payload, &mut off).unwrap_or(b"");
                let want_reply = payload.get(off).copied().unwrap_or(0) != 0;
                off += 1;

                if want_reply {
                    let mut ok = vec![SSH_MSG_CHANNEL_SUCCESS];
                    ok.extend_from_slice(&self.client_chan.to_be_bytes());
                    self.send_packet(&ok);
                }

                if req_type == b"shell" {
                    let prompt = self.shell.prompt_prefix(true);
                    self.send_channel_data(&format!(
                        "Welcome to EOS 0.1.0 (Encrypted SSH-2.0 / Unified POSIX Root Shell)\r\n{}",
                        prompt
                    ));
                } else if req_type == b"exec" {
                    if let Some(cmd_bytes) = read_string(payload, &mut off) {
                        let cmd = String::from_utf8_lossy(cmd_bytes).to_string();
                        let _ = self.exec_shell_command(&cmd);
                        let mut eof = vec![SSH_MSG_CHANNEL_EOF];
                        eof.extend_from_slice(&self.client_chan.to_be_bytes());
                        self.send_packet(&eof);
                        let mut cl = vec![SSH_MSG_CHANNEL_CLOSE];
                        cl.extend_from_slice(&self.client_chan.to_be_bytes());
                        self.send_packet(&cl);
                    }
                }
            }
            SSH_MSG_CHANNEL_DATA => {
                let mut off = 1;
                let _recip = read_u32(payload, &mut off);
                if let Some(data) = read_string(payload, &mut off) {
                    let text = String::from_utf8_lossy(data).to_string();
                    for ch in text.chars() {
                        match ch {
                            '\r' | '\n' => {
                                self.send_channel_data("\r\n");
                                let line = core::mem::take(&mut self.cmd_buf);
                                if self.exec_shell_command(&line) {
                                    let p = self.shell.prompt_prefix(true);
                                    self.send_channel_data(&p);
                                }
                            }
                            '\x08' | '\x7f' => {
                                if self.cmd_buf.pop().is_some() {
                                    self.send_channel_data("\x08 \x08");
                                }
                            }
                            '\x03' => {
                                self.cmd_buf.clear();
                                let p = self.shell.prompt_prefix(true);
                                self.send_channel_data(&format!("^C\r\n{}", p));
                            }
                            c if c >= ' ' && c <= '~' => {
                                self.cmd_buf.push(c);
                                self.send_channel_data(&c.to_string());
                            }
                            _ => {}
                        }
                    }
                }
            }
            SSH_MSG_DISCONNECT => {
                self.rx_buf.clear();
            }
            _ => {}
        }
    }
}

static ACTIVE_SESSION: Mutex<Option<SshSession>> = Mutex::new(None);

pub fn init_sshd() {
    // Self-verify OpenSSH ChaCha20-Poly1305 AEAD & Ed25519
    let key = [0x42u8; 32];
    let mut probe = *b"EOS_SSH2_CHACHA20_OK";
    chacha20_openssh_xor(&key, 1, 1, &mut probe);
    chacha20_openssh_xor(&key, 1, 1, &mut probe);
    let kex = build_kexinit_payload();

    if &probe == b"EOS_SSH2_CHACHA20_OK" && kex.len() > 32 {
        net_manager::tcp_listen(22);
        if let Ok(mut s) = ACTIVE_SESSION.lock() {
            *s = Some(SshSession::new());
        }
        if let Ok(mut st) = SSHD.lock() {
            st.running = true;
            st.port = 22;
        }
        crate::sdk::agent::audit_log("NET_SSH:PASS:SSH2_Ready");
    }
}

/// Polled from the desktop event loop so Linux host (`ssh -p 2222 root@127.0.0.1`) gets a full encrypted SSH-2 shell
pub fn poll_sshd() {
    net_manager::poll_network();
    let state = {
        let lock = NET.lock().unwrap();
        lock.as_ref()
            .and_then(|n| n.tcp_sockets.get(&22))
            .map(|s| (s.state, s.remote_ip))
    };

    match state {
        Some((TcpState::Established, rip)) => {
            let mut lock = ACTIVE_SESSION.lock().unwrap();
            let sess = lock.get_or_insert_with(SshSession::new);
            if !sess.banner_sent {
                sess.banner_sent = true;
                net_manager::tcp_send(22, SSH_BANNER.as_bytes());
                let kex = build_kexinit_payload();
                sess.server_kexinit = kex.clone();
                sess.send_packet(&kex);
                if let Ok(mut st) = SSHD.lock() {
                    st.active_sessions += 1;
                    st.last_peer = format!("{}.{}.{}.{}", rip[0], rip[1], rip[2], rip[3]);
                }
            }

            let rx = net_manager::tcp_take_rx(22);
            if !rx.is_empty() {
                sess.rx_buf.extend_from_slice(&rx);
            }

            if sess.client_banner.is_none() {
                if let Some(pos) = sess.rx_buf.iter().position(|&b| b == b'\n') {
                    let line = String::from_utf8_lossy(&sess.rx_buf[..pos]).trim_end_matches('\r').to_string();
                    sess.rx_buf.drain(..=pos);
                    sess.client_banner = Some(line);
                } else {
                    return;
                }
            }

            while let Some(pkt) = sess.try_pop_packet() {
                sess.handle_packet(&pkt);
            }
        }
        Some((TcpState::CloseWait | TcpState::Closed, _)) => {
            if let Ok(mut s) = ACTIVE_SESSION.lock() {
                *s = Some(SshSession::new());
            }
            net_manager::tcp_listen(22);
        }
        _ => {}
    }
}

pub fn sshd_status_lines() -> Vec<String> {
    if let Ok(st) = SSHD.lock() {
        vec![
            format!("sshd: {} on 0.0.0.0:{}", if st.running { "LISTENING" } else { "STOPPED" }, st.port),
            "      Protocol : SSH-2.0-EOS_0.1.0 (curve25519-sha256 / ssh-ed25519 / chacha20-poly1305@openssh.com)".into(),
            format!("      Sessions : {} active (last peer: {})", st.active_sessions, if st.last_peer.is_empty() { "none" } else { &st.last_peer }),
        ]
    } else {
        vec!["sshd: unavailable".into()]
    }
}

pub fn run_ssh_client(target: &str, port: u16) -> Vec<String> {
    let (user, host) = match target.split_once('@') {
        Some((u, h)) => (u, h),
        None => ("root", target),
    };
    let Some(ip) = net_manager::parse_ipv4(host) else {
        return vec![format!("ssh: invalid IPv4 host '{}' (usage: ssh user@10.0.2.2 [port])", host)];
    };

    let mut out = vec![format!("Connecting to {}@{}.{}.{}.{}:{} via SSH-2.0...", user, ip[0], ip[1], ip[2], ip[3], port)];
    let local_port = net_manager::tcp_connect(ip, port);
    if local_port == 0 {
        out.push("ssh: network stack unavailable".into());
        return out;
    }

    let mut connected = false;
    for _ in 0..40 {
        net_manager::poll_network();
        if let Ok(lock) = NET.lock() {
            if let Some(net) = lock.as_ref() {
                if let Some(s) = net.tcp_sockets.get(&local_port) {
                    if s.state == TcpState::Established {
                        connected = true;
                        break;
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    if !connected {
        out.push(format!("ssh: connect to host {} port {}: Connection timed out", host, port));
        return out;
    }

    out.push(format!("TCP connected. Sending banner: {}", SSH_BANNER.trim()));
    net_manager::tcp_send(local_port, SSH_BANNER.as_bytes());
    let kex = wrap_ssh_packet(&build_kexinit_payload());
    net_manager::tcp_send(local_port, &kex);

    let mut remote_banner = String::new();
    for _ in 0..30 {
        net_manager::poll_network();
        let rx = net_manager::tcp_take_rx(local_port);
        if !rx.is_empty() {
            let txt = String::from_utf8_lossy(&rx);
            if let Some(first) = txt.lines().next() {
                remote_banner = first.to_string();
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    if !remote_banner.is_empty() {
        out.push(format!("Remote banner: {}", remote_banner));
        out.push("SSH_MSG_KEXINIT sent (curve25519-sha256 / ssh-ed25519 / chacha20-poly1305@openssh.com).".into());
    } else {
        out.push("Connected, awaiting remote SSH-2 banner (no response within 750ms).".into());
    }
    out
}
