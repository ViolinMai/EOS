pub mod pointer;

pub use pointer::{PointerButton, PointerEvent, PointerState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Press,
    Release,
}

#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    pub keycode: u8,
    pub mods: u8,
    pub action: KeyAction,
}

pub struct InputManager {
    pub pointer: PointerState,
    pub shift_down: bool,
    pub ctrl_down: bool,
    pub alt_down: bool,
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            pointer: PointerState::default(),
            shift_down: false,
            ctrl_down: false,
            alt_down: false,
        }
    }

    pub fn handle_raw_motion(&mut self, dx: i32, dy: i32, max_w: i32, max_h: i32) -> PointerEvent {
        let old_x = self.pointer.x;
        let old_y = self.pointer.y;
        self.pointer.x = (self.pointer.x + dx).clamp(0, (max_w - 1).max(0));
        self.pointer.y = (self.pointer.y + dy).clamp(0, (max_h - 1).max(0));

        PointerEvent::Move {
            x: self.pointer.x,
            y: self.pointer.y,
            dx: self.pointer.x - old_x,
            dy: self.pointer.y - old_y,
        }
    }

    pub fn handle_raw_button(&mut self, button_idx: u8, pressed: bool) -> PointerEvent {
        let btn = match button_idx {
            0 => PointerButton::Primary,
            1 => PointerButton::Secondary,
            2 => PointerButton::Middle,
            _ => PointerButton::Other(button_idx),
        };

        if pressed {
            self.pointer.set_down(btn);
            PointerEvent::Down {
                x: self.pointer.x,
                y: self.pointer.y,
                button: btn,
            }
        } else {
            self.pointer.set_up(btn);
            PointerEvent::Up {
                x: self.pointer.x,
                y: self.pointer.y,
                button: btn,
            }
        }
    }

    pub fn handle_raw_key(&mut self, keycode: u8, mods: u8, pressed: bool) -> KeyEvent {
        self.shift_down = (mods & 0x01) != 0;
        self.ctrl_down = (mods & 0x02) != 0;
        self.alt_down = (mods & 0x04) != 0;

        KeyEvent {
            keycode,
            mods,
            action: if pressed { KeyAction::Press } else { KeyAction::Release },
        }
    }
}
