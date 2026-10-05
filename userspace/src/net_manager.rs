use std::collections::HashMap;
use std::sync::Mutex;
use crate::syscall::{sys_net_tx, sys_net_rx, sys_net_mac};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    SynSent,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    TimeWait,
}

pub struct TcpSocket {
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
    pub state: TcpState,
    pub seq: u32,
    pub ack: u32,
    pub pending_tx: Vec<u8>,
    pub rx_buffer: Vec<u8>,
}

#[derive(Clone)]
pub struct PendingPacket {
    pub target_ip: [u8; 4],
    pub protocol: u8,
    pub data: Vec<u8>,
}

pub struct NetworkManager {
    pub mac: [u8; 6],
    pub ip: [u8; 4],
    pub gateway: [u8; 4],
    pub dns_server: [u8; 4],
    pub arp_cache: HashMap<[u8; 4], [u8; 6]>,
    pub pending_tx: Vec<PendingPacket>,
    pub tcp_sockets: HashMap<u16, TcpSocket>,
    pub next_port: u16,
    pub ping_replies: Vec<([u8; 4], usize, usize)>,
    pub dns_replies: HashMap<u16, Option<[u8; 4]>>,
}

pub static NET: Mutex<Option<NetworkManager>> = Mutex::new(None);

pub fn init_net(ip: [u8; 4], gateway: [u8; 4]) {
    let mut mac = [0u8; 6];
    sys_net_mac(&mut mac);
    let nm = NetworkManager {
        mac, ip, gateway,
        dns_server: [10, 0, 2, 3],
        arp_cache: HashMap::new(),
        pending_tx: Vec::new(),
        tcp_sockets: HashMap::new(),
        next_port: 49152,
        ping_replies: Vec::new(),
        dns_replies: HashMap::new(),
    };
    nm.send_arp_request(gateway);
    *NET.lock().unwrap() = Some(nm);
}

pub fn poll_network() {
    let mut lock = NET.lock().unwrap();
    if let Some(net) = lock.as_mut() {
        net.poll();
    }
}

fn calc_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = ((data[i] as u32) << 8) | (data[i + 1] as u32);
        sum = sum.wrapping_add(word);
        i += 2;
    }
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

fn calc_tcp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], tcp_segment: &[u8]) -> u16 {
    let mut pseudo = Vec::with_capacity(12 + tcp_segment.len());
    pseudo.extend_from_slice(&src_ip);
    pseudo.extend_from_slice(&dst_ip);
    pseudo.push(0);
    pseudo.push(6);
    let len = tcp_segment.len() as u16;
    pseudo.push((len >> 8) as u8);
    pseudo.push((len & 0xFF) as u8);
    pseudo.extend_from_slice(tcp_segment);
    calc_checksum(&pseudo)
}

fn build_tcp_packet(src_ip: [u8; 4], dst_ip: [u8; 4], src_port: u16, dst_port: u16, seq: u32, ack: u32, flags: u8, data: &[u8]) -> Vec<u8> {
    let mut pkt = vec![
        (src_port >> 8) as u8, (src_port & 0xFF) as u8,
        (dst_port >> 8) as u8, (dst_port & 0xFF) as u8,
        (seq >> 24) as u8, (seq >> 16) as u8, (seq >> 8) as u8, (seq & 0xFF) as u8,
        (ack >> 24) as u8, (ack >> 16) as u8, (ack >> 8) as u8, (ack & 0xFF) as u8,
        0x50, flags, 0xFF, 0xFF,
        0x00, 0x00, 0x00, 0x00,
    ];
    pkt.extend_from_slice(data);
    let csum = calc_tcp_checksum(src_ip, dst_ip, &pkt);
    pkt[16] = (csum >> 8) as u8;
    pkt[17] = (csum & 0xFF) as u8;
    pkt
}

impl NetworkManager {
    pub fn poll(&mut self) {
        let mut buf = vec![0u8; 2048];
        loop {
            let len = sys_net_rx(&mut buf);
            if len == 0 { break; }
            self.handle_frame(&buf[..len]);
        }
    }

    fn handle_frame(&mut self, frame: &[u8]) {
        if frame.len() < 14 { return; }
        let ethertype = ((frame[12] as u16) << 8) | (frame[13] as u16);
        match ethertype {
            0x0806 => self.handle_arp(&frame[14..]),
            0x0800 => self.handle_ipv4(&frame[14..]),
            _ => {}
        }
    }

