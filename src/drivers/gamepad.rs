use core::sync::atomic::{AtomicU8, AtomicU64, AtomicBool, Ordering};
use crate::writer::WRITER;

pub static GAMEPAD_CONNECTED: AtomicBool = AtomicBool::new(false);
pub static BTN_MASK: AtomicU8 = AtomicU8::new(0);
pub static STICK_X: AtomicU8 = AtomicU8::new(128);
pub static STICK_Y: AtomicU8 = AtomicU8::new(128);

pub static RAW_BYTES_COUNT: AtomicU64 = AtomicU64::new(0);
pub static PACKETS_PARSED_COUNT: AtomicU64 = AtomicU64::new(0);
pub static LAST_RAW_BYTE: AtomicU8 = AtomicU8::new(0);

pub static KEYBOARD_NAV_MASK: AtomicU8 = AtomicU8::new(0);

const CHAR_QUEUE_SIZE: usize = 32;
static mut CHAR_QUEUE: [u8; CHAR_QUEUE_SIZE] = [0; CHAR_QUEUE_SIZE];
static mut CHAR_HEAD: usize = 0;
static mut CHAR_TAIL: usize = 0;

static mut SYNC_STATE: u8 = 0;
static mut PACKET: [u8; 3] = [0; 3];
static mut KBD_EXTENDED: bool = false;

pub fn push_editor_char(ch: u8) {
    unsafe {
        let next = (CHAR_HEAD + 1) % CHAR_QUEUE_SIZE;
        if next != CHAR_TAIL {
            CHAR_QUEUE[CHAR_HEAD] = ch;
            CHAR_HEAD = next;
        }
    }
}

pub fn pop_editor_char() -> Option<u8> {
    unsafe {
        if CHAR_HEAD == CHAR_TAIL {
            None
        } else {
            let ch = CHAR_QUEUE[CHAR_TAIL];
            CHAR_TAIL = (CHAR_TAIL + 1) % CHAR_QUEUE_SIZE;
            Some(ch)
        }
    }
}

pub fn process_serial_byte(byte: u8) {
    RAW_BYTES_COUNT.fetch_add(1, Ordering::Relaxed);
    LAST_RAW_BYTE.store(byte, Ordering::Relaxed);

    unsafe {
        match SYNC_STATE {
            0 => { if byte == 0xAA { SYNC_STATE = 1; } }
            1 => { PACKET[0] = byte; SYNC_STATE = 2; }
            2 => { PACKET[1] = byte; SYNC_STATE = 3; }
            3 => {
                PACKET[2] = byte;
                SYNC_STATE = 0;
                
                GAMEPAD_CONNECTED.store(true, Ordering::Relaxed);
                PACKETS_PARSED_COUNT.fetch_add(1, Ordering::Relaxed);
                BTN_MASK.store(PACKET[0], Ordering::Relaxed);
                STICK_X.store(PACKET[1], Ordering::Relaxed);
                STICK_Y.store(PACKET[2], Ordering::Relaxed);
            }
            _ => SYNC_STATE = 0,
        }
    }
}

pub fn handle_keyboard_nav_scancode(scancode: u8) {
    unsafe {
        if scancode == 0xE0 {
            KBD_EXTENDED = true;
            return;
        }

        let is_ext = KBD_EXTENDED;
        KBD_EXTENDED = false;

        let is_release = (scancode & 0x80) != 0;
        let code = scancode & 0x7F;

        if is_ext {
            let bit = match code {
                0x48 => 0x10,
                0x50 => 0x20,
                0x4B => 0x40,
                0x4D => 0x80,
                _ => 0,
            };
            if bit != 0 {
                if is_release {
                    KEYBOARD_NAV_MASK.fetch_and(!bit, Ordering::Relaxed);
                } else {
                    KEYBOARD_NAV_MASK.fetch_or(bit, Ordering::Relaxed);
                }
            }
        } else {
            let bit = match code {
                0x11 => 0x10,
                0x1F => 0x20,
                0x1E => 0x40,
                0x20 => 0x80,
                0x1C | 0x39 => 0x01,
                0x01 => 0x02,
                0x12 => 0x04,
                _ => 0,
            };
            if bit != 0 {
                if is_release {
                    KEYBOARD_NAV_MASK.fetch_and(!bit, Ordering::Relaxed);
                } else {
                    KEYBOARD_NAV_MASK.fetch_or(bit, Ordering::Relaxed);
                }
            }

            if !is_release {
                let ascii = match code {
                    0x0E => Some(b'\x08'),
                    0x1C => Some(b'\n'),
                    0x39 => Some(b' '),
                    0x02..=0x0B => Some(b'0' + ((code - 1) % 10)),
                    0x10 => Some(b'q'), 0x11 => Some(b'w'), 0x12 => Some(b'e'), 0x13 => Some(b'r'),
                    0x14 => Some(b't'), 0x15 => Some(b'y'), 0x16 => Some(b'u'), 0x17 => Some(b'i'),
                    0x18 => Some(b'o'), 0x19 => Some(b'p'), 0x1E => Some(b'a'), 0x1F => Some(b's'),
                    0x20 => Some(b'd'), 0x21 => Some(b'f'), 0x22 => Some(b'g'), 0x23 => Some(b'h'),
                    0x24 => Some(b'j'), 0x25 => Some(b'k'), 0x26 => Some(b'l'), 0x2C => Some(b'z'),
                    0x2D => Some(b'x'), 0x2E => Some(b'c'), 0x2F => Some(b'v'), 0x30 => Some(b'b'),
                    0x31 => Some(b'n'), 0x32 => Some(b'm'),
                    0x33 => Some(b','), 0x34 => Some(b'.'), 0x35 => Some(b'/'),
                    0x0C => Some(b'-'), 0x0D => Some(b'='),
                    _ => None,
                };
                if let Some(ch) = ascii {
                    push_editor_char(ch);
                }
            }
        }
    }
}

pub fn run_terminal_test() {
    unsafe {
        if let Some(writer) = &mut *core::ptr::addr_of_mut!(WRITER) {
            writer.cursor_x = 24; writer.cursor_y = 20;
            writer.write_str("=== Diagnostics Controller Active ===", 56, 189, 248);
        }
    }
}
