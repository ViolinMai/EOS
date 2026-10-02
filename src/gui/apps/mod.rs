
pub mod terminal;
pub mod finder;
pub mod settings;
pub mod userwin;
pub mod preview;
pub mod textedit;
pub mod taskmanager;

use alloc::string::String;
use alloc::vec::Vec;
use crate::input::InputEvent;
use crate::gui::draw::FrameBuffer;
use crate::gui::theme::Theme;

pub enum AppAction {
    OpenImage { name: String, width: usize, height: usize, pixels: Vec<u32> },
    OpenText { name: String, content: String },
    LaunchElf { name: String, arg: String },
}

pub trait App {
    fn draw(&mut self, fb: &mut FrameBuffer, theme: &Theme, x: usize, y: usize, w: usize, h: usize);
    fn on_event(&mut self, event: &InputEvent, mx: isize, my: isize);
    fn title(&self) -> &str;
    fn poll_action(&mut self) -> Option<AppAction> { None }
    fn on_resize(&mut self, _new_w: usize, _new_h: usize) {}
    fn wants_redraw(&self) -> bool { false } // Explicitly prevent unnecessary 60 FPS rebuilding
    fn menu_items(&self) -> &'static [&'static str] { &[] }
    fn icon(&self) -> &'static str { "APP" }
}
