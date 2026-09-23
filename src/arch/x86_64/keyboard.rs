use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

const KEYBOARD_PORT: u16 = 0x60;
#[allow(dead_code)]
const SERIAL_PORT: u16 = 0x3F8;

static SHIFT_ACTIVE: AtomicBool = AtomicBool::new(false);
static CTRL_ACTIVE: AtomicBool = AtomicBool::new(false);
static EXTENDED_PREFIX: AtomicBool = AtomicBool::new(false);

const KBD_BUF_SIZE: usize = 128;
static mut KEY_RING_BUFFER: [u8; KBD_BUF_SIZE] = [0; KBD_BUF_SIZE];
static mut KBD_HEAD: usize = 0;
static mut KBD_TAIL: usize = 0;

pub fn clear_keyboard_buffer() {
    unsafe {
        KBD_HEAD = 0;
        KBD_TAIL = 0;
    }
}

pub fn push_char_to_buffer(c: u8) {
    unsafe {
        let next_head = (KBD_HEAD + 1) % KBD_BUF_SIZE;
        if next_head != KBD_TAIL {
            KEY_RING_BUFFER[KBD_HEAD] = c;
            KBD_HEAD = next_head;
        }
    }
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[allow(dead_code)]
fn read_serial_char_if_available() -> Option<u8> {
    unsafe {
        let status = inb(SERIAL_PORT + 5);
        if (status & 1) != 0 {
            let c = inb(SERIAL_PORT);
            if c == b'\r' {
                Some(b'\n')
            } else {
                Some(c)
            }
        } else {
            None
        }
    }
}

#[allow(dead_code)]
pub fn pop_char_from_buffer() -> Option<u8> {
    if let Some(c) = read_serial_char_if_available() {
        return Some(c);
    }

    unsafe {
        if KBD_HEAD == KBD_TAIL {
            None
        } else {
            let c = KEY_RING_BUFFER[KBD_TAIL];
            KBD_TAIL = (KBD_TAIL + 1) % KBD_BUF_SIZE;
            Some(c)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyEvent {
    Char(char),
    UpArrow,
    DownArrow,
    LeftArrow,
    RightArrow,
    CtrlS,
    CtrlQ,
    CtrlC,
    CtrlV,
    CtrlX,
    Escape,
    None,
}

pub fn handle_scancode(scancode: u8) -> KeyEvent {
    if scancode == 0xE0 {
        EXTENDED_PREFIX.store(true, Ordering::Relaxed);
        return KeyEvent::None;
    }

    let is_extended = EXTENDED_PREFIX.swap(false, Ordering::Relaxed);

    if is_extended {
        return match scancode {
            0x48 => KeyEvent::UpArrow,
            0x50 => KeyEvent::DownArrow,
            0x4B => KeyEvent::LeftArrow,
            0x4D => KeyEvent::RightArrow,
            _ => KeyEvent::None,
        };
    }

    match scancode {
        0x1D => {
            CTRL_ACTIVE.store(true, Ordering::Relaxed);
            return KeyEvent::None;
        }
        0x9D => {
            CTRL_ACTIVE.store(false, Ordering::Relaxed);
            return KeyEvent::None;
        }
        0x2A | 0x36 => {
            SHIFT_ACTIVE.store(true, Ordering::Relaxed);
            return KeyEvent::None;
        }
        0xAA | 0xB6 => {
            SHIFT_ACTIVE.store(false, Ordering::Relaxed);
            return KeyEvent::None;
        }
        0x01 => return KeyEvent::Escape,
        _ => {}
    }

    if scancode >= 0x80 {
        return KeyEvent::None;
    }

    let ctrl = CTRL_ACTIVE.load(Ordering::Relaxed);
    let shift = SHIFT_ACTIVE.load(Ordering::Relaxed);

    if ctrl {
        return match scancode {
            0x1F => KeyEvent::CtrlS,
            0x10 => KeyEvent::CtrlQ,
            0x2E => KeyEvent::CtrlC,
            0x2F => KeyEvent::CtrlV,
            0x2D => KeyEvent::CtrlX,
            _ => KeyEvent::None,
        };
    }

    let c = match scancode {
        0x02 => if shift { '!' } else { '1' },
        0x03 => if shift { '@' } else { '2' },
        0x04 => if shift { '#' } else { '3' },
        0x05 => if shift { '$' } else { '4' },
        0x06 => if shift { '%' } else { '5' },
        0x07 => if shift { '^' } else { '6' },
        0x08 => if shift { '&' } else { '7' },
        0x09 => if shift { '*' } else { '8' },
        0x0A => if shift { '(' } else { '9' },
        0x0B => if shift { ')' } else { '0' },
        0x0C => if shift { '_' } else { '-' },
        0x0D => if shift { '+' } else { '=' },
        0x0E => '\x08',
        0x0F => '\t',
        0x10 => if shift { 'Q' } else { 'q' },
        0x11 => if shift { 'W' } else { 'w' },
        0x12 => if shift { 'E' } else { 'e' },
        0x13 => if shift { 'R' } else { 'r' },
        0x14 => if shift { 'T' } else { 't' },
        0x15 => if shift { 'Y' } else { 'y' },
        0x16 => if shift { 'U' } else { 'u' },
        0x17 => if shift { 'I' } else { 'i' },
        0x18 => if shift { 'O' } else { 'o' },
        0x19 => if shift { 'P' } else { 'p' },
        0x1A => if shift { '{' } else { '[' },
        0x1B => if shift { '}' } else { ']' },
        0x1C => '\n',
        0x1E => if shift { 'A' } else { 'a' },
        0x1F => if shift { 'S' } else { 's' },
        0x20 => if shift { 'D' } else { 'd' },
        0x21 => if shift { 'F' } else { 'f' },
        0x22 => if shift { 'G' } else { 'g' },
        0x23 => if shift { 'H' } else { 'h' },
        0x24 => if shift { 'J' } else { 'j' },
        0x25 => if shift { 'K' } else { 'k' },
        0x26 => if shift { 'L' } else { 'l' },
        0x27 => if shift { ':' } else { ';' },
        0x28 => if shift { '"' } else { '\'' },
        0x29 => if shift { '~' } else { '`' },
        0x2B => if shift { '|' } else { '\\' },
        0x2C => if shift { 'Z' } else { 'z' },
        0x2D => if shift { 'X' } else { 'x' },
        0x2E => if shift { 'C' } else { 'c' },
        0x2F => if shift { 'V' } else { 'v' },
        0x30 => if shift { 'B' } else { 'b' },
        0x31 => if shift { 'N' } else { 'n' },
        0x32 => if shift { 'M' } else { 'm' },
        0x33 => if shift { '<' } else { ',' },
        0x34 => if shift { '>' } else { '.' },
        0x35 => if shift { '?' } else { '/' },
        0x39 => ' ',
        _ => return KeyEvent::None,
    };

    push_char_to_buffer(c as u8);
    KeyEvent::Char(c)
}

pub unsafe fn read_scancode() -> u8 {
    unsafe { inb(KEYBOARD_PORT) }
}
