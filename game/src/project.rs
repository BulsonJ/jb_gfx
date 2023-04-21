use crate::application::Application;
use winit::event::WindowEvent;
use winit::event_loop::EventLoop;

pub trait Project {
    fn new(ctx: &mut Application, event_loop: &EventLoop<()>) -> Self;
    fn update(&mut self, ctx: &mut Application);
    fn draw(&mut self, ctx: &mut Application);
    fn on_window_event(&mut self, event: &WindowEvent);
}
