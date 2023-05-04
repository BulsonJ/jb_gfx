use cgmath::{
    Array, Deg, EuclideanSpace, InnerSpace, Matrix4, Point3, Quaternion, Rotation, Rotation3,
    Vector3, Vector4, Zero,
};
use egui_winit::EventResponse;
use env_logger::{Builder, Target};
use kira::manager::backend::cpal::CpalBackend;
use kira::manager::{AudioManager, AudioManagerSettings};
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use log::info;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

use engine::prelude::*;
use game::app::Application;
use game::components::{CameraComponent, LightComponent};
use game::egui_context::EguiContext;
use game::{debug_ui, Camera};
use jb_gfx::prelude::*;
use jb_gfx::renderer::RenderModelHandle;

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

struct TurretGame {
    lights: Vec<LightComponent>,
    player: Player,
    egui: EguiContext,
    audio_manager: AudioManager,
    background_music: StaticSoundData,
    draw_debug_ui: bool,
    bullet_model: Model,
    bullets: Vec<Bullet>,
}

struct Player {
    camera: Camera,
    rate_of_fire: f32,
    time_since_fired: f32,
}

struct Bullet {
    renderer_handle: Vec<RenderModelHandle>,
    position: Vector3<f32>,
    velocity: Vector3<f32>,
    lifetime: f32,
}

impl TurretGame {
    fn new(app: &mut Application, event_loop: &EventLoop<()>) -> Self {
        // Load bullet model
        let bullet_model = {
            let models = app
                .asset_manager
                .load_gltf(&mut app.renderer, "assets/models/Cube/glTF/Cube.gltf")
                .unwrap();
            models[0].clone()
        };

        let lights = vec![create_light(
            &mut app.renderer,
            Light {
                position: Point3::new(-10.0f32, -5.0f32, 16.0f32),
                intensity: 5.0,
                ..Default::default()
            },
        )];

        let audio_manager =
            AudioManager::<CpalBackend>::new(AudioManagerSettings::default()).unwrap();
        let background_music =
            StaticSoundData::from_file("assets/sounds/prelude.ogg", StaticSoundSettings::default())
                .unwrap();

        let egui = EguiContext::new(event_loop);
        let draw_ui = true;

        let player = Player {
            camera: Camera {
                position: (-8.0, 0.0, 0.0).into(),
                direction: (1.0, 0.0, 0.0).into(),
                aspect: app.window.inner_size().width as f32
                    / app.window.inner_size().height as f32,
                fovy: 90.0,
                znear: 0.1,
                zfar: 4000.0,
            },
            rate_of_fire: 2f32,
            time_since_fired: 100f32,
        };

        Self {
            egui,
            lights,
            player,
            audio_manager,
            background_music,
            draw_debug_ui: draw_ui,
            bullet_model,
            bullets: Vec::new(),
        }
    }

    fn update(&mut self, ctx: &mut Application) {
        if ctx.input.is_just_pressed(VirtualKeyCode::F1) {
            self.draw_debug_ui = !self.draw_debug_ui
        }
        self.handle_player_input(ctx);

        for bullet in self.bullets.iter_mut() {
            bullet.position += bullet.velocity * ctx.delta_time;
            bullet.lifetime -= ctx.delta_time;
        }

        // Remove any bullets that need deleting and remove render handles;
        let old_handles: Vec<RenderModelHandle> = self
            .bullets
            .iter()
            .flat_map(|bullet| bullet.renderer_handle.clone())
            .collect();
        self.bullets.retain(|bullet| bullet.lifetime >= 0.0f32);
        let new_handles: Vec<RenderModelHandle> = self
            .bullets
            .iter()
            .flat_map(|bullet| bullet.renderer_handle.clone())
            .collect();
        for handle in old_handles.into_iter() {
            if !new_handles.contains(&handle) {
                ctx.renderer.remove_render_model(handle);
            }
        }

        // Update render objects & then render
        self.update_renderer_object_states(&mut ctx.renderer);
        ctx.renderer.set_camera(&self.player.camera);
    }

    fn handle_player_input(&mut self, ctx: &mut Application) {
        let speed = 25.0f32;
        let movement = speed * ctx.delta_time;
        if ctx.input.is_held(VirtualKeyCode::A) {
            self.player.camera.position -= Vector3::new(0.0, 0.0, movement);
        }
        if ctx.input.is_held(VirtualKeyCode::D) {
            self.player.camera.position += Vector3::new(0.0, 0.0, movement);
        }

        self.player.time_since_fired += ctx.delta_time;
        if ctx.input.is_held(VirtualKeyCode::Space)
            && self.player.time_since_fired >= 1.0f32 / self.player.rate_of_fire
        {
            self.player.time_since_fired = 0.0f32;
            self.bullets.push(spawn_bullet(
                ctx,
                &self.bullet_model,
                self.player.camera.position.to_vec() + Vector3::new(0f32, -6f32, 8f32),
                Vector3::new(1f32, 0.2f32, 0f32),
                100f32,
            ));
        }
    }

