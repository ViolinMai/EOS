use alloc::vec::Vec;
use crate::{log_info, log_error};
use crate::drivers::e1000::get_driver;

#[derive(Clone, Copy, Debug)]
pub struct MacAddress(pub [u8; 6]);
#[derive(Clone, Copy, Debug)]
pub struct Ipv4Address(pub [u8; 4]);

pub struct NetworkManager {
    pub active: bool,
    pub mac: MacAddress,
    pub ip: Ipv4Address,
    pub gateway: Ipv4Address,
    pub subnet: Ipv4Address,
    pub tx_packets: usize,
    pub rx_packets: usize,
}

static mut NET_MGR_STORAGE: NetworkManager = NetworkManager {
    active: false,
    mac: MacAddress([0; 6]),
    ip: Ipv4Address([10, 0, 2, 15]),
    gateway: Ipv4Address([10, 0, 2, 2]),
    subnet: Ipv4Address([255, 255, 255, 0]),
    tx_packets: 0,
    rx_packets: 0,
};

#[inline(always)]
fn get_mgr_mut() -> &'static mut NetworkManager {
    unsafe { &mut *core::ptr::addr_of_mut!(NET_MGR_STORAGE) }
}

#[inline(always)]
fn get_mgr() -> &'static NetworkManager {
    unsafe { &*core::ptr::addr_of!(NET_MGR_STORAGE) }
}

pub fn init() {
    if let Some(e1k) = get_driver() {
        let mgr = get_mgr_mut();
        mgr.active = true;
        mgr.mac = MacAddress(e1k.read_mac());
        log_info!("NET", "Network Manager Active. Bound to E1000 NIC.");
        log_info!("NET", "MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", 
            mgr.mac.0[0], mgr.mac.0[1], mgr.mac.0[2],
            mgr.mac.0[3], mgr.mac.0[4], mgr.mac.0[5]
        );
    } else {
        log_error!("NET", "No E1000 NIC found. Network disabled.");
    }
}

pub fn poll() {
    if !get_mgr().active { return; }
    if let Some(e1k) = get_driver() {
        while let Some(packet) = e1k.receive_packet() {
            get_mgr_mut().rx_packets += 1;
            handle_ethernet_frame(&packet);
        }
    }
}

pub fn handle_ethernet_frame(frame: &[u8]) {
    if frame.len() < 14 { return; }
    let ethertype = (frame[12] as u16) << 8 | (frame[13] as u16);
    match ethertype {
        0x0806 => handle_arp(&frame[14..]),
        0x0800 => handle_ipv4(&frame[14..]),
        _ => {}
    }
}

pub fn handle_arp(payload: &[u8]) {
    if payload.len() < 28 { return; }
    let opcode = (payload[6] as u16) << 8 | (payload[7] as u16);
    if opcode == 1 {
        let target_ip = [payload[24], payload[25], payload[26], payload[27]];
        if target_ip == get_mgr().ip.0 {
            send_arp_reply(&payload[8..14], &payload[14..18]);
        }
    }
}

pub fn send_arp_reply(target_mac: &[u8], target_ip: &[u8]) {
    let mut frame = alloc::vec![0u8; 42];
    let mgr = get_mgr();
    frame[0..6].copy_from_slice(target_mac);
    frame[6..12].copy_from_slice(&mgr.mac.0);
    frame[12..14].copy_from_slice(&[0x08, 0x06]);

    frame[14..16].copy_from_slice(&[0x00, 0x01]);
    frame[16..18].copy_from_slice(&[0x08, 0x00]);
    frame[18] = 6; frame[19] = 4;
    frame[20..22].copy_from_slice(&[0x00, 0x02]);

    frame[22..28].copy_from_slice(&mgr.mac.0);
    frame[28..32].copy_from_slice(&mgr.ip.0);
    frame[32..38].copy_from_slice(target_mac);
    frame[38..42].copy_from_slice(target_ip);

    if let Some(e1k) = get_driver() {
        e1k.send_packet(&frame);
        get_mgr_mut().tx_packets += 1;
    }
}

pub fn handle_ipv4(_payload: &[u8]) { }

pub fn cmd_ipconfig() {
    let mgr = get_mgr();
    log_info!("NET", "=== EOS NETWORK INTERFACE STATUS ===");
    if mgr.active {
        log_info!("NET", "Interface : eth0 (QEMU Intel E1000)");
        log_info!("NET", "State     : UP");
        log_info!("NET", "MAC Addr  : {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", 
            mgr.mac.0[0], mgr.mac.0[1], mgr.mac.0[2],
            mgr.mac.0[3], mgr.mac.0[4], mgr.mac.0[5]);
        log_info!("NET", "IPv4 Addr : {}.{}.{}.{}", mgr.ip.0[0], mgr.ip.0[1], mgr.ip.0[2], mgr.ip.0[3]);
        log_info!("NET", "Gateway   : {}.{}.{}.{}", mgr.gateway.0[0], mgr.gateway.0[1], mgr.gateway.0[2], mgr.gateway.0[3]);
        log_info!("NET", "Subnet    : {}.{}.{}.{}", mgr.subnet.0[0], mgr.subnet.0[1], mgr.subnet.0[2], mgr.subnet.0[3]);
        log_info!("NET", "Traffic   : RX {} packets | TX {} packets", mgr.rx_packets, mgr.tx_packets);
    } else {
        log_info!("NET", "State     : DOWN / UNINITIALIZED");
    }
}
