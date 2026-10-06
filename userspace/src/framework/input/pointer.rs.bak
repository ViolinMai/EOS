#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Left = 0,
    Right = 1,
    Middle = 2,
}

#[derive(Debug, Clone, Copy)]
pub enum PointerEvent {
    Move { x: i32, y: i32, dx: i32, dy: i32 },
    Down { x: i32, y: i32, button: PointerButton },
    Up { x: i32, y: i32, button: PointerButton },
    DoubleClick { x: i32, y: i32, button: PointerButton },
    Scroll { x: i32, y: i32, dy: i32 },
}

#[derive(Clone, Copy)]
pub struct PointerState {
    pub x: i32,
    pub y: i32,
    pub buttons: [bool; 3],
}

struct ClickTracker {
    last_button: Option<PointerButton>,
    last_x: i32,
    last_y: i32,
    last_tick: u64,
}

pub struct InputManager {
    pub state: PointerState,
    pub sensitivity: f32,
    pub capture: Option<usize>, // Captured Window/Widget ID
    screen_w: i32,
    screen_h: i32,
    click_tracker: ClickTracker,
    pending_dx: i32,
    pending_dy: i32,
}

impl InputManager {
    pub fn new(screen_w: i32, screen_h: i32) -> Self {
        Self {
            state: PointerState {
                x: screen_w / 2,
                y: screen_h / 2,
                buttons: [false; 3],
            },
            sensitivity: 1.0,
            capture: None,
            screen_w,
            screen_h,
            click_tracker: ClickTracker {
                last_button: None,
                last_x: 0,
                last_y: 0,
                last_tick: 0,
            },
            pending_dx: 0,
            pending_dy: 0,
        }
    }

    /// تجميع حركات الماوس لتفادي معالجة كل إزاحة على حدة (Event Coalescing)
    pub fn feed_raw_move(&mut self, dx: i32, dy: i32) {
        self.pending_dx += dx;
        self.pending_dy += dy;
    }

    /// استخراج الحركة المتراكمة كحدث واحد متكامل
    pub fn flush_move(&mut self) -> Option<PointerEvent> {
        if self.pending_dx == 0 && self.pending_dy == 0 {
            return None;
        }

        let scaled_dx = (self.pending_dx as f32 * self.sensitivity).round() as i32;
        let scaled_dy = (self.pending_dy as f32 * self.sensitivity).round() as i32;

        self.pending_dx = 0;
        self.pending_dy = 0;

        self.state.x = (self.state.x + scaled_dx).clamp(0, self.screen_w - 1);
        self.state.y = (self.state.y + scaled_dy).clamp(0, self.screen_h - 1);

        Some(PointerEvent::Move {
            x: self.state.x,
            y: self.state.y,
            dx: scaled_dx,
            dy: scaled_dy,
        })
    }

    pub fn feed_button(&mut self, button_idx: u8, pressed: bool, current_tick: u64) -> PointerEvent {
        let btn = match button_idx {
            1 => PointerButton::Right,
            2 => PointerButton::Middle,
            _ => PointerButton::Left,
        };

        let idx = button_idx as usize;
        if idx < 3 {
            self.state.buttons[idx] = pressed;
        }

        if pressed {
            // فحص النقر المزدوج (Double-Click Detection)
            let is_double = if let Some(last_btn) = self.click_tracker.last_button {
                last_btn == btn
                    && current_tick.saturating_sub(self.click_tracker.last_tick) < 350
                    && (self.state.x - self.click_tracker.last_x).abs() < 6
                    && (self.state.y - self.click_tracker.last_y).abs() < 6
            } else {
                false
            };

            self.click_tracker.last_button = Some(btn);
            self.click_tracker.last_x = self.state.x;
            self.click_tracker.last_y = self.state.y;
            self.click_tracker.last_tick = current_tick;

            if is_double {
                PointerEvent::DoubleClick { x: self.state.x, y: self.state.y, button: btn }
            } else {
                PointerEvent::Down { x: self.state.x, y: self.state.y, button: btn }
            }
        } else {
            // إفلات الاحتجاز تلقائياً عند رفع الزر الأيسر
            if btn == PointerButton::Left {
                self.capture = None;
            }
            PointerEvent::Up { x: self.state.x, y: self.state.y, button: btn }
        }
    }

    pub fn set_capture(&mut self, id: usize) {
        self.capture = Some(id);
    }

    pub fn release_capture(&mut self) {
        self.capture = None;
    }
}
