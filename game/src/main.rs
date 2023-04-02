use jb_gfx::asset::AssetManager;
use jb_gfx::renderer::{Colour, MaterialTextures, Renderer};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(1920, 1080))
        .with_title("Rust Renderer")
        .build(&event_loop)
        .unwrap();

    let mut renderer = Renderer::new(&window).unwrap();
    renderer.render().unwrap();
    let mut asset_manager = AssetManager::default();
    let models = asset_manager
        .load_model(&mut renderer, "assets/models/MetalRoughSpheres/glTF/MetalRoughSpheres.gltf")
        .unwrap();
    for model in models.iter() {
        renderer.add_render_model(model.mesh, model.textures.clone());
    }
    renderer.clear_colour = Colour::CUSTOM(0.0, 0.1, 0.3);

    let mut initial_resize = true;
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::MainEventsCleared => {
                renderer.render().unwrap();
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
                WindowEvent::Resized(physical_size) => {
                    if initial_resize {
                        initial_resize = false;
                    } else {
                        renderer.resize(*physical_size).unwrap();
                    }
                }
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    renderer.resize(**new_inner_size).unwrap();
                }
                _ => {}
            },
            _ => {}
        };
    });
}
