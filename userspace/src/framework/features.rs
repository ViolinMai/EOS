use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

pub const FRAMEWORK_VERSION: &str = "0.2.0";

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameworkChannel {
    Stable = 0,
    Beta = 1,
}

static ACTIVE_CHANNEL: AtomicU8 = AtomicU8::new(FrameworkChannel::Stable as u8);
static FEAT_AERO_BLUR: AtomicBool = AtomicBool::new(true);
static FEAT_AA_CORNERS: AtomicBool = AtomicBool::new(true);
static FEAT_SMOOTH_ANIM: AtomicBool = AtomicBool::new(true);
static FEAT_ADAPTIVE_TEXT: AtomicBool = AtomicBool::new(true);
static FEAT_DEBUG_DAMAGE_OVERLAY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug)]
pub struct FrameworkFeatures {
    pub channel: FrameworkChannel,
    pub aero_blur: bool,
    pub aa_corners: bool,
    pub smooth_animations: bool,
    pub adaptive_text: bool,
    pub debug_damage_overlay: bool,
}

pub fn get_features() -> FrameworkFeatures {
    let ch = if ACTIVE_CHANNEL.load(Ordering::Relaxed) == 1 {
        FrameworkChannel::Beta
    } else {
        FrameworkChannel::Stable
    };
    FrameworkFeatures {
        channel: ch,
        aero_blur: FEAT_AERO_BLUR.load(Ordering::Relaxed),
        aa_corners: FEAT_AA_CORNERS.load(Ordering::Relaxed),
        smooth_animations: FEAT_SMOOTH_ANIM.load(Ordering::Relaxed),
        adaptive_text: FEAT_ADAPTIVE_TEXT.load(Ordering::Relaxed),
        debug_damage_overlay: FEAT_DEBUG_DAMAGE_OVERLAY.load(Ordering::Relaxed),
    }
}

pub fn toggle_adaptive_text() -> bool {
    let next = !FEAT_ADAPTIVE_TEXT.load(Ordering::Relaxed);
    FEAT_ADAPTIVE_TEXT.store(next, Ordering::Relaxed);
    crate::framework::theme::redraw_fullscreen();
    next
}

pub fn set_channel(ch: FrameworkChannel) {
    ACTIVE_CHANNEL.store(ch as u8, Ordering::Relaxed);
    match ch {
        FrameworkChannel::Stable => {
            FEAT_AERO_BLUR.store(true, Ordering::Relaxed);
            FEAT_AA_CORNERS.store(true, Ordering::Relaxed);
            FEAT_SMOOTH_ANIM.store(true, Ordering::Relaxed);
            FEAT_DEBUG_DAMAGE_OVERLAY.store(false, Ordering::Relaxed);
        }
        FrameworkChannel::Beta => {
            FEAT_AERO_BLUR.store(true, Ordering::Relaxed);
            FEAT_AA_CORNERS.store(true, Ordering::Relaxed);
            FEAT_SMOOTH_ANIM.store(true, Ordering::Relaxed);
        }
    }
    crate::framework::theme::redraw_fullscreen();
}

pub fn toggle_aero_blur() -> bool {
    let next = !FEAT_AERO_BLUR.load(Ordering::Relaxed);
    FEAT_AERO_BLUR.store(next, Ordering::Relaxed);
    crate::framework::theme::redraw_fullscreen();
    next
}

pub fn toggle_aa_corners() -> bool {
    let next = !FEAT_AA_CORNERS.load(Ordering::Relaxed);
    FEAT_AA_CORNERS.store(next, Ordering::Relaxed);
    crate::framework::theme::redraw_fullscreen();
    next
}

pub fn toggle_smooth_animations() -> bool {
    let next = !FEAT_SMOOTH_ANIM.load(Ordering::Relaxed);
    FEAT_SMOOTH_ANIM.store(next, Ordering::Relaxed);
    next
}

pub fn toggle_debug_damage_overlay() -> bool {
    let next = !FEAT_DEBUG_DAMAGE_OVERLAY.load(Ordering::Relaxed);
    FEAT_DEBUG_DAMAGE_OVERLAY.store(next, Ordering::Relaxed);
    crate::framework::theme::redraw_fullscreen();
    next
}
