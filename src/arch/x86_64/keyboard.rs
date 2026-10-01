use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::input::{push_event, InputEvent, NavAction, MOD_SHIFT, MOD_CTRL, MOD_ALT, MOD_CAPS};

const KEYBOARD_PORT: u16 = 0x60;

static SHIFT_ACTIVE: AtomicBool = AtomicBool::new(false);
static CTRL_ACTIVE: AtomicBool = AtomicBool::new(false);
static ALT_ACTIVE: AtomicBool = AtomicBool::new(false);
static CAPS_ACTIVE: AtomicBool = AtomicBool::new(false);
static EXTENDED_PREFIX: AtomicBool = AtomicBool::new(false);

pub fn clear_keyboard_buffer() { }

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    }
    val
}

pub unsafe fn read_scancode() -> Option<u8> {
    unsafe {
        let status = inb(0x64);
        // التأكد من أن البايت جاهز وينتمي للكيبورد (Bit 5 == 0) لمنع ابتلاع بايتات الماوس
        if (status & 0x01) != 0 && (status & 0x20) == 0 {
            Some(inb(KEYBOARD_PORT))
        } else {
            None
        }
    }
}

fn get_current_mods() -> u8 {
    let mut m = 0;
    if SHIFT_ACTIVE.load(Ordering::Relaxed) { m |= MOD_SHIFT; }
    if CTRL_ACTIVE.load(Ordering::Relaxed) { m |= MOD_CTRL; }
    if ALT_ACTIVE.load(Ordering::Relaxed) { m |= MOD_ALT; }
    if CAPS_ACTIVE.load(Ordering::Relaxed) { m |= MOD_CAPS; }
    m
}

