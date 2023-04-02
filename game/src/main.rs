use jb_gfx::asset::AssetManager;
use jb_gfx::renderer::{Colour, MaterialTextures, Renderer};
use jb_gfx::Mesh;
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

    let mut renderer = Renderer::new(&window);
    let mut asset_manager = AssetManager::default();
    let models = asset_manager.load_model(&mut renderer, "assets/models/Sponza/glTF/Sponza.gltf");
    for model in models.iter() {
        renderer.add_render_model(
            model.mesh,
            MaterialTextures {
                diffuse: model.diffuse_texture.unwrap(),
            },
        );
    }
    renderer.clear_colour = Colour::CUSTOM(0.0, 0.1, 0.3);

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::MainEventsCleared => {
                renderer.render();
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
                _ => {}
            },
            _ => {}
        };
    });
}
