use crate::asset::AssetManager;
use crate::input::Input;
use crate::project::Project;
use env_logger::{Builder, Target};
use jb_gfx::renderer::Renderer;
use std::time::Instant;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

pub struct Application {
    pub window: Window,
    pub input: Input,
    pub renderer: Renderer,
    pub asset_manager: AssetManager,
    pub delta_time: f32,
    pub time_passed: f32,
}

pub fn run_game<T: Project + 'static>() {
    let (screen_width, screen_height) = (1920, 1080);
    let event_loop = EventLoop::new();
    let mut app = {
        let input = Input {
            now_keys: [false; 255],
            prev_keys: [false; 255],
        };

        // TODO: Fix this config flag not being set for some reason
        //#[cfg(feature = "profile-with-tracy")]
        profiling::tracy_client::Client::start();
        profiling::register_thread!("Main Thread");
        profiling::scope!("Game");

        // Enable logging
        let mut builder = Builder::from_default_env();
        builder.target(Target::Stdout);
        builder.init();

        let window = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(screen_width, screen_height))
            .with_title("Rust Renderer")
            .build(&event_loop)
            .unwrap();

        let mut renderer = Renderer::new(&window).unwrap();
        renderer.render().unwrap();
        let asset_manager = AssetManager::default();

        Application {
            window,
            input,
            renderer,
            asset_manager,
            delta_time: 0.0,
            time_passed: 0.0,
        }
    };

    let mut project = T::new(&mut app, &event_loop);

    let mut initial_resize = true;

    let mut frame_start_time = Instant::now();
    let mut t = 0.0;
    let target_dt = 1.0 / 60.0;

    event_loop.run(move |event, _, control_flow| {
        profiling::scope!("Game Event Loop");
        match event {
            Event::MainEventsCleared => {
                let mut frame_time = frame_start_time.elapsed().as_secs_f32();
                frame_start_time = Instant::now();

                while frame_time > 0.0f32 {
                    let delta_time = frame_time.min(target_dt);

                    // Update
                    app.delta_time = delta_time;
                    app.time_passed = t;
                    project.update(&mut app);

                    frame_time -= delta_time;
                    t += delta_time;
                }

                project.draw(&mut app);

                app.renderer.render().unwrap();
            }
            Event::NewEvents(_) => {
                app.input.prev_keys.copy_from_slice(&app.input.now_keys);
            }
            Event::WindowEvent { ref event, .. } => match event {
                WindowEvent::CloseRequested
                | WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            ..
                        },
                    ..
                } => *control_flow = ControlFlow::Exit,
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state,
                            virtual_keycode: Some(keycode),
                            ..
                        },
                    ..
                } => match state {
                    ElementState::Pressed => {
                        app.input.now_keys[*keycode as usize] = true;
                    }
                    ElementState::Released => {
                        app.input.now_keys[*keycode as usize] = false;
                    }
                },
                WindowEvent::Resized(physical_size) => {
                    if initial_resize {
                        initial_resize = false;
                    } else {
                        app.renderer.resize(*physical_size).unwrap();
                    }
                }
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    app.renderer.resize(**new_inner_size).unwrap();
                }
                event => {
                    project.on_window_event(event);
                }
            },
            _ => {}
        };
        profiling::finish_frame!()
    });
}
