use env_logger::{Builder, Target};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use engine::prelude::*;
use game::turret_game::TurretGame;

fn main() {
    #[cfg(feature = "tracy")]
    profiling::tracy_client::Client::start();
    profiling::register_thread!("Main Thread");
    profiling::scope!("Game");

    // Enable logging
    let mut builder = Builder::from_default_env();
    builder.target(Target::Stdout);
    builder.init();

    run_game()
}

pub fn run_game() {
    let (screen_width, screen_height) = (1920, 1080);
    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(screen_width, screen_height))
        .with_title("Rust Renderer")
        .build(&event_loop)
        .unwrap();

    let mut game = TurretGame::new(window, &event_loop);

    let mut initial_resize = true;

    let mut frame_timer = FrameTimer::new();

    profiling::scope!("Game Event Loop");
    {
        event_loop.run(move |event, _, control_flow| {
            match event {
                Event::MainEventsCleared => {
                    frame_timer.update();

                    while frame_timer.sub_frame_update() {
                        game.delta_time = frame_timer.delta_time();
                        game.time_passed = frame_timer.total_time_elapsed();

                        game.update();
                        game.renderer.tick_particle_systems(game.delta_time);
                    }

                    game.draw_ui();

                    game.renderer.render().unwrap();
                }
                Event::NewEvents(_) => {
                    game.input.prev_keys.copy_from_slice(&game.input.now_keys);
                }
                Event::WindowEvent { ref event, .. } => {
                    let response = game.on_window_event(event);
                    if !response.consumed {
                        game.input.update_input_from_event(event);
                    }
                    match event {
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
                        WindowEvent::Resized(physical_size) => {
                            if initial_resize {
                                initial_resize = false;
                            } else {
                                game.renderer.resize(*physical_size).unwrap();
                            }
                        }
                        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                            game.renderer.resize(**new_inner_size).unwrap();
                        }
                        _ => {}
                    }
                }
                _ => {}
            };
            profiling::finish_frame!();
        });
    }
}
