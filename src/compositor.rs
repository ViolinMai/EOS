use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use crate::arch::x86_64::interrupts::GUI_ACTIVE;
use crate::arch::x86_64::syscall::{OVERLAY_ACTIVE, OVERLAY_WIDTH, OVERLAY_HEIGHT, OVERLAY_PIXELS};
use crate::input::{self, InputEvent, NavAction};
use crate::writer::WRITER;
use core::ptr::addr_of_mut;

pub fn compositor_core_entry() {
    crate::gui::run();
}
