use core::arch::asm;
use crate::writer::WRITER;
use core::ptr::addr_of_mut;

const MOUSE_PORT: u16 = 0x60;
const MOUSE_STATUS: u16 = 0x64;

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
    }
    val
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }
}

unsafe fn mouse_wait(wait_type: u8) {
    let mut timeout = 100_000;
    while timeout > 0 {
        timeout -= 1;
        let status = unsafe { inb(MOUSE_STATUS) };
        if wait_type == 0 && (status & 1) == 1 {
            return;
        }
        if wait_type == 1 && (status & 2) == 0 {
            return;
        }
    }
}

unsafe fn mouse_write(data: u8) {
    unsafe {
        mouse_wait(1);
        outb(MOUSE_STATUS, 0xD4);
        mouse_wait(1);
        outb(MOUSE_PORT, data);
    }
}

unsafe fn mouse_read() -> u8 {
    unsafe {
        mouse_wait(0);
        inb(MOUSE_PORT)
    }
}

pub struct MouseState {
    pub x: isize,
    pub y: isize,
    pub left_button: bool,
    pub right_button: bool,
    cycle: u8,
    bytes: [u8; 3],
}

pub static mut MOUSE: MouseState = MouseState {
    x: 100,
    y: 100,
    left_button: false,
    right_button: false,
    cycle: 0,
    bytes: [0; 3],
};

pub fn init() {
    unsafe {
        mouse_wait(1);
        outb(MOUSE_STATUS, 0xA8);

        mouse_wait(1);
        outb(MOUSE_STATUS, 0x20);
        mouse_wait(0);
        let mut status = inb(MOUSE_PORT) | 2;
        status &= !0x20;

        mouse_wait(1);
        outb(MOUSE_STATUS, 0x60);
        mouse_wait(1);
        outb(MOUSE_PORT, status);

        mouse_write(0xF6);
        let _ = mouse_read();

        mouse_write(0xF4);
        let _ = mouse_read();
    }
}

pub unsafe fn on_mouse_interrupt(current_input: &str, cursor_idx: usize) {
    let raw = unsafe { inb(MOUSE_PORT) };
    let mouse = unsafe { &mut *addr_of_mut!(MOUSE) };

    match mouse.cycle {
        0 => {
            if (raw & 0x08) != 0 {
                mouse.bytes[0] = raw;
                mouse.cycle = 1;
            }
        }
        1 => {
            mouse.bytes[1] = raw;
            mouse.cycle = 2;
        }
        2 => {
            mouse.bytes[2] = raw;
            mouse.cycle = 0;

            let flags = mouse.bytes[0];
            let mut dx = mouse.bytes[1] as isize;
            let mut dy = mouse.bytes[2] as isize;

            if (flags & 0x10) != 0 {
                dx |= !0xFF;
            }
            if (flags & 0x20) != 0 {
                dy |= !0xFF;
            }

            mouse.left_button = (flags & 0x01) != 0;
            mouse.right_button = (flags & 0x02) != 0;

            let (max_w, max_h) = if let Some(writer) = unsafe { &*addr_of_mut!(WRITER) } {
                (writer.width as isize, writer.height as isize)
            } else {
                (1920, 1080)
            };

            mouse.x = (mouse.x + dx).clamp(0, max_w - 12);
            mouse.y = (mouse.y - dy).clamp(0, max_h - 18);

            if let Some(writer) = unsafe { &mut *addr_of_mut!(WRITER) } {
                writer.update_mouse_cursor(mouse.x as usize, mouse.y as usize, mouse.left_button, current_input, cursor_idx);
            }
        }
        _ => mouse.cycle = 0,
    }
}
