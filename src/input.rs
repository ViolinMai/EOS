use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NavAction {
    Up, Down, Left, Right, Select, Back, ActionX, ActionY,
}

#[derive(Copy, Clone, Debug)]
pub enum InputEvent {
    Char(char),
    KeyDown { keycode: u8, mods: u8 },
    KeyUp { keycode: u8 },
    Nav(NavAction),
    MouseMove { x: isize, y: isize },
    MouseClick { left: bool, right: bool },
    MouseButton { button: u8, pressed: bool },
    Scroll { dy: isize },
    DeviceConnected(&'static str),
    DeviceDisconnected(&'static str),
}

pub const MOD_SHIFT: u8 = 1 << 0;
pub const MOD_CTRL:  u8 = 1 << 1;
pub const MOD_ALT:   u8 = 1 << 2;
pub const MOD_CAPS:  u8 = 1 << 3;
pub const MOD_WIN:   u8 = 1 << 4;

const QUEUE_SIZE: usize = 512;
static mut EVENT_QUEUE: [Option<InputEvent>; QUEUE_SIZE] = [None; QUEUE_SIZE];
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);

pub fn push_event(event: InputEvent) {
    let head = HEAD.load(Ordering::Relaxed);
    let next_head = (head + 1) % QUEUE_SIZE;
    if next_head != TAIL.load(Ordering::Acquire) {
        unsafe { EVENT_QUEUE[head] = Some(event); }
        HEAD.store(next_head, Ordering::Release);
    }
}

pub fn poll_event() -> Option<InputEvent> {
    let tail = TAIL.load(Ordering::Relaxed);
    if tail == HEAD.load(Ordering::Acquire) { return None; }
    let event = unsafe { EVENT_QUEUE[tail].take() };
    TAIL.store((tail + 1) % QUEUE_SIZE, Ordering::Release);
    event
}

#[inline(always)]
pub fn has_events() -> bool {
    TAIL.load(Ordering::Relaxed) != HEAD.load(Ordering::Acquire)
}
