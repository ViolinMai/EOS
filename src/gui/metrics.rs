use crate::config::CONFIG;

#[derive(Clone, Copy)]
pub struct UiMetrics {
    pub scale: usize,
    pub titlebar_height: usize,
    pub control_height: usize,
    pub border_width: usize,
    pub corner_radius: usize,
    pub padding_sm: usize,
    pub padding_md: usize,
    pub padding_lg: usize,
    pub icon_sm: usize,
    pub icon_md: usize,
    pub icon_lg: usize,
    pub font_caption: usize,
    pub font_body: usize,
    pub font_title: usize,
    pub font_heading: usize,
}

pub fn get_metrics() -> UiMetrics {
    let scale = CONFIG.get_ui_scale();
    UiMetrics {
        scale,
        titlebar_height: 22 + (scale * 8),
        control_height: 16 + (scale * 8),
        border_width: 1,
        corner_radius: 6 + (scale * 2),
        padding_sm: 2 + (scale * 2),
        padding_md: 4 + (scale * 4),
        padding_lg: 8 + (scale * 8),
        icon_sm: 12 + (scale * 4),
        icon_md: 16 + (scale * 8),
        icon_lg: 24 + (scale * 12),
        font_caption: 10 + (scale * 2),
        font_body: 12 + (scale * 3),
        font_title: 14 + (scale * 4),
        font_heading: 18 + (scale * 6),
    }
}
