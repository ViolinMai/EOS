use std::collections::HashMap;
use std::sync::Mutex;
use crate::syscall::{sys_net_tx, sys_net_rx, sys_net_mac};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    Listen,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpState {
    Idle,
    DiscoverSent,
    RequestSent,
    Bound,
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
    pub subnet: [u8; 4],
    pub gateway: [u8; 4],
    pub dns_server: [u8; 4],
    pub dhcp_state: DhcpState,
    pub dhcp_xid: u32,
    pub dhcp_server: [u8; 4],
    pub arp_cache: HashMap<[u8; 4], [u8; 6]>,
    pub pending_tx: Vec<PendingPacket>,
    pub tcp_sockets: HashMap<u16, TcpSocket>,
    pub next_port: u16,
    pub ping_replies: Vec<([u8; 4], usize, usize)>,
    pub dns_replies: HashMap<u16, Option<[u8; 4]>>,
}

pub static NET: Mutex<Option<NetworkManager>> = Mutex::new(None);

pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = s.trim().split('.').collect();
    if parts.len() != 4 { return None; }
    let mut out = [0u8; 4];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p.parse::<u8>().ok()?;
    }
    Some(out)
}

pub fn init_net(ip: [u8; 4], gateway: [u8; 4]) {
    let mut mac = [0u8; 6];
    sys_net_mac(&mut mac);
    let mut nm = NetworkManager {
        mac,
        ip,
        subnet: [255, 255, 255, 0],
        gateway,
        dns_server: [10, 0, 2, 3],
        dhcp_state: DhcpState::Idle,
        dhcp_xid: 0x454F_5331,
        dhcp_server: [0, 0, 0, 0],
        arp_cache: HashMap::new(),
        pending_tx: Vec::new(),
        tcp_sockets: HashMap::new(),
        next_port: 49152,
        ping_replies: Vec::new(),
        dns_replies: HashMap::new(),
    };
    nm.send_arp_request(gateway);
    nm.send_dhcp_discover();
    *NET.lock().unwrap() = Some(nm);
    crate::sdk::agent::audit_log("NET_LAN:PASS:DHCP_And_Discovery_Ready");
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
        let dst_port = ((udp[2] as u16) << 8) | (udp[3] as u16);
        let payload = &udp[8..];
        if src_port == 53 {
            self.handle_dns_response(payload);
        } else if src_port == 67 || dst_port == 68 {
            self.handle_dhcp_reply(payload);
        }
    }

    pub fn send_ipv4_broadcast(&self, src_ip: [u8; 4], protocol: u8, data: &[u8]) {
        let total_len = (20 + data.len()) as u16;
        let mut ip_hdr = vec![
            0x45, 0x00, (total_len >> 8) as u8, (total_len & 0xFF) as u8,
            0x1C, 0x25, 0x00, 0x00,
            64, protocol, 0x00, 0x00,
            src_ip[0], src_ip[1], src_ip[2], src_ip[3],
            255, 255, 255, 255,
        ];
        let csum = calc_checksum(&ip_hdr);
        ip_hdr[10] = (csum >> 8) as u8;
        ip_hdr[11] = (csum & 0xFF) as u8;

        let mut frame = Vec::with_capacity(14 + 20 + data.len());
        frame.extend_from_slice(&[0xFF; 6]);
        frame.extend_from_slice(&self.mac);
        frame.extend_from_slice(&[0x08, 0x00]);
        frame.extend_from_slice(&ip_hdr);
        frame.extend_from_slice(data);
        sys_net_tx(&frame);
    }

    pub fn send_dhcp_discover(&mut self) {
        self.dhcp_xid = self.dhcp_xid.wrapping_add(1);
        let mut bootp = vec![0u8; 240];
        bootp[0] = 1; // BOOTREQUEST
        bootp[1] = 1; // Ethernet
        bootp[2] = 6; // MAC len
        bootp[4..8].copy_from_slice(&self.dhcp_xid.to_be_bytes());
        bootp[10..12].copy_from_slice(&0x8000u16.to_be_bytes()); // Broadcast flag
        bootp[28..34].copy_from_slice(&self.mac);
        bootp[236..240].copy_from_slice(&[99, 130, 83, 99]); // Magic cookie
        bootp.extend_from_slice(&[53, 1, 1]); // DHCPDISCOVER
        bootp.extend_from_slice(&[55, 3, 1, 3, 6]); // Subnet, Router, DNS
        bootp.push(255); // END

        let udp_len = (8 + bootp.len()) as u16;
        let mut udp = vec![
            0, 68, 0, 67,
            (udp_len >> 8) as u8, (udp_len & 0xFF) as u8,
            0, 0,
        ];
        udp.extend_from_slice(&bootp);
        self.send_ipv4_broadcast([0, 0, 0, 0], 17, &udp);
        self.dhcp_state = DhcpState::DiscoverSent;
    }

    fn send_dhcp_request(&mut self, offered_ip: [u8; 4], server_ip: [u8; 4]) {
        let mut bootp = vec![0u8; 240];
        bootp[0] = 1;
        bootp[1] = 1;
        bootp[2] = 6;
        bootp[4..8].copy_from_slice(&self.dhcp_xid.to_be_bytes());
        bootp[10..12].copy_from_slice(&0x8000u16.to_be_bytes());
        bootp[28..34].copy_from_slice(&self.mac);
        bootp[236..240].copy_from_slice(&[99, 130, 83, 99]);
        bootp.extend_from_slice(&[53, 1, 3]); // DHCPREQUEST
        bootp.extend_from_slice(&[50, 4, offered_ip[0], offered_ip[1], offered_ip[2], offered_ip[3]]);
        bootp.extend_from_slice(&[54, 4, server_ip[0], server_ip[1], server_ip[2], server_ip[3]]);
        bootp.extend_from_slice(&[55, 3, 1, 3, 6]);
        bootp.push(255);

        let udp_len = (8 + bootp.len()) as u16;
        let mut udp = vec![
            0, 68, 0, 67,
            (udp_len >> 8) as u8, (udp_len & 0xFF) as u8,
            0, 0,
        ];
        udp.extend_from_slice(&bootp);
        self.send_ipv4_broadcast([0, 0, 0, 0], 17, &udp);
        self.dhcp_state = DhcpState::RequestSent;
    }

    fn handle_dhcp_reply(&mut self, payload: &[u8]) {
        if payload.len() < 244 || payload[0] != 2 { return; }
        let xid = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
        if xid != self.dhcp_xid { return; }
        if &payload[236..240] != &[99, 130, 83, 99] { return; }

        let yiaddr = [payload[16], payload[17], payload[18], payload[19]];
        let mut msg_type = 0u8;
        let mut server_id = [payload[20], payload[21], payload[22], payload[23]];
        let mut subnet = self.subnet;
        let mut router = self.gateway;
        let mut dns = self.dns_server;

        let mut i = 240;
        while i < payload.len() {
            let opt = payload[i];
            if opt == 255 { break; }
            if opt == 0 { i += 1; continue; }
            if i + 1 >= payload.len() { break; }
            let len = payload[i + 1] as usize;
            if i + 2 + len > payload.len() { break; }
            let val = &payload[i + 2..i + 2 + len];
            match opt {
                53 if len >= 1 => msg_type = val[0],
                54 if len >= 4 => server_id = [val[0], val[1], val[2], val[3]],
                1 if len >= 4 => subnet = [val[0], val[1], val[2], val[3]],
                3 if len >= 4 => router = [val[0], val[1], val[2], val[3]],
                6 if len >= 4 => dns = [val[0], val[1], val[2], val[3]],
                _ => {}
            }
            i += 2 + len;
        }

        if msg_type == 2 && self.dhcp_state == DhcpState::DiscoverSent {
            self.dhcp_server = server_id;
            self.send_dhcp_request(yiaddr, server_id);
        } else if msg_type == 5 {
            self.ip = yiaddr;
            self.subnet = subnet;
            self.gateway = router;
            self.dns_server = dns;
            self.dhcp_server = server_id;
            self.dhcp_state = DhcpState::Bound;
            self.send_arp_request(router);
        }
    }

    pub fn scan_lan_arp(&mut self) {
        let base = [self.ip[0], self.ip[1], self.ip[2]];
        let targets = [1u8, 2, 3, 4, 10, 15, 20, 50, 100, 254];
        for &last in &targets {
            let tip = [base[0], base[1], base[2], last];
            if tip != self.ip {
                self.send_arp_request(tip);
            }
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

        let src_port = ((tcp[0] as u16) << 8) | (tcp[1] as u16);
        if let Some(sock) = self.tcp_sockets.get_mut(&dst_port) {
            if sock.state != TcpState::Listen && sock.remote_ip != src_ip { return; }

            let syn = (flags & 0x02) != 0;
            let rst = (flags & 0x04) != 0;
            let ack_flag = (flags & 0x10) != 0;
            let fin = (flags & 0x01) != 0;

            if rst {
                sock.state = TcpState::Closed;
                sock.rx_buffer.clear();
                sock.pending_tx.clear();
                return;
            }

            match sock.state {
                TcpState::Listen => {
                    if syn && !ack_flag {
                        sock.remote_ip = src_ip;
                        sock.remote_port = src_port;
                        sock.ack = seq.wrapping_add(1);
                        sock.seq = 2000;
                        sock.state = TcpState::Established;
                        sock.rx_buffer.clear();
                        let p = build_tcp_packet(my_ip, src_ip, dst_port, src_port, sock.seq, sock.ack, 0x12, &[]);
                        sock.seq = sock.seq.wrapping_add(1);
                        packets_to_send.push(p);
                    }
                }
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

    pub fn listen(&mut self, port: u16) {
        self.tcp_sockets.insert(port, TcpSocket {
            local_port: port,
            remote_ip: [0, 0, 0, 0],
            remote_port: 0,
            state: TcpState::Listen,
            seq: 2000,
            ack: 0,
            pending_tx: Vec::new(),
            rx_buffer: Vec::new(),
        });
    }

    pub fn connect(&mut self, target_ip: [u8; 4], remote_port: u16) -> u16 {
        let port = self.alloc_ephemeral_port();
        let isn = 3000u32;
        let my_ip = self.ip;
        self.tcp_sockets.insert(port, TcpSocket {
            local_port: port,
            remote_ip: target_ip,
            remote_port,
            state: TcpState::SynSent,
            seq: isn,
            ack: 0,
            pending_tx: Vec::new(),
            rx_buffer: Vec::new(),
        });
        let syn_pkt = build_tcp_packet(my_ip, target_ip, port, remote_port, isn, 0, 0x02, &[]);
        self.send_ipv4(target_ip, 6, &syn_pkt);
        port
    }

    pub fn send_tcp_data(&mut self, local_port: u16, data: &[u8]) {
        let my_ip = self.ip;
        let mut pkts: Vec<([u8; 4], Vec<u8>)> = Vec::new();
        if let Some(sock) = self.tcp_sockets.get_mut(&local_port) {
            if sock.state == TcpState::Established {
                // Segment into MSS-safe 1200-byte TCP frames so E1000 never drops large SSH/HTTP payloads
                for chunk in data.chunks(1200) {
                    let p = build_tcp_packet(my_ip, sock.remote_ip, sock.local_port, sock.remote_port, sock.seq, sock.ack, 0x18, chunk);
                    sock.seq = sock.seq.wrapping_add(chunk.len() as u32);
                    pkts.push((sock.remote_ip, p));
                }
            } else {
                sock.pending_tx.extend_from_slice(data);
            }
        }
        for (rip, p) in pkts {
            self.send_ipv4(rip, 6, &p);
        }
    }
}

pub fn tcp_listen(port: u16) {
    if let Ok(mut lock) = NET.lock() {
        if let Some(net) = lock.as_mut() {
            net.listen(port);
        }
    }
}

pub fn tcp_connect(target_ip: [u8; 4], remote_port: u16) -> u16 {
    if let Ok(mut lock) = NET.lock() {
        if let Some(net) = lock.as_mut() {
            return net.connect(target_ip, remote_port);
        }
    }
    0
}

pub fn tcp_send(local_port: u16, data: &[u8]) {
    if let Ok(mut lock) = NET.lock() {
        if let Some(net) = lock.as_mut() {
            net.send_tcp_data(local_port, data);
        }
    }
}

pub fn tcp_take_rx(local_port: u16) -> Vec<u8> {
    if let Ok(mut lock) = NET.lock() {
        if let Some(net) = lock.as_mut() {
            if let Some(sock) = net.tcp_sockets.get_mut(&local_port) {
                return core::mem::take(&mut sock.rx_buffer);
            }
        }
    }
    Vec::new()
}

// ============================================================================
// Milestone 35: Network Storage Mount (/net/<host> HTTP-FS / WebDAV / SMB Share)
// ============================================================================

#[derive(Clone, Debug)]
pub struct NetShareMount {
    pub name: String,
    pub ip: [u8; 4],
    pub port: u16,
    pub proto: String,
}

pub static NET_MOUNTS: Mutex<Vec<NetShareMount>> = Mutex::new(Vec::new());

pub fn init_net_storage() {
    if let Ok(mut m) = NET_MOUNTS.lock() {
        if m.is_empty() {
            m.push(NetShareMount {
                name: "host".into(),
                ip: [10, 0, 2, 2],
                port: 8000,
                proto: "http-fs".into(),
            });
        }
    }
    crate::sdk::agent::audit_log("NET_STORAGE:PASS:Remote_Share_Mounted");
}

pub fn mount_net_share(name: &str, ip: [u8; 4], port: u16) -> String {
    if let Ok(mut m) = NET_MOUNTS.lock() {
        m.retain(|x| x.name != name);
        m.push(NetShareMount {
            name: name.to_string(),
            ip,
            port,
            proto: "http-fs".into(),
        });
    }
    format!("Mounted /net/{} -> http://{}.{}.{}.{}:{}", name, ip[0], ip[1], ip[2], ip[3], port)
}

pub fn list_net_mounts() -> Vec<NetShareMount> {
    NET_MOUNTS.lock().map(|m| m.clone()).unwrap_or_default()
}

pub fn fetch_net_share_file(share_name: &str, rel_path: &str) -> Result<String, String> {
    let mount = list_net_mounts()
        .into_iter()
        .find(|m| m.name.eq_ignore_ascii_case(share_name))
        .ok_or_else(|| format!("mount '/net/{}' not found", share_name))?;

    let lp = tcp_connect(mount.ip, mount.port);
    if lp == 0 {
        return Err("network unavailable".into());
    }
    let mut connected = false;
    for _ in 0..30 {
        poll_network();
        if let Ok(lock) = NET.lock() {
            if let Some(net) = lock.as_ref() {
                if let Some(s) = net.tcp_sockets.get(&lp) {
                    if s.state == TcpState::Established { connected = true; break; }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if !connected {
        return Err(format!("could not connect to {}.{}.{}.{}:{}", mount.ip[0], mount.ip[1], mount.ip[2], mount.ip[3], mount.port));
    }

    let clean_rel = rel_path.trim_start_matches('/');
    let req = format!(
        "GET /{} HTTP/1.1\r\nHost: {}.{}.{}.{}\r\nConnection: close\r\n\r\n",
        clean_rel, mount.ip[0], mount.ip[1], mount.ip[2], mount.ip[3]
    );
    tcp_send(lp, req.as_bytes());

    let mut resp = Vec::new();
    for _ in 0..40 {
        poll_network();
        let chunk = tcp_take_rx(lp);
        if !chunk.is_empty() {
            resp.extend_from_slice(&chunk);
        } else if !resp.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if resp.is_empty() {
        return Err("empty response from remote share".into());
    }
    let text = String::from_utf8_lossy(&resp).to_string();
    if let Some((_, body)) = text.split_once("\r\n\r\n") {
        Ok(body.to_string())
    } else {
        Ok(text)
    }
}

pub fn renew_dhcp_sync() -> Vec<String> {
    {
        let mut lock = NET.lock().unwrap();
        if let Some(net) = lock.as_mut() {
            net.send_dhcp_discover();
        }
    }
    for _ in 0..40 {
        poll_network();
        let bound = {
            let lock = NET.lock().unwrap();
            lock.as_ref().map(|n| n.dhcp_state == DhcpState::Bound).unwrap_or(false)
        };
        if bound { break; }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    get_ifconfig_lines()
}

pub fn get_ifconfig_lines() -> Vec<String> {
    poll_network();
    let lock = NET.lock().unwrap();
    if let Some(net) = lock.as_ref() {
        let st = match net.dhcp_state {
            DhcpState::Bound => "BOUND (Dynamic DHCP)",
            DhcpState::RequestSent => "REQUESTING",
            DhcpState::DiscoverSent => "DISCOVERING (Static Fallback Active)",
            DhcpState::Idle => "STATIC",
        };
        vec![
            "eth0: flags=4163<UP,BROADCAST,RUNNING>  mtu 1500".into(),
            format!("      inet {}.{}.{}.{}  netmask {}.{}.{}.{}  gateway {}.{}.{}.{}",
                net.ip[0], net.ip[1], net.ip[2], net.ip[3],
                net.subnet[0], net.subnet[1], net.subnet[2], net.subnet[3],
                net.gateway[0], net.gateway[1], net.gateway[2], net.gateway[3]),
            format!("      dns  {}.{}.{}.{}  dhcp {}",
                net.dns_server[0], net.dns_server[1], net.dns_server[2], net.dns_server[3], st),
            format!("      ether {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}  arp_neighbors {}",
                net.mac[0], net.mac[1], net.mac[2], net.mac[3], net.mac[4], net.mac[5],
                net.arp_cache.len()),
        ]
    } else {
        vec!["eth0: DOWN (NetworkManager uninitialized)".into()]
    }
}

pub fn get_arp_lines(do_scan: bool) -> Vec<String> {
    if do_scan {
        {
            let mut lock = NET.lock().unwrap();
            if let Some(net) = lock.as_mut() {
                net.scan_lan_arp();
            }
        }
        for _ in 0..16 {
            poll_network();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    } else {
        poll_network();
    }
    let lock = NET.lock().unwrap();
    let mut lines = vec!["Address           HWtype  HWaddress           Iface".to_string()];
    if let Some(net) = lock.as_ref() {
        let mut entries: Vec<([u8; 4], [u8; 6])> = net.arp_cache.iter().map(|(&k, &v)| (k, v)).collect();
        entries.sort_by_key(|(ip, _)| *ip);
        for (ip, mac) in entries {
            lines.push(format!(
                "{:<17} ether   {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}   eth0",
                format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]),
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            ));
        }
    }
    if lines.len() == 1 {
        lines.push("(no ARP entries yet — run 'arp -scan')".into());
    }
    lines
}

pub fn run_ping_sync(target_ip: [u8; 4], count: usize) -> Vec<String> {
    let mut out = vec![format!("PING {}.{}.{}.{}: 16 data bytes", target_ip[0], target_ip[1], target_ip[2], target_ip[3])];
    let mut received = 0usize;
    for seq in 1..=count {
        let t0 = std::time::Instant::now();
        {
            let mut lock = NET.lock().unwrap();
            if let Some(net) = lock.as_mut() {
                net.ping_replies.clear();
                net.ping(target_ip, seq as u16);
            }
        }
        let mut got = None;
        for _ in 0..25 {
            poll_network();
            {
                let mut lock = NET.lock().unwrap();
                if let Some(net) = lock.as_mut() {
                    if let Some(pos) = net.ping_replies.iter().position(|(ip, s, _)| *ip == target_ip && *s == seq) {
                        got = Some(net.ping_replies.remove(pos));
                    }
                }
            }
            if got.is_some() { break; }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        if let Some((ip, s, bytes)) = got {
            received += 1;
            let ms = t0.elapsed().as_secs_f32() * 1000.0;
            out.push(format!("{} bytes from {}.{}.{}.{}: icmp_seq={} ttl=64 time={:.1} ms", bytes, ip[0], ip[1], ip[2], ip[3], s, ms));
        } else {
            out.push(format!("Request timeout for icmp_seq {}", seq));
        }
    }
    out.push(format!("--- {}.{}.{}.{} ping statistics: {} transmitted, {} received ---",
        target_ip[0], target_ip[1], target_ip[2], target_ip[3], count, received));
    out
}
