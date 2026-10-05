use crate::framework::*;

pub struct PreviewApp {
    bounds: Rect,
    pub title: String,
    pub image_data: Vec<u8>,
}

impl PreviewApp {
    pub fn new(title: String, image_data: Vec<u8>) -> Self {
        Self {
            bounds: Rect::default(),
            title,
            image_data,
        }
    }
}

impl Widget for PreviewApp {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn layout(&mut self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        self.bounds = Rect::new(x, y, w, h);
        self.bounds
    }

    fn paint(&self, canvas: &mut Canvas) {
        let theme = get_theme();
        canvas.draw_rect(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h, 0xFF0F172A, 0);

        let info = format!("Image: {} ({} bytes)", self.title, self.image_data.len());
        canvas.draw_text(self.bounds.x + theme.pt(24.0), self.bounds.y + theme.pt(24.0), &info, 0xFFF8FAFC, theme.font_body());
    }

    fn handle_mouse(&mut self, mx: i32, my: i32, _pressed: bool) -> bool {
        self.bounds.contains(mx, my)
    }
}
