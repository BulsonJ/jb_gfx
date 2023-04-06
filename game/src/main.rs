use cgmath::{Array, Deg, InnerSpace, Matrix4, Quaternion, Rotation3, Vector3, Zero};
use jb_gfx::asset::AssetManager;
use jb_gfx::renderer::Renderer;
use jb_gfx::Colour;
use std::time::Instant;
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
    // Load sponza
    {
        let models = asset_manager
            .load_model(&mut renderer, "assets/models/Sponza/glTF/Sponza.gltf")
            .unwrap();
        for model in models.iter() {
            renderer.add_render_model(model.mesh, model.material_instance.clone());
        }
    }
    // Load helmet
    {
        let models = asset_manager
            .load_model(
                &mut renderer,
                "assets/models/DamagedHelmet/glTF/DamagedHelmet.gltf",
            )
            .unwrap();
        for model in models.iter() {
            let helmet = renderer.add_render_model(model.mesh, model.material_instance.clone());
            renderer
                .set_render_model_transform(
                    helmet,
                    from_transforms(
                        Vector3::new(0f32, 100f32, -4.0f32),
                        Quaternion::from_axis_angle(
                            Vector3::new(1f32, 0f32, 0.0f32).normalize(),
                            Deg(100f32),
                        ) * Quaternion::from_axis_angle(
                            Vector3::new(0f32, 0f32, 1.0f32).normalize(),
                            Deg(20f32),
                        ),
                        Vector3::from_value(2f32),
                    ),
                )
                .unwrap();
        }
    }
    renderer.clear_colour = Colour::new(0.0, 0.1, 0.3);

    let mut initial_resize = true;
    let mut frame_start_time = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::MainEventsCleared => {
                let delta_time = frame_start_time.elapsed().as_secs_f64();
                frame_start_time = Instant::now();

                // Check if need to skip frame
                let frame_skip = delta_time > 0.1f64;
                if frame_skip {
                    //warn!("Skipping large time delta");
                } else {
                    renderer.render().unwrap();
                }
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

fn from_transforms(
    position: Vector3<f32>,
    rotation: Quaternion<f32>,
    size: Vector3<f32>,
) -> Matrix4<f32> {
    let translation = Matrix4::from_translation(position);
    let rotation = Matrix4::from(rotation);
    let scale = Matrix4::from_nonuniform_scale(size.x, size.y, size.z);

    translation * rotation * scale
}
