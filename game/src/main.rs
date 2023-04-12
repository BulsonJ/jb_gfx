use cgmath::{Array, Deg, InnerSpace, Matrix4, Quaternion, Rotation3, Vector3};
use env_logger::{Builder, Target};
use jb_gfx::asset::AssetManager;
use jb_gfx::renderer::{Light, LightHandle, Renderer};
use jb_gfx::Colour;
use std::time::Instant;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

fn main() {
    // Enable logging
    let mut builder = Builder::from_default_env();
    builder.target(Target::Stdout);
    builder.init();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(1920, 1080))
        .with_title("Rust Renderer")
        .build(&event_loop)
        .unwrap();

    let mut renderer = Renderer::new(&window).unwrap();
    renderer.render().unwrap();
    let mut asset_manager = AssetManager::default();
    // Load cube
    {
        let models = asset_manager
            .load_gltf(&mut renderer, "assets/models/Cube/glTF/Cube.gltf")
            .unwrap();
        for model in models.iter() {
            let render_model =
                renderer.add_render_model(model.mesh, model.material_instance.clone());
            renderer.light_mesh = Some(render_model);
        }
    }
    // Load sponza
    //{
    //    let models = asset_manager
    //        .load_gltf(&mut renderer, "assets/models/Sponza/glTF/Sponza.gltf")
    //        .unwrap();
    //    for model in models.iter() {
    //        renderer.add_render_model(model.mesh, model.material_instance.clone());
    //    }
    //}
    // Load helmet
    {
        let models = asset_manager
            .load_gltf(
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
                        Vector3::new(5f32, 100f32, 0.0f32),
                        Quaternion::from_axis_angle(
                            Vector3::new(1f32, 0f32, 0.0f32).normalize(),
                            Deg(100f32),
                        ) * Quaternion::from_axis_angle(
                            Vector3::new(0f32, 0f32, 1.0f32).normalize(),
                            Deg(60f32),
                        ),
                        Vector3::from_value(2f32),
                    ),
                )
                .unwrap();
        }
    }
    renderer.clear_colour = Colour::new(0.0, 0.1, 0.3);

    let initial_lights = vec![
        Light::new(
            Vector3::new(5.0f32, 95.0f32, -4.0f32),
            Vector3::new(1.0f32, 0.0f32, 0.0f32),
        ),
        Light::new(
            Vector3::new(-5.0f32, 105.0f32, 4.0f32),
            Vector3::new(0.0f32, 1.0f32, 0.0f32),
        ),
        Light::new(
            Vector3::new(5.0f32, 105.0f32, -4.0f32),
            Vector3::new(0.0f32, 0.0f32, 1.0f32),
        ),
        Light::new(
            Vector3::new(-5.0f32, 95.0f32, 4.0f32),
            Vector3::new(1.0f32, 1.0f32, 1.0f32),
        ),
    ];

    let mut lights = vec![
        LightComponent {
            handle: renderer
                .create_light(initial_lights.get(0).unwrap())
                .unwrap(),
            light: *initial_lights.get(0).unwrap(),
        },
        LightComponent {
            handle: renderer
                .create_light(initial_lights.get(1).unwrap())
                .unwrap(),
            light: *initial_lights.get(1).unwrap(),
        },
        LightComponent {
            handle: renderer
                .create_light(initial_lights.get(2).unwrap())
                .unwrap(),
            light: *initial_lights.get(2).unwrap(),
        },
        LightComponent {
            handle: renderer
                .create_light(initial_lights.get(3).unwrap())
                .unwrap(),
            light: *initial_lights.get(3).unwrap(),
        },
    ];

    let mut initial_resize = true;
    let mut frame_start_time = Instant::now();
    let mut time_passed = 0f64;
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::MainEventsCleared => {
                let delta_time = frame_start_time.elapsed().as_secs_f64();
                frame_start_time = Instant::now();
                time_passed += delta_time;

                // Update lights
                for (i, component) in lights.iter_mut().enumerate() {
                    let position = 10f32 + ((i as f32 + 3f32 * time_passed as f32).sin() * 5f32);
                    component.light.position = Vector3::new(
                        position,
                        component.light.position.y,
                        component.light.position.z,
                    );
                    renderer
                        .set_light(component.handle, &component.light)
                        .unwrap();
                }

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

#[derive(Copy, Clone)]
struct LightComponent {
    handle: LightHandle,
    pub light: Light,
}