    fn update_renderer_object_states(&self, renderer: &mut Renderer) {
        for component in self.lights.iter() {
            renderer
                .set_light(component.handle, &component.light)
                .unwrap();
        }
        for bullet in self.bullets.iter() {
            for &handle in bullet.renderer_handle.iter() {
                renderer
                    .set_render_model_transform(
                        handle,
                        from_transforms(
                            bullet.position,
                            Quaternion::from_angle_y(Deg(-90f32))
                                * Quaternion::look_at(
                                    bullet.velocity.normalize(),
                                    Vector3::unit_y(),
                                ),
                            Vector3::new(1f32, 0.2f32, 0.2f32),
                        ),
                    )
                    .unwrap();
            }
        }
    }

    fn draw_ui(&mut self, app: &mut Application) {
        if self.draw_debug_ui {
            self.egui.run(&app.window, |ctx| {
                egui::Window::new("Game Debug")
                    .vscroll(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Rate of Fire(per s)");
                            ui.add(
                                egui::Slider::new(&mut self.player.rate_of_fire, 1.0..=10.0)
                                    .step_by(0.1),
                            );
                        });
                    });
                egui::Window::new("Timings")
                    .vscroll(false)
                    .resizable(false)
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
                    .show(ctx, |ui| {
                        let timestamps = app.renderer.timestamps();
                        debug_ui::draw_timestamps(ui, timestamps);
                    });
            });
            self.egui.paint(&mut app.renderer);
        }
    }

    fn on_window_event(&mut self, event: &WindowEvent) -> EventResponse {
        self.egui.on_event(event)
    }
}

fn create_light(renderer: &mut Renderer, light: Light) -> LightComponent {
    LightComponent {
        handle: renderer.create_light(&light).unwrap(),
        light,
    }
}

#[profiling::function]
pub fn from_transforms(
    position: Vector3<f32>,
    rotation: Quaternion<f32>,
    size: Vector3<f32>,
) -> Matrix4<f32> {
    let translation = Matrix4::from_translation(position);
    let rotation = Matrix4::from(rotation);
    let scale = Matrix4::from_nonuniform_scale(size.x, size.y, size.z);

    translation * rotation * scale
}

pub fn run_game() {
    let (screen_width, screen_height) = (1920, 1080);
    let event_loop = EventLoop::new();

    let mut app = Application::new(screen_width, screen_height, &event_loop);
    app.renderer.draw_debug_ui = false;

    let mut project = TurretGame::new(&mut app, &event_loop);

    let mut initial_resize = true;

    let mut frame_timer = FrameTimer::new();

    profiling::scope!("Game Event Loop");
    {
        event_loop.run(move |event, _, control_flow| {
            match event {
                Event::MainEventsCleared => {
                    frame_timer.update();

                    while frame_timer.sub_frame_update() {
                        app.delta_time = frame_timer.delta_time();
                        app.time_passed = frame_timer.total_time_elapsed();

                        project.update(&mut app);
                    }

                    project.draw_ui(&mut app);

                    app.renderer.render().unwrap();
                }
                Event::NewEvents(_) => {
                    app.input.prev_keys.copy_from_slice(&app.input.now_keys);
                }
                Event::WindowEvent { ref event, .. } => {
                    let response = project.on_window_event(event);
                    if !response.consumed {
                        app.input.update_input_from_event(event);
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
                                app.renderer.resize(*physical_size).unwrap();
                            }
                        }
                        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                            app.renderer.resize(**new_inner_size).unwrap();
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

fn spawn_bullet(
    app: &mut Application,
    bullet_model: &Model,
    position: Vector3<f32>,
    direction: Vector3<f32>,
    speed: f32,
) -> Bullet {
    let handles = spawn_model(app, bullet_model);
    for &handle in handles.iter() {
        app.renderer
            .set_render_model_material(
                handle,
                MaterialInstance {
                    diffuse: Vector4::new(0.0f32, 0.6f32, 0.0f32, 1.0f32),
                    emissive: Vector3::new(1.0f32, 1.0f32, 1.0f32),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    Bullet {
        renderer_handle: handles,
        position,
        velocity: direction.normalize() * speed,
        lifetime: 10.0,
    }
}

fn spawn_model(ctx: &mut Application, model: &Model) -> Vec<RenderModelHandle> {
    let mut model_handles = Vec::new();
    for mesh in model.mesh.submeshes.iter() {
        let renderer_handle = ctx
            .renderer
            .add_render_model(mesh.mesh, mesh.material_instance);
        model_handles.push(renderer_handle);
    }
    model_handles
}
