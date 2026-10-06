#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Other(u8),
}

#[derive(Clone, Copy, Debug)]
pub enum PointerEvent {
    Move { x: i32, y: i32, dx: i32, dy: i32 },
    Down { x: i32, y: i32, button: PointerButton },
    Up { x: i32, y: i32, button: PointerButton },
    Scroll { x: i32, y: i32, dy: i32 },
}

#[derive(Clone, Copy, Debug)]
pub struct PointerState {
    pub x: i32,
    pub y: i32,
    pub primary_down: bool,
    pub secondary_down: bool,
    pub middle_down: bool,
}

impl Default for PointerState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            primary_down: false,
            secondary_down: false,
            middle_down: false,
        }
    }
}

impl PointerState {
    pub fn set_down(&mut self, button: PointerButton) {
        match button {
            PointerButton::Primary => self.primary_down = true,
            PointerButton::Secondary => self.secondary_down = true,
            PointerButton::Middle => self.middle_down = true,
            PointerButton::Other(_) => {}
        }
    }

    pub fn set_up(&mut self, button: PointerButton) {
        match button {
            PointerButton::Primary => self.primary_down = false,
            PointerButton::Secondary => self.secondary_down = false,
            PointerButton::Middle => self.middle_down = false,
            PointerButton::Other(_) => {}
        }
    }
}
