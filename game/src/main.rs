use std::collections::HashMap;
use std::time::Instant;

use cgmath::{Array, Deg, InnerSpace, Matrix4, Point3, Quaternion, Rotation3, Vector3, Zero};
use egui::epaint::Primitive;
use egui::{Context, TextureId};
use env_logger::{Builder, Target};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use jb_gfx::asset::AssetManager;
use jb_gfx::device::ImageFormatType;
use jb_gfx::renderer::{Renderer, UIMesh, UIVertex};
use jb_gfx::resource::ImageHandle;
use jb_gfx::{Camera, Colour, Light};

use crate::components::{CameraComponent, LightComponent};
use crate::input::Input;

mod components;
mod input;

fn main() {
    let mut input = Input {
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

    let (screen_width, screen_height) = (1920, 1080);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(screen_width, screen_height))
        .with_title("Rust Renderer")
        .build(&event_loop)
        .unwrap();

    let mut renderer = Renderer::new(&window).unwrap();
    renderer.render().unwrap();
    let mut asset_manager = AssetManager::default();
    // Load cube
    //{
    //    let models = asset_manager
    //        .load_gltf(&mut renderer, "assets/models/Cube/glTF/Cube.gltf")
    //        .unwrap();
    //    for model in models.iter() {
    //        let render_model =
    //            renderer.add_render_model(model.mesh, model.material_instance.clone());
    //        renderer.light_mesh = Some(model.mesh);
    //    }
    //}
    // Load sponza
    //{
    //    let models = asset_manager
    //        .load_gltf(&mut renderer, "assets/models/Sponza/glTF/Sponza.gltf")
    //        .unwrap();
    //    for model in models.iter() {
    //        let handle = renderer.add_render_model(model.mesh, model.material_instance.clone());
    //        renderer
    //            .set_render_model_transform(
    //                handle,
    //                from_transforms(
    //                    Vector3::new(0f32, 80f32, 0.0f32),
    //                    Quaternion::from_axis_angle(
    //                        Vector3::new(0f32, 1f32, 0.0f32).normalize(),
    //                        Deg(180f32),
    //                    ),
    //                    Vector3::from_value(0.1f32),
    //                ),
    //            )
    //            .unwrap();
    //    }
    //}
    // Load helmet
    //{
    //    let models = asset_manager
    //        .load_gltf(
    //            &mut renderer,
    //            "assets/models/DamagedHelmet/glTF/DamagedHelmet.gltf",
    //        )
    //        .unwrap();
    //    for model in models.iter() {
    //        let helmet = renderer.add_render_model(model.mesh, model.material_instance.clone());
    //        renderer
    //            .set_render_model_transform(
    //                helmet,
    //                from_transforms(
    //                    Vector3::new(10f32, 100f32, 0.0f32),
    //                    Quaternion::from_axis_angle(
    //                        Vector3::new(1f32, 0f32, 0.0f32).normalize(),
    //                        Deg(100f32),
    //                    ) * Quaternion::from_axis_angle(
    //                        Vector3::new(0f32, 0f32, 1.0f32).normalize(),
    //                        Deg(60f32),
    //                    ),
    //                    Vector3::from_value(6f32),
    //                ),
    //            )
    //            .unwrap();
    //    }
    //}
    renderer.clear_colour = Colour::new(0.0, 0.1, 0.3);

    let (mut lights, cameras) =
        setup_scene(&mut renderer, (screen_width as u32, screen_height as u32));

    let mut initial_resize = true;

    let mut frame_start_time = Instant::now();
    let mut t = 0.0;
    let target_dt = 1.0 / 60.0;

    let mut ctx = egui::Context::default();
    let mut stored_textures = HashMap::default();

    event_loop.run(move |event, _, control_flow| {
        profiling::scope!("Game Event Loop");
        match event {
            Event::MainEventsCleared => {
                let mut frame_time = frame_start_time.elapsed().as_secs_f32();
                frame_start_time = Instant::now();

                if input.is_just_pressed(VirtualKeyCode::F5) {
                    renderer.reload_shaders().unwrap();
                }
                if input.is_just_pressed(VirtualKeyCode::Key1) {
                    if let Some(camera) = cameras.get(0) {
                        renderer.active_camera = Some(camera.handle);
                    }
                }
                if input.is_just_pressed(VirtualKeyCode::Key2) {
                    if let Some(camera) = cameras.get(1) {
                        renderer.active_camera = Some(camera.handle);
                    }
                }

                while frame_time > 0.0f32 {
                    let delta_time = frame_time.min(target_dt);

                    // Update
                    for (i, component) in lights.iter_mut().enumerate() {
                        let position = 10f32 + ((i as f32 + 3f32 * t).sin() * 5f32);
                        component.light.position.x = position;
                    }

                    frame_time -= delta_time;
                    t += delta_time;
                }

                // Test EGUI
                paint_egui(&mut renderer, &ctx, &mut stored_textures);

                // Update render objects & then render
                update_renderer_object_states(&mut renderer, &lights, &cameras);
                renderer.render().unwrap();
            }
            Event::NewEvents(_) => {
                input.prev_keys.copy_from_slice(&input.now_keys);
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
                        input.now_keys[*keycode as usize] = true;
                    }
                    ElementState::Released => {
                        input.now_keys[*keycode as usize] = false;
                    }
                },
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
        profiling::finish_frame!()
    });
}