    fn handle_arp(&mut self, payload: &[u8]) {
        if payload.len() < 28 { return; }
        let sender_mac = [payload[8], payload[9], payload[10], payload[11], payload[12], payload[13]];
        let sender_ip = [payload[14], payload[15], payload[16], payload[17]];
        self.arp_cache.insert(sender_ip, sender_mac);

        let op = ((payload[6] as u16) << 8) | (payload[7] as u16);
        if op == 1 {
            let target_ip = [payload[24], payload[25], payload[26], payload[27]];
            if target_ip == self.ip {
                self.send_arp_reply(sender_mac, sender_ip);
            }
        }

        let mut remaining = Vec::new();
        let pending = core::mem::take(&mut self.pending_tx);
        for pkt in pending {
            if pkt.target_ip == sender_ip || (self.arp_cache.contains_key(&self.gateway)) {
                self.send_ipv4(pkt.target_ip, pkt.protocol, &pkt.data);
            } else {
                remaining.push(pkt);
            }
        }
        self.pending_tx = remaining;
    }

    pub fn send_arp_request(&self, target_ip: [u8; 4]) {
        let mut frame = Vec::with_capacity(42);
        frame.extend_from_slice(&[0xFF; 6]);
        frame.extend_from_slice(&self.mac);
        frame.extend_from_slice(&[0x08, 0x06]);
        frame.extend_from_slice(&[0x00, 0x01, 0x08, 0x00, 6, 4, 0x00, 0x01]);
        frame.extend_from_slice(&self.mac);
        frame.extend_from_slice(&self.ip);
        frame.extend_from_slice(&[0x00; 6]);
        frame.extend_from_slice(&target_ip);
        sys_net_tx(&frame);
    }

    fn send_arp_reply(&self, target_mac: [u8; 6], target_ip: [u8; 4]) {
        let mut frame = Vec::with_capacity(42);
        frame.extend_from_slice(&target_mac);
        frame.extend_from_slice(&self.mac);
        frame.extend_from_slice(&[0x08, 0x06]);
        frame.extend_from_slice(&[0x00, 0x01, 0x08, 0x00, 6, 4, 0x00, 0x02]);
        frame.extend_from_slice(&self.mac);
        frame.extend_from_slice(&self.ip);
        frame.extend_from_slice(&target_mac);
        frame.extend_from_slice(&target_ip);
        sys_net_tx(&frame);
    }

    fn handle_ipv4(&mut self, payload: &[u8]) {
        if payload.len() < 20 { return; }
        let ihl = (payload[0] & 0x0F) as usize * 4;
        let total_len = (((payload[2] as u16) << 8) | (payload[3] as u16)) as usize;

        if payload.len() < ihl || payload.len() < total_len || total_len < ihl { return; }

        let ip_packet = &payload[..total_len];
        let protocol = ip_packet[9];
        let src_ip = [ip_packet[12], ip_packet[13], ip_packet[14], ip_packet[15]];
        let dst_ip = [ip_packet[16], ip_packet[17], ip_packet[18], ip_packet[19]];

        if dst_ip != self.ip && dst_ip != [255, 255, 255, 255] { return; }

        let transport_data = &ip_packet[ihl..];
        match protocol {
            1 => self.handle_icmp(src_ip, transport_data),
            6 => self.handle_tcp(src_ip, transport_data),
            17 => self.handle_udp(src_ip, transport_data),
            _ => {}
        }
    }

    fn handle_icmp(&mut self, src_ip: [u8; 4], icmp: &[u8]) {
        if icmp.len() < 8 { return; }
        let icmp_type = icmp[0];
        if icmp_type == 0 {
            let seq = ((icmp[6] as u16) << 8) | (icmp[7] as u16);
            self.ping_replies.push((src_ip, seq as usize, icmp.len()));
        } else if icmp_type == 8 {
            let mut reply = icmp.to_vec();
            reply[0] = 0;
            reply[2] = 0;
            reply[3] = 0;
            let csum = calc_checksum(&reply);
            reply[2] = (csum >> 8) as u8;
            reply[3] = (csum & 0xFF) as u8;
            self.send_ipv4(src_ip, 1, &reply);
        }
    }

    fn handle_udp(&mut self, _src_ip: [u8; 4], udp: &[u8]) {
        if udp.len() < 8 { return; }
        let src_port = ((udp[0] as u16) << 8) | (udp[1] as u16);
        let payload = &udp[8..];
        if src_port == 53 {
            self.handle_dns_response(payload);
        }
    }

