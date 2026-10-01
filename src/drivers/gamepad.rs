use crate::input::{push_event, InputEvent, NavAction};
use core::sync::atomic::{AtomicU64, Ordering};

pub static RAW_BYTES_COUNT: AtomicU64 = AtomicU64::new(0);
pub static PACKETS_PARSED_COUNT: AtomicU64 = AtomicU64::new(0);

static mut SYNC_STATE: u8 = 0;
static mut PACKET_BUF: [u8; 5] = [0; 5];
static mut LAST_NAV_TICK: u64 = 0;

pub fn process_serial_byte(byte: u8) {
    RAW_BYTES_COUNT.fetch_add(1, Ordering::Relaxed);
    unsafe {
        match SYNC_STATE {
            0 => if byte == 0xAA { SYNC_STATE = 1; },
            1 => if byte == 0x55 { SYNC_STATE = 2; } else { SYNC_STATE = 0; },
            2 => { PACKET_BUF[0] = byte; SYNC_STATE = 3; }, // seq
            3 => { PACKET_BUF[1] = byte; SYNC_STATE = 4; }, // btns
            4 => { PACKET_BUF[2] = byte; SYNC_STATE = 5; }, // x
            5 => { PACKET_BUF[3] = byte; SYNC_STATE = 6; }, // y
            6 => {
                PACKET_BUF[4] = byte; // crc
                SYNC_STATE = 0;
                let expected_crc = PACKET_BUF[0] ^ PACKET_BUF[1] ^ PACKET_BUF[2] ^ PACKET_BUF[3];
                if byte == expected_crc {
                    PACKETS_PARSED_COUNT.fetch_add(1, Ordering::Relaxed);
                    parse_gamepad_packet(PACKET_BUF[1], PACKET_BUF[2], PACKET_BUF[3]);
                }
            }
            _ => SYNC_STATE = 0,
        }
    }
}

fn parse_gamepad_packet(btns: u8, x: u8, y: u8) {
    let tick = crate::arch::x86_64::pit::get_ticks();
    unsafe {
        if tick.saturating_sub(LAST_NAV_TICK) > 15 {
            if (btns & 0x01) != 0 { push_event(InputEvent::Nav(NavAction::Select)); LAST_NAV_TICK = tick; }
            else if (btns & 0x02) != 0 { push_event(InputEvent::Nav(NavAction::Back)); LAST_NAV_TICK = tick; }
            else if (btns & 0x04) != 0 { push_event(InputEvent::Nav(NavAction::ActionX)); LAST_NAV_TICK = tick; }
            else if (btns & 0x08) != 0 { push_event(InputEvent::Nav(NavAction::ActionY)); LAST_NAV_TICK = tick; }
            else if (btns & 0x10) != 0 || y > 180 { push_event(InputEvent::Nav(NavAction::Up)); LAST_NAV_TICK = tick; }
            else if (btns & 0x20) != 0 || y < 70 { push_event(InputEvent::Nav(NavAction::Down)); LAST_NAV_TICK = tick; }
            else if (btns & 0x40) != 0 || x < 70 { push_event(InputEvent::Nav(NavAction::Left)); LAST_NAV_TICK = tick; }
            else if (btns & 0x80) != 0 || x > 180 { push_event(InputEvent::Nav(NavAction::Right)); LAST_NAV_TICK = tick; }
        }
    }
}
