use core::arch::asm;
use crate::input::{push_event, InputEvent};

const MOUSE_DATA_PORT: u16 = 0x60;
const MOUSE_STATUS_PORT: u16 = 0x64;
const MOUSE_COMMAND_PORT: u16 = 0x64;

static mut CYCLE: u8 = 0;
static mut BYTES: [u8; 4] = [0; 4];
static mut HAS_WHEEL: bool = false;
static mut LAST_LEFT: bool = false;
static mut LAST_RIGHT: bool = false;

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

unsafe fn mouse_wait_write() {
    let mut timeout = 100_000usize;
    while timeout > 0 {
        if (unsafe { inb(MOUSE_STATUS_PORT) } & 0x02) == 0 { return; }
        timeout -= 1;
        core::hint::spin_loop();
    }
}

unsafe fn mouse_wait_read() -> bool {
    let mut timeout = 100_000usize;
    while timeout > 0 {
        if (unsafe { inb(MOUSE_STATUS_PORT) } & 0x01) != 0 { return true; }
        timeout -= 1;
        core::hint::spin_loop();
    }
    false
}

unsafe fn mouse_write_cmd(cmd: u8) {
    unsafe {
        mouse_wait_write();
        outb(MOUSE_COMMAND_PORT, cmd);
    }
}

unsafe fn mouse_write_device(byte: u8) {
    unsafe {
        mouse_wait_write();
        outb(MOUSE_COMMAND_PORT, 0xD4);
        mouse_wait_write();
        outb(MOUSE_DATA_PORT, byte);
    }
}

unsafe fn mouse_read_data() -> u8 {
    unsafe {
        if mouse_wait_read() { inb(MOUSE_DATA_PORT) } else { 0 }
    }
}

pub fn init() {
    unsafe {
        mouse_write_cmd(0xA8);
        mouse_write_cmd(0x20);
        let mut config = mouse_read_data();
        config |= 0x02;
        config |= 0x01;
        config &= !0x20;
        mouse_write_cmd(0x60);
        mouse_wait_write();
        outb(MOUSE_DATA_PORT, config);

        mouse_write_device(0xF6);
        let _ = mouse_read_data();

        // IntelliMouse sequence
        mouse_write_device(0xF3); let _ = mouse_read_data();
        mouse_write_device(200);  let _ = mouse_read_data();
        mouse_write_device(0xF3); let _ = mouse_read_data();
        mouse_write_device(100);  let _ = mouse_read_data();
        mouse_write_device(0xF3); let _ = mouse_read_data();
        mouse_write_device(80);   let _ = mouse_read_data();

        mouse_write_device(0xF2); let _ = mouse_read_data();
        let dev_id = mouse_read_data();
        HAS_WHEEL = dev_id == 3 || dev_id == 4;

        // Boost sample rate to 200 Hz & resolution to 8 counts/mm
        mouse_write_device(0xF3); let _ = mouse_read_data();
        mouse_write_device(200);  let _ = mouse_read_data();
        mouse_write_device(0xE8); let _ = mouse_read_data();
        mouse_write_device(0x03); let _ = mouse_read_data();

        mouse_write_device(0xF4);
        let _ = mouse_read_data();

        CYCLE = 0;
        LAST_LEFT = false;
        LAST_RIGHT = false;
        crate::log_info!("MOUSE", "PS/2 Mouse Ready (200Hz, High-Res).");
    }
}

pub unsafe fn on_mouse_interrupt() {
    unsafe {
        let mut limit = 16;
        while (inb(MOUSE_STATUS_PORT) & 0x01) != 0 && limit > 0 {
            limit -= 1;
            let status = inb(MOUSE_STATUS_PORT);
            if (status & 0x20) == 0 { break; }
            let raw = inb(MOUSE_DATA_PORT);
            match CYCLE {
                0 => {
                    if (raw & 0x08) != 0 {
                        BYTES[0] = raw;
                        CYCLE = 1;
                    }
                },
                1 => {
                    BYTES[1] = raw;
                    CYCLE = 2;
                },
                2 => {
                    BYTES[2] = raw;
                    if HAS_WHEEL {
                        CYCLE = 3;
                    } else {
                        CYCLE = 0;
                        process_packet();
                    }
                },
                3 => {
                    BYTES[3] = raw;
                    CYCLE = 0;
                    process_packet();
                },
                _ => CYCLE = 0,
            }
        }
    }
}

unsafe fn process_packet() {
    unsafe {
        let flags = BYTES[0];

        let mut dx = BYTES[1] as isize;
        let mut dy = BYTES[2] as isize;

        if (flags & 0x10) != 0 { dx |= !0xFF; }
        if (flags & 0x20) != 0 { dy |= !0xFF; }

        // Clamp overflows instead of dropping packet completely
        if (flags & 0x40) != 0 { dx = if (flags & 0x10) != 0 { -255 } else { 255 }; }
        if (flags & 0x80) != 0 { dy = if (flags & 0x20) != 0 { -255 } else { 255 }; }

        let left = (flags & 0x01) != 0;
        let right = (flags & 0x02) != 0;

        if dx != 0 || dy != 0 {
            push_event(InputEvent::MouseMove { x: dx, y: -dy });
        }

        if left != LAST_LEFT {
            LAST_LEFT = left;
            push_event(InputEvent::MouseButton { button: 0, pressed: left });
        }
        if right != LAST_RIGHT {
            LAST_RIGHT = right;
            push_event(InputEvent::MouseButton { button: 1, pressed: right });
        }

        if HAS_WHEEL {
            let dz = (BYTES[3] as i8) as isize;
            if dz != 0 {
                push_event(InputEvent::Scroll { dy: dz });
            }
        }
    }
}
