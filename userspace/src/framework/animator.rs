use crate::framework::widget::Rect;
use crate::framework::damage::DamageRegion;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    OutCubic,
    InOutCubic,
    OutBack,
}

#[derive(Clone, Copy)]
pub struct Tween {
    pub key: u32,
    pub from: f32,
    pub to: f32,
    pub start_ms: u64,
    pub dur_ms: u16,
    pub ease: Easing,
    pub dirty: Rect,
}

pub struct Animator {
    pub tweens: [Option<Tween>; 32],
}

impl Animator {
    pub fn new() -> Self {
        Self {
            tweens: [const { None }; 32],
        }
    }

    pub fn is_active(&self) -> bool {
        self.tweens.iter().any(|t| t.is_some())
    }

    pub fn value(&self, key: u32, default: f32) -> f32 {
        for t_opt in &self.tweens {
            if let Some(ref t) = t_opt {
                if t.key == key {
                    return t.from;
                }
            }
        }
        default
    }

    pub fn animate(&mut self, key: u32, to: f32, dur_ms: u16, ease: Easing, dirty: Rect, now_ms: u64) {
        let current_val = self.value(key, to);
        let mut slot = None;

        for (i, t_opt) in self.tweens.iter_mut().enumerate() {
            if let Some(ref mut t) = t_opt {
                if t.key == key {
                    t.from = current_val;
                    t.to = to;
                    t.start_ms = now_ms;
                    t.dur_ms = dur_ms;
                    t.ease = ease;
                    t.dirty = dirty;
                    return;
                }
            } else if slot.is_none() {
                slot = Some(i);
            }
        }

        if let Some(idx) = slot {
            self.tweens[idx] = Some(Tween {
                key,
                from: current_val,
                to,
                start_ms: now_ms,
                dur_ms,
                ease,
                dirty,
            });
        }
    }

    pub fn tick(&mut self, now_ms: u64, damage: &mut DamageRegion) -> bool {
        let mut any_running = false;

        for t_opt in self.tweens.iter_mut() {
            if let Some(mut t) = t_opt.take() {
                damage.add(t.dirty);
                let elapsed = now_ms.saturating_sub(t.start_ms);

                if elapsed >= t.dur_ms as u64 {
                    // Animation completed
                } else {
                    let progress = (elapsed as f32 / t.dur_ms as f32).clamp(0.0, 1.0);
                    let factor = match t.ease {
                        Easing::Linear => progress,
                        Easing::OutCubic => 1.0 - (1.0 - progress).powi(3),
                        Easing::InOutCubic => {
                            if progress < 0.5 {
                                4.0 * progress.powi(3)
                            } else {
                                1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0
                            }
                        }
                        Easing::OutBack => {
                            let c1 = 1.70158;
                            let c3 = c1 + 1.0;
                            1.0 + c3 * (progress - 1.0).powi(3) + c1 * (progress - 1.0).powi(2)
                        }
                    };
                    t.from = t.from + (t.to - t.from) * factor;
                    *t_opt = Some(t);
                    any_running = true;
                }
            }
        }
        any_running
    }
}
