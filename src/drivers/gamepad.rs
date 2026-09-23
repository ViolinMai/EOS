use crate::writer::WRITER;
use core::sync::atomic::{AtomicU8, AtomicBool, Ordering};
use crate::arch::x86_64::pit;
use crate::arch::x86_64::keyboard;

pub static GAMEPAD_CONNECTED: AtomicBool = AtomicBool::new(false);
pub static BTN_MASK: AtomicU8 = AtomicU8::new(0);
pub static STICK_X: AtomicU8 = AtomicU8::new(128);
pub static STICK_Y: AtomicU8 = AtomicU8::new(128);

static mut SYNC_STATE: u8 = 0;
static mut PACKET: [u8; 3] = [0; 3];

pub fn process_serial_byte(byte: u8) {
    unsafe {
        match SYNC_STATE {
            0 => { if byte == 0xAA { SYNC_STATE = 1; } }
            1 => { PACKET[0] = byte; SYNC_STATE = 2; }
            2 => { PACKET[1] = byte; SYNC_STATE = 3; }
            3 => {
                PACKET[2] = byte;
                SYNC_STATE = 0;
                GAMEPAD_CONNECTED.store(true, Ordering::Relaxed);
                BTN_MASK.store(PACKET[0], Ordering::Relaxed);
                STICK_X.store(PACKET[1], Ordering::Relaxed);
                STICK_Y.store(PACKET[2], Ordering::Relaxed);
            }
            _ => SYNC_STATE = 0,
        }
    }
}

pub fn run_terminal_test() {
    unsafe {
        if let Some(writer) = &mut *core::ptr::addr_of_mut!(WRITER) {
            writer.cursor_x = 24; writer.cursor_y = 20;
            writer.write_str("=== GameSir Nova Lite Controller Test ===", 56, 189, 248);
            writer.cursor_x = 24; writer.cursor_y = 50;
            writer.write_str("Press [ESC] on keyboard to exit.", 148, 163, 184);
        }
    }

    loop {
        // الخروج عند الضغط على Escape
        if unsafe { keyboard::read_scancode() } == 0x01 {
            break;
        }

        if GAMEPAD_CONNECTED.load(Ordering::Relaxed) {
            let btns = BTN_MASK.load(Ordering::Relaxed);
            let x = STICK_X.load(Ordering::Relaxed);
            let y = STICK_Y.load(Ordering::Relaxed);

            let a = (btns & 0x01) != 0;
            let b = (btns & 0x02) != 0;
            let x_btn = (btns & 0x04) != 0;
            let y_btn = (btns & 0x08) != 0;
            let up = (btns & 0x10) != 0;
            let down = (btns & 0x20) != 0;
            let left = (btns & 0x40) != 0;
            let right = (btns & 0x80) != 0;

            unsafe {
                if let Some(writer) = &mut *core::ptr::addr_of_mut!(WRITER) {
                    writer.cursor_x = 24; writer.cursor_y = 90;
                    writer.clear_current_line(0);
                    writer.write_fmt(format_args!("D-Pad:  UP:{}  DOWN:{}  LEFT:{}  RIGHT:{}", 
                        if up {"[X]"} else {"[ ]"}, 
                        if down {"[X]"} else {"[ ]"}, 
                        if left {"[X]"} else {"[ ]"}, 
                        if right {"[X]"} else {"[ ]"}
                    ), 255, 255, 255);

                    writer.cursor_x = 24; writer.cursor_y = 120;
                    writer.clear_current_line(0);
                    writer.write_fmt(format_args!("Action: A:{}  B:{}  X:{}  Y:{}", 
                        if a {"[X]"} else {"[ ]"}, 
                        if b {"[X]"} else {"[ ]"}, 
                        if x_btn {"[X]"} else {"[ ]"}, 
                        if y_btn {"[X]"} else {"[ ]"}
                    ), 74, 222, 128);

                    writer.cursor_x = 24; writer.cursor_y = 150;
                    writer.clear_current_line(0);
                    writer.write_fmt(format_args!("Left Analog Stick:  X: {:03}  |  Y: {:03}   (128 is center)", x, y), 250, 204, 21);
                }
            }
        } else {
            unsafe {
                if let Some(writer) = &mut *core::ptr::addr_of_mut!(WRITER) {
                    writer.cursor_x = 24; writer.cursor_y = 90;
                    writer.clear_current_line(0);
                    writer.write_str("Waiting for Gamepad Bridge to connect on COM2...", 248, 113, 113);
                }
            }
        }

        pit::sleep_ms(16); // ~60 FPS update rate
    }

    unsafe {
        if let Some(writer) = &mut *core::ptr::addr_of_mut!(WRITER) {
            writer.clear(15, 23, 42);
        }
    }
    crate::arch::x86_64::interrupts::print_prompt();
}