fn paint_egui(
    renderer: &mut Renderer,
    ctx: &Context,
    stored_textures: &mut HashMap<TextureId, ImageHandle>,
) {
    let raw_input = egui::RawInput::default();
    let full_output = ctx.run(raw_input, |ctx| {
        egui::CentralPanel::default().show(&ctx, |ui| {
            ui.label("Hello world!");
            if ui.button("Click me").clicked() {
                // take some action here
            }
        });
    });
    //handle_platform_output(full_output.platform_output);
    let clipped_primitives = ctx.tessellate(full_output.shapes); // create triangles to paint
                                                                 //paint(full_output.textures_delta, clipped_primitives);
    for (id, delta) in full_output.textures_delta.set.iter() {
        let data: Vec<u8> = match &delta.image {
            egui::ImageData::Color(image) => {
                assert_eq!(
                    image.width() * image.height(),
                    image.pixels.len(),
                    "Mismatch between texture size and texel count"
                );
                image
                    .pixels
                    .iter()
                    .flat_map(|color| color.to_array())
                    .collect()
            }
            egui::ImageData::Font(image) => image
                .srgba_pixels(None)
                .flat_map(|color| color.to_array())
                .collect(),
        };

        if stored_textures.contains_key(id) {
            continue;
        }

        let image = renderer.load_texture_from_bytes(
            &data,
            delta.image.width() as u32,
            delta.image.height() as u32,
            &ImageFormatType::Default,
            1,
        );
        stored_textures.insert(*id, image.unwrap());
    }

    // Paint meshes
    for prim in clipped_primitives.into_iter() {
        match prim.primitive {
            Primitive::Mesh(mesh) => {
                let ui_verts = mesh
                    .vertices
                    .iter()
                    .map(|vert| UIVertex {
                        pos: vert.pos.into(),
                        uv: vert.uv.into(),
                        colour: vert.color.to_array().map(|colour| colour as f32 / 255f32),
                    })
                    .collect();

                let texture_id = {
                    if let Some(image) = stored_textures.get(&mesh.texture_id) {
                        *image
                    } else {
                        Default::default()
                    }
                };

                let ui_mesh = UIMesh {
                    indices: mesh.indices,
                    vertices: ui_verts,
                    texture_id,
                };
                renderer.draw_ui(ui_mesh).unwrap();
            }
            Primitive::Callback(_) => {}
        }
    }
}

#[profiling::function]
fn setup_scene(
    renderer: &mut Renderer,
    screen_size: (u32, u32),
) -> (Vec<LightComponent>, Vec<CameraComponent>) {
    let initial_lights = vec![
        Light::new(
            Point3::new(10.0f32, 95.0f32, -16.0f32),
            Vector3::new(3.0f32, 0.0f32, 0.0f32),
        ),
        Light::new(
            Point3::new(-10.0f32, 105.0f32, 16.0f32),
            Vector3::new(0.0f32, 3.0f32, 0.0f32),
        ),
        Light::new(
            Point3::new(10.0f32, 105.0f32, -16.0f32),
            Vector3::new(1.0f32, 1.0f32, 1.0f32),
        ),
        Light::new(
            Point3::new(-10.0f32, 95.0f32, 16.0f32),
            Vector3::new(1.0f32, 1.0f32, 1.0f32),
        ),
    ];

    let mut light_components = vec![
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

    let cameras = vec![
        {
            let camera = Camera {
                position: (-8.0, 100.0, 0.0).into(),
                direction: (1.0, 0.0, 0.0).into(),
                aspect: screen_size.0 as f32 / screen_size.1 as f32,
                fovy: 90.0,
                znear: 0.1,
                zfar: 4000.0,
            };
            CameraComponent {
                handle: renderer.create_camera(&camera),
                camera,
            }
        },
        {
            let camera = Camera {
                position: (-50.0, 100.0, 20.0).into(),
                direction: (1.0, 0.25, -0.5).into(),
                aspect: screen_size.0 as f32 / screen_size.1 as f32,
                fovy: 90.0,
                znear: 0.1,
                zfar: 4000.0,
            };
            CameraComponent {
                handle: renderer.create_camera(&camera),
                camera,
            }
        },
    ];
    renderer.active_camera = Some(cameras.get(0).unwrap().handle);

    (light_components, cameras)
}

#[profiling::function]
fn update_renderer_object_states(
    renderer: &mut Renderer,
    light_components: &[LightComponent],
    camera_component: &[CameraComponent],
) {
    for component in light_components.iter() {
        renderer
            .set_light(component.handle, &component.light)
            .unwrap();
    }
    for component in camera_component.iter() {
        renderer
            .set_camera(component.handle, &component.camera)
            .unwrap();
    }
}

#[profiling::function]
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
