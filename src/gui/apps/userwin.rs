use crate::input::InputEvent;
use super::App;
use crate::gui::draw::{self, FrameBuffer};
use crate::gui::theme::Theme;
use core::sync::atomic::Ordering;
use crate::arch::x86_64::syscall::{OVERLAY_PIXELS, OVERLAY_WIDTH, OVERLAY_HEIGHT, OVERLAY_ACTIVE, OVERLAY_LOCK, push_user_event};

pub struct UserAppOverlay;

impl UserAppOverlay {
    pub fn new() -> Self { Self }
}

impl App for UserAppOverlay {
    fn draw(&mut self, fb: &mut FrameBuffer, _theme: &Theme, x: usize, y: usize, w: usize, h: usize) {
        draw::draw_rect(fb, x, y, w, h, 0xFF000000);
        
        if OVERLAY_ACTIVE.load(Ordering::Acquire) {
            let ow = OVERLAY_WIDTH.load(Ordering::Acquire);
            let oh = OVERLAY_HEIGHT.load(Ordering::Acquire);
            let cx = x + (w.saturating_sub(ow)) / 2;
            let cy = y + (h.saturating_sub(oh)) / 2;
            
            unsafe {
                while OVERLAY_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); }
                let src = core::ptr::addr_of_mut!(OVERLAY_PIXELS) as *const u32;
                
                for dy in 0..oh.min(h) {
                    for dx in 0..ow.min(w) {
                        fb.pixels[(cy + dy) * fb.pitch_pixels + (cx + dx)] = *src.add(dy * ow + dx);
                    }
                }
                OVERLAY_LOCK.store(false, Ordering::Release);
            }
        } else {
            draw::draw_text(fb, x + w / 2 - 110, y + h / 2 - 10, "Starting Ring 3 Userspace Desktop...", 0xFFFFFFFF, 1);
        }
    }

    fn on_event(&mut self, event: &InputEvent, _mx: isize, _my: isize) {
        push_user_event(*event);
    }

    fn on_resize(&mut self, new_w: usize, new_h: usize) {
        push_user_event(InputEvent::MouseMove { x: new_w as isize, y: new_h as isize });
    }
    
    fn title(&self) -> &str { "Userspace Desktop" }
    fn icon(&self) -> &'static str { "USR" }
}
