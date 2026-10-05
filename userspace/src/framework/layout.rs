use crate::framework::{Canvas, Rect, Widget};
use std::any::Any;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Start,
    Center,
    End,
    Stretch,
}

pub struct LayoutItem {
    pub widget: Box<dyn Widget>,
    pub flex_grow: usize,
    pub min_size: (i32, i32),
}

pub struct ContainerWidget {
    pub bounds: Rect,
    pub direction: Direction,
    pub alignment: Alignment,
    pub spacing: i32,
    pub padding: i32,
    pub children: Vec<LayoutItem>,
}

impl ContainerWidget {
    pub fn new(direction: Direction) -> Self {
        Self {
            bounds: Rect::default(),
            direction,
            alignment: Alignment::Stretch,
            spacing: 8,
            padding: 8,
            children: Vec::new(),
        }
    }

    pub fn vbox() -> Self {
        Self::new(Direction::Vertical)
    }

    pub fn hbox() -> Self {
        Self::new(Direction::Horizontal)
    }

    pub fn with_spacing(mut self, spacing: i32) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn with_padding(mut self, padding: i32) -> Self {
        self.padding = padding;
        self
    }

    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn add_child(&mut self, widget: Box<dyn Widget>, flex_grow: usize, min_size: (i32, i32)) {
        self.children.push(LayoutItem {
            widget,
            flex_grow,
            min_size,
        });
    }
}

impl Widget for ContainerWidget {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect { x, y, w, h };
        if self.children.is_empty() {
            return self.bounds;
        }

        let inner_x = x + self.padding;
        let inner_y = y + self.padding;
        let inner_w = (w - self.padding * 2).max(0);
        let inner_h = (h - self.padding * 2).max(0);

        let count = self.children.len() as i32;
        let total_spacing = self.spacing * (count - 1).max(0);

        match self.direction {
            Direction::Vertical => {
                let avail_h = (inner_h - total_spacing).max(0);
                let total_min_h: i32 = self.children.iter().map(|c| c.min_size.1).sum();
                let extra_h = (avail_h - total_min_h).max(0);
                let total_flex: usize = self.children.iter().map(|c| c.flex_grow).sum();

                let mut current_y = inner_y;
                for item in self.children.iter_mut() {
                    let mut item_h = item.min_size.1;
                    if total_flex > 0 && item.flex_grow > 0 {
                        item_h += (extra_h * (item.flex_grow as i32)) / (total_flex as i32);
                    }

                    let (child_x, child_w) = match self.alignment {
                        Alignment::Stretch => (inner_x, inner_w),
                        Alignment::Start => (inner_x, item.min_size.0.min(inner_w)),
                        Alignment::Center => {
                            let cw = item.min_size.0.min(inner_w);
                            (inner_x + (inner_w - cw) / 2, cw)
                        }
                        Alignment::End => {
                            let cw = item.min_size.0.min(inner_w);
                            (inner_x + inner_w - cw, cw)
                        }
                    };

                    item.widget.layout(child_x, current_y, child_w, item_h);
                    current_y += item_h + self.spacing;
                }
            }
            Direction::Horizontal => {
                let avail_w = (inner_w - total_spacing).max(0);
                let total_min_w: i32 = self.children.iter().map(|c| c.min_size.0).sum();
                let extra_w = (avail_w - total_min_w).max(0);
                let total_flex: usize = self.children.iter().map(|c| c.flex_grow).sum();

                let mut current_x = inner_x;
                for item in self.children.iter_mut() {
                    let mut item_w = item.min_size.0;
                    if total_flex > 0 && item.flex_grow > 0 {
                        item_w += (extra_w * (item.flex_grow as i32)) / (total_flex as i32);
                    }

                    let (child_y, child_h) = match self.alignment {
                        Alignment::Stretch => (inner_y, inner_h),
                        Alignment::Start => (inner_y, item.min_size.1.min(inner_h)),
                        Alignment::Center => {
                            let ch = item.min_size.1.min(inner_h);
                            (inner_y + (inner_h - ch) / 2, ch)
                        }
                        Alignment::End => {
                            let ch = item.min_size.1.min(inner_h);
                            (inner_y + inner_h - ch, ch)
                        }
                    };

                    item.widget.layout(current_x, child_y, item_w, child_h);
                    current_x += item_w + self.spacing;
                }
            }
        }

        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        for item in &self.children {
            item.widget.paint(canvas);
        }
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, pressed: bool) -> bool {
        if !self.bounds.contains(mx, my) {
            return false;
        }
        for item in self.children.iter_mut().rev() {
            if item.widget.handle_mouse(mx, my, pressed) {
                return true;
            }
        }
        true
    }

    fn handle_scroll(&mut self, mx: i32, my: i32, dy: i32) -> bool {
        if !self.bounds.contains(mx, my) {
            return false;
        }
        for item in self.children.iter_mut().rev() {
            if item.widget.handle_scroll(mx, my, dy) {
                return true;
            }
        }
        false
    }

    fn handle_key(&mut self, keycode: u8, mods: u8) -> bool {
        for item in self.children.iter_mut() {
            if item.widget.handle_key(keycode, mods) {
                return true;
            }
        }
        false
    }

    fn handle_char(&mut self, c: char) -> bool {
        for item in self.children.iter_mut() {
            if item.widget.handle_char(c) {
                return true;
            }
        }
        false
    }
}
