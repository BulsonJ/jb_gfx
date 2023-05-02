use crate::input::Input;
use engine::prelude::*;
use jb_gfx::Renderer;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

pub struct Application {
    pub window: Window,
    pub input: Input,
    pub renderer: Renderer,
    pub asset_manager: AssetManager,
    pub delta_time: f32,
    pub time_passed: f32,
}

impl Application {
    pub fn new(screen_width: i32, screen_height: i32, event_loop: &EventLoop<()>) -> Self {
        let input = Input::default();

        let window = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(screen_width, screen_height))
            .with_title("Rust Renderer")
            .build(&event_loop)
            .unwrap();

        let mut renderer = Renderer::new(&window).unwrap();
        renderer.render().unwrap();
        let asset_manager = AssetManager::default();

        Self {
            window,
            input,
            renderer,
            asset_manager,
            delta_time: 0.0,
            time_passed: 0.0,
        }
    }
}