pub fn handle_scancode(scancode: u8) {
    if scancode == 0xE0 {
        EXTENDED_PREFIX.store(true, Ordering::Relaxed);
        return;
    }

    let is_extended = EXTENDED_PREFIX.swap(false, Ordering::Relaxed);
    let is_release = (scancode & 0x80) != 0;
    let code = scancode & 0x7F;

    if is_release {
        match code {
            0x2A | 0x36 => { SHIFT_ACTIVE.store(false, Ordering::Relaxed); }
            0x1D => { CTRL_ACTIVE.store(false, Ordering::Relaxed); }
            0x38 => { ALT_ACTIVE.store(false, Ordering::Relaxed); }
            _ => {}
        }
        push_event(InputEvent::KeyUp { keycode: code });
        return;
    }

    if is_extended {
        match code {
            0x48 => { push_event(InputEvent::Nav(NavAction::Up)); push_event(InputEvent::KeyDown { keycode: 0x48, mods: get_current_mods() }); }
            0x50 => { push_event(InputEvent::Nav(NavAction::Down)); push_event(InputEvent::KeyDown { keycode: 0x50, mods: get_current_mods() }); }
            0x4B => { push_event(InputEvent::Nav(NavAction::Left)); push_event(InputEvent::KeyDown { keycode: 0x4B, mods: get_current_mods() }); }
            0x4D => { push_event(InputEvent::Nav(NavAction::Right)); push_event(InputEvent::KeyDown { keycode: 0x4D, mods: get_current_mods() }); }
            0x1D => { CTRL_ACTIVE.store(true, Ordering::Relaxed); push_event(InputEvent::KeyDown { keycode: 0x1D, mods: get_current_mods() }); }
            0x38 => { ALT_ACTIVE.store(true, Ordering::Relaxed); push_event(InputEvent::KeyDown { keycode: 0x38, mods: get_current_mods() }); }
            _ => { push_event(InputEvent::KeyDown { keycode: code | 0x80, mods: get_current_mods() }); }
        }
        return;
    }

    match code {
        0x2A | 0x36 => { SHIFT_ACTIVE.store(true, Ordering::Relaxed); push_event(InputEvent::KeyDown { keycode: code, mods: get_current_mods() }); return; }
        0x1D => { CTRL_ACTIVE.store(true, Ordering::Relaxed); push_event(InputEvent::KeyDown { keycode: code, mods: get_current_mods() }); return; }
        0x38 => { ALT_ACTIVE.store(true, Ordering::Relaxed); push_event(InputEvent::KeyDown { keycode: code, mods: get_current_mods() }); return; }
        0x3A => { CAPS_ACTIVE.store(!CAPS_ACTIVE.load(Ordering::Relaxed), Ordering::Relaxed); push_event(InputEvent::KeyDown { keycode: code, mods: get_current_mods() }); return; }
        0x01 => { push_event(InputEvent::Nav(NavAction::Back)); push_event(InputEvent::KeyDown { keycode: code, mods: get_current_mods() }); return; } 
        0x1C => { 
            push_event(InputEvent::Nav(NavAction::Select)); 
            push_event(InputEvent::KeyDown { keycode: code, mods: get_current_mods() });
            push_event(InputEvent::Char('\n')); 
            return; 
        }
        0x0F => { push_event(InputEvent::Nav(NavAction::Right)); push_event(InputEvent::KeyDown { keycode: code, mods: get_current_mods() }); return; }
        _ => {}
    }

    push_event(InputEvent::KeyDown { keycode: code, mods: get_current_mods() });

    let shift = SHIFT_ACTIVE.load(Ordering::Relaxed);
    let caps = CAPS_ACTIVE.load(Ordering::Relaxed);
    let is_upper = shift ^ caps;
    
    let c = match code {
        0x02 => if shift { '!' } else { '1' }, 0x03 => if shift { '@' } else { '2' },
        0x04 => if shift { '#' } else { '3' }, 0x05 => if shift { '$' } else { '4' },
        0x06 => if shift { '%' } else { '5' }, 0x07 => if shift { '^' } else { '6' },
        0x08 => if shift { '&' } else { '7' }, 0x09 => if shift { '*' } else { '8' },
        0x0A => if shift { '(' } else { '9' }, 0x0B => if shift { ')' } else { '0' },
        0x0C => if shift { '_' } else { '-' }, 0x0D => if shift { '+' } else { '=' },
        0x0E => '\x08', // Backspace
        0x10 => if is_upper { 'Q' } else { 'q' }, 0x11 => if is_upper { 'W' } else { 'w' },
        0x12 => if is_upper { 'E' } else { 'e' }, 0x13 => if is_upper { 'R' } else { 'r' },
        0x14 => if is_upper { 'T' } else { 't' }, 0x15 => if is_upper { 'Y' } else { 'y' },
        0x16 => if is_upper { 'U' } else { 'u' }, 0x17 => if is_upper { 'I' } else { 'i' },
        0x18 => if is_upper { 'O' } else { 'o' }, 0x19 => if is_upper { 'P' } else { 'p' },
        0x1A => if shift { '{' } else { '[' }, 0x1B => if shift { '}' } else { ']' },
        0x1E => if is_upper { 'A' } else { 'a' }, 0x1F => if is_upper { 'S' } else { 's' },
        0x20 => if is_upper { 'D' } else { 'd' }, 0x21 => if is_upper { 'F' } else { 'f' },
        0x22 => if is_upper { 'G' } else { 'g' }, 0x23 => if is_upper { 'H' } else { 'h' },
        0x24 => if is_upper { 'J' } else { 'j' }, 0x25 => if is_upper { 'K' } else { 'k' },
        0x26 => if is_upper { 'L' } else { 'l' }, 
        0x27 => if shift { ':' } else { ';' }, 0x28 => if shift { '"' } else { '\'' },
        0x29 => if shift { '~' } else { '`' }, 0x2B => if shift { '|' } else { '\\' },
        0x2C => if is_upper { 'Z' } else { 'z' }, 0x2D => if is_upper { 'X' } else { 'x' },
        0x2E => if is_upper { 'C' } else { 'c' }, 0x2F => if is_upper { 'V' } else { 'v' },
        0x30 => if is_upper { 'B' } else { 'b' }, 0x31 => if is_upper { 'N' } else { 'n' },
        0x32 => if is_upper { 'M' } else { 'm' }, 
        0x33 => if shift { '<' } else { ',' }, 0x34 => if shift { '>' } else { '.' },
        0x35 => if shift { '?' } else { '/' },
        0x39 => ' ',
        _ => return,
    };
    push_event(InputEvent::Char(c));
}