    fn handle_tcp(&mut self, src_ip: [u8; 4], tcp: &[u8]) {
        if tcp.len() < 20 { return; }
        let dst_port = ((tcp[2] as u16) << 8) | (tcp[3] as u16);
        let seq = u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]);
        let ack = u32::from_be_bytes([tcp[8], tcp[9], tcp[10], tcp[11]]);
        let data_offset = ((tcp[12] >> 4) as usize) * 4;
        let flags = tcp[13];

        let mut packets_to_send: Vec<Vec<u8>> = Vec::new();
        let my_ip = self.ip;

        if let Some(sock) = self.tcp_sockets.get_mut(&dst_port) {
            if sock.remote_ip != src_ip { return; }

            let syn = (flags & 0x02) != 0;
            let ack_flag = (flags & 0x10) != 0;
            let fin = (flags & 0x01) != 0;

            match sock.state {
                TcpState::SynSent => {
                    if syn && ack_flag {
                        sock.ack = seq.wrapping_add(1);
                        sock.seq = ack;
                        sock.state = TcpState::Established;

                        let empty = [];
                        let p = build_tcp_packet(my_ip, sock.remote_ip, sock.local_port, sock.remote_port, sock.seq, sock.ack, 0x10, &empty);
                        packets_to_send.push(p);

                        if !sock.pending_tx.is_empty() {
                            let data = core::mem::take(&mut sock.pending_tx);
                            let p_data = build_tcp_packet(my_ip, sock.remote_ip, sock.local_port, sock.remote_port, sock.seq, sock.ack, 0x18, &data);
                            sock.seq = sock.seq.wrapping_add(data.len() as u32);
                            packets_to_send.push(p_data);
                        }
                    }
                }
                TcpState::Established => {
                    let payload = if tcp.len() > data_offset { &tcp[data_offset..] } else { &[] };
                    if !payload.is_empty() {
                        sock.rx_buffer.extend_from_slice(payload);
                        sock.ack = seq.wrapping_add(payload.len() as u32);
                        let empty = [];
                        let p = build_tcp_packet(my_ip, sock.remote_ip, sock.local_port, sock.remote_port, sock.seq, sock.ack, 0x10, &empty);
                        packets_to_send.push(p);
                    }
                    if fin {
                        sock.ack = seq.wrapping_add(1);
                        sock.state = TcpState::CloseWait;
                        let empty = [];
                        let p = build_tcp_packet(my_ip, sock.remote_ip, sock.local_port, sock.remote_port, sock.seq, sock.ack, 0x11, &empty);
                        packets_to_send.push(p);
                    }
                }
                _ => {}
            }
        }

        for pkt in packets_to_send {
            self.send_ipv4(src_ip, 6, &pkt);
        }
    }

    fn handle_dns_response(&mut self, payload: &[u8]) {
        if payload.len() < 12 { return; }
        let id = ((payload[0] as u16) << 8) | (payload[1] as u16);
        let flags = ((payload[2] as u16) << 8) | (payload[3] as u16);
        let rcode = flags & 0x0F;
        let ancount = ((payload[6] as u16) << 8) | (payload[7] as u16);

        if rcode != 0 || ancount == 0 {
            self.dns_replies.insert(id, None);
            return;
        }

        let mut idx = 12;
        while idx < payload.len() {
            let len = payload[idx] as usize;
            if len == 0 { idx += 5; break; }
            idx += len + 1;
        }

        for _ in 0..ancount {
            if idx >= payload.len() { break; }
            if (payload[idx] & 0xC0) == 0xC0 {
                idx += 2;
            } else {
                while idx < payload.len() && payload[idx] != 0 { idx += payload[idx] as usize + 1; }
                idx += 1;
            }
            if idx + 10 > payload.len() { break; }
            let atype = ((payload[idx] as u16) << 8) | (payload[idx + 1] as u16);
            let rdlength = (((payload[idx + 8] as u16) << 8) | (payload[idx + 9] as u16)) as usize;
            idx += 10;
            if atype == 1 && rdlength == 4 && idx + 4 <= payload.len() {
                let ip = [payload[idx], payload[idx + 1], payload[idx + 2], payload[idx + 3]];
                self.dns_replies.insert(id, Some(ip));
                return;
            }
            idx += rdlength;
        }
        self.dns_replies.insert(id, None);
    }

    pub fn send_ipv4(&mut self, dest_ip: [u8; 4], protocol: u8, data: &[u8]) {
        let dest_mac = match self.arp_cache.get(&dest_ip) {
            Some(&m) => m,
            None => {
                match self.arp_cache.get(&self.gateway) {
                    Some(&gm) => gm,
                    None => {
                        self.send_arp_request(dest_ip);
                        self.send_arp_request(self.gateway);
                        self.pending_tx.push(PendingPacket { target_ip: dest_ip, protocol, data: data.to_vec() });
                        return;
                    }
                }
            }
        };

        let total_len = (20 + data.len()) as u16;
        let mut ip_hdr = vec![
            0x45, 0x00, (total_len >> 8) as u8, (total_len & 0xFF) as u8,
            0x1C, 0x24, 0x40, 0x00,
            64, protocol, 0x00, 0x00,
            self.ip[0], self.ip[1], self.ip[2], self.ip[3],
            dest_ip[0], dest_ip[1], dest_ip[2], dest_ip[3],
        ];
        let csum = calc_checksum(&ip_hdr);
        ip_hdr[10] = (csum >> 8) as u8;
        ip_hdr[11] = (csum & 0xFF) as u8;

        let mut frame = Vec::with_capacity(14 + 20 + data.len());
        frame.extend_from_slice(&dest_mac);
        frame.extend_from_slice(&self.mac);
        frame.extend_from_slice(&[0x08, 0x00]);
        frame.extend_from_slice(&ip_hdr);
        frame.extend_from_slice(data);

        sys_net_tx(&frame);
    }

    pub fn send_udp(&mut self, dest_ip: [u8; 4], src_port: u16, dst_port: u16, data: &[u8]) {
        let udp_len = (8 + data.len()) as u16;
        let mut udp_hdr = vec![
            (src_port >> 8) as u8, (src_port & 0xFF) as u8,
            (dst_port >> 8) as u8, (dst_port & 0xFF) as u8,
            (udp_len >> 8) as u8, (udp_len & 0xFF) as u8,
            0x00, 0x00,
        ];
        udp_hdr.extend_from_slice(data);
        self.send_ipv4(dest_ip, 17, &udp_hdr);
    }

    pub fn resolve_dns(&mut self, host: &str, query_id: u16) {
        let mut packet = vec![
            (query_id >> 8) as u8, (query_id & 0xFF) as u8,
            0x01, 0x00,
            0x00, 0x01,
            0x00, 0x00,
            0x00, 0x00,
            0x00, 0x00,
        ];

        for part in host.split('.') {
            if part.is_empty() { continue; }
            packet.push(part.len() as u8);
            packet.extend_from_slice(part.as_bytes());
        }
        packet.push(0);
        packet.extend_from_slice(&[0x00, 0x01]);
        packet.extend_from_slice(&[0x00, 0x01]);

        let src_port = self.alloc_ephemeral_port();
        self.send_udp(self.dns_server, src_port, 53, &packet);
    }

    pub fn alloc_ephemeral_port(&mut self) -> u16 {
        let port = self.next_port;
        self.next_port = if self.next_port >= 65530 { 49152 } else { self.next_port + 1 };
        port
    }

    pub fn ping(&mut self, target_ip: [u8; 4], seq: u16) {
        let mut icmp = vec![
            8, 0, 0, 0,
            0x00, 0x01, (seq >> 8) as u8, (seq & 0xFF) as u8,
            b'E', b'O', b'S', b'_', b'P', b'I', b'N', b'G',
        ];
        let csum = calc_checksum(&icmp);
        icmp[2] = (csum >> 8) as u8;
        icmp[3] = (csum & 0xFF) as u8;
        self.send_ipv4(target_ip, 1, &icmp);
    }

    pub fn http_get(&mut self, target_ip: [u8; 4], host: &str, path: &str) -> u16 {
        let port = self.alloc_ephemeral_port();
        let request = format!("GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: EOSBrowser/1.0\r\nConnection: close\r\n\r\n", path, host);
        let isn = 1000u32;
        let my_ip = self.ip;

        self.tcp_sockets.insert(port, TcpSocket {
            local_port: port,
            remote_ip: target_ip,
            remote_port: 80,
            state: TcpState::SynSent,
            seq: isn,
            ack: 0,
            pending_tx: request.into_bytes(),
            rx_buffer: Vec::new(),
        });

        let empty = [];
        let syn_pkt = build_tcp_packet(my_ip, target_ip, port, 80, isn, 0, 0x02, &empty);
        self.send_ipv4(target_ip, 6, &syn_pkt);

        port
    }
}
