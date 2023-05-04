use cgmath::{
    Array, Deg, EuclideanSpace, InnerSpace, Matrix4, Point3, Quaternion, Rotation, Rotation3,
    Vector3, Vector4, Zero,
};
use egui_winit::EventResponse;
use env_logger::{Builder, Target};
use kira::manager::backend::cpal::CpalBackend;
use kira::manager::{AudioManager, AudioManagerSettings};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::tween::{Easing, Tween};
use kira::Volume::Amplitude;
use kira::{LoopBehavior, Volume};
use log::info;
use rand::{thread_rng, Rng};
use std::time::Duration;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use engine::prelude::*;
use game::components::{CameraComponent, LightComponent};
use game::egui_context::EguiContext;
use game::input::Input;
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
    pub window: Window,
    pub input: Input,
    pub renderer: Renderer,
    pub asset_manager: AssetManager,
    pub delta_time: f32,
    pub time_passed: f32,

    lights: Vec<LightComponent>,
    player: Player,
    egui: EguiContext,
    audio_manager: AudioManager,
    fire_sound: StaticSoundData,
    firing_sound_handle: Option<StaticSoundHandle>,
    draw_debug_ui: bool,
    bullet_model: Model,
    bullets: Vec<Bullet>,
    engine_sound: StaticSoundData,
    engine_looping_sound: Option<StaticSoundHandle>,
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
    fn new(window: Window, event_loop: &EventLoop<()>) -> Self {
        let input = Input::default();

        let mut renderer = Renderer::new(&window).unwrap();
        renderer.render().unwrap();
        let mut asset_manager = AssetManager::default();

        let grass_texture = asset_manager
            .load_texture(
                &mut renderer,
                "assets/textures/grass.jpg",
                &ImageFormatType::Default,
            )
            .unwrap();
        // Load bullet model
        let bullet_model = {
            let models = asset_manager
                .load_gltf(&mut renderer, "assets/models/Cube/glTF/Cube.gltf")
                .unwrap();
            models[0].clone()
        };
        let tile_height = 9;
        let tile_width = 12;
        let size = 100.0f32;
        for y in 0..tile_height {
            for x in 0..tile_width {
                let handles = spawn_model(&mut renderer, &bullet_model);
                for &handle in handles.iter() {
                    renderer
                        .set_render_model_material(
                            handle,
                            MaterialInstance {
                                diffuse: Vector4::new(1.0f32, 1.0f32, 1.0f32, 1.0f32),
                                diffuse_texture: Some(grass_texture),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                    renderer
                        .set_render_model_transform(
                            handle,
                            from_transforms(
                                Vector3::new(
                                    -(((tile_height - 1) / 2) as f32 * size) + (y as f32 * size),
                                    -100.0f32,
                                    -(((tile_width - 1) / 2) as f32 * size) + (x as f32 * size),
                                ),
                                Quaternion::from_angle_y(Deg(0.0)),
                                Vector3::new(size, 1.0, size),
                            ),
                        )
                        .unwrap();
                }
            }
        }

        let lights = vec![create_light(
            &mut renderer,
            Light {
                position: Point3::new(-10.0f32, -5.0f32, 16.0f32),
                intensity: 5.0,
                ..Default::default()
            },
        )];

        let mut audio_manager =
            AudioManager::<CpalBackend>::new(AudioManagerSettings::default()).unwrap();
        let fire_sound = StaticSoundData::from_file(
            "assets/sounds/firing_loop.mp3",
            StaticSoundSettings::default()
                .loop_behavior(LoopBehavior {
                    start_position: 0.0,
                })
                .volume(Amplitude(0.1)),
        )
        .unwrap();
        let engine_sound_amplitude = 0.01;
        let engine_sound = StaticSoundData::from_file(
            "assets/sounds/prop-plane-flying.wav",
            StaticSoundSettings::default()
                .volume(Amplitude(engine_sound_amplitude))
                .loop_behavior(LoopBehavior {
                    start_position: 0.0,
                }),
        )
        .unwrap();
        let engine_looping_sound = audio_manager.play(engine_sound.clone()).unwrap();

        let egui = EguiContext::new(event_loop);
        let draw_ui = true;

        let player = Player {
            camera: Camera {
                position: (0.0, 0.0, 0.0).into(),
                direction: (1.0, 0.0, 0.0).into(),
                aspect: window.inner_size().width as f32 / window.inner_size().height as f32,
                fovy: 90.0,
                znear: 0.1,
                zfar: 4000.0,
            },
            rate_of_fire: 8f32,
            time_since_fired: 100f32,
        };

        Self {
            window,
            input,
            renderer,
            asset_manager,
            delta_time: 0.0,
            time_passed: 0.0,
            egui,
            lights,
            player,
            audio_manager,
            fire_sound,
            firing_sound_handle: None,
            engine_sound,
            engine_looping_sound: Some(engine_looping_sound),
            draw_debug_ui: draw_ui,
            bullet_model,
            bullets: Vec::new(),
        }
    }

    fn update(&mut self) {
        if self.input.is_just_pressed(VirtualKeyCode::F1) {
            self.draw_debug_ui = !self.draw_debug_ui
        }
        self.handle_player_input();

        for bullet in self.bullets.iter_mut() {
            bullet.position += bullet.velocity * self.delta_time;
            bullet.lifetime -= self.delta_time;
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
                self.renderer.remove_render_model(handle);
            }
        }

        // Update render objects & then render
        self.update_renderer_object_states();
        self.renderer.set_camera(&self.player.camera);
    }

    fn handle_player_input(&mut self) {
        let speed = 25.0f32;
        let movement = speed * self.delta_time;
        if self.input.is_held(VirtualKeyCode::A) {
            self.player.camera.position -= Vector3::new(0.0, 0.0, movement);
        }
        if self.input.is_held(VirtualKeyCode::D) {
            self.player.camera.position += Vector3::new(0.0, 0.0, movement);
        }

        self.player.time_since_fired += self.delta_time;
        if self.input.is_just_pressed(VirtualKeyCode::Space) {
            self.firing_sound_handle =
                Some(self.audio_manager.play(self.fire_sound.clone()).unwrap());
        }
        if self.input.is_held(VirtualKeyCode::Space)
            && self.player.time_since_fired >= 1.0f32 / self.player.rate_of_fire
        {
            self.player.time_since_fired = 0.0f32;
            let bullet = self.spawn_bullet(
                self.player.camera.position.to_vec() + Vector3::new(0f32, -6f32, 8f32),
                Vector3::new(1f32, 0.0f32, 0f32),
                100f32,
            );
            self.bullets.push(bullet);
        }
        if self.input.was_released(VirtualKeyCode::Space) {
            if let Some(sound) = self.firing_sound_handle.as_mut() {
                sound
                    .stop(Tween {
                        start_time: Default::default(),
                        duration: Duration::from_secs_f32(0.2f32),
                        easing: Easing::InPowi(1),
                    })
                    .unwrap();
            }
        }
    }

    fn update_renderer_object_states(&mut self) {
        for component in self.lights.iter() {
            self.renderer
                .set_light(component.handle, &component.light)
                .unwrap();
        }
        for bullet in self.bullets.iter() {
            for &handle in bullet.renderer_handle.iter() {
                self.renderer
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

    fn draw_ui(&mut self) {
        if self.draw_debug_ui {
            self.egui.run(&self.window, |ctx| {
                egui::Window::new("Game Debug")
                    .vscroll(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        if self.engine_looping_sound.is_none() {
                            if ui.button("Play Engine Sound").clicked() {
                                self.engine_looping_sound = Some(
                                    self.audio_manager.play(self.engine_sound.clone()).unwrap(),
                                );
                            }
                        } else if ui.button("Stop Engine Sound").clicked() {
                            let sound = self.engine_looping_sound.take();
                            sound.unwrap().stop(Tween::default()).unwrap();
                        }
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
                        let timestamps = self.renderer.timestamps();
                        debug_ui::draw_timestamps(ui, timestamps);
                    });
            });
            self.egui.paint(&mut self.renderer);
        }
    }

    fn on_window_event(&mut self, event: &WindowEvent) -> EventResponse {
        self.egui.on_event(event)
    }

    fn spawn_bullet(
        &mut self,
        position: Vector3<f32>,
        direction: Vector3<f32>,
        speed: f32,
    ) -> Bullet {
        let handles = spawn_model(&mut self.renderer, &self.bullet_model);
        for &handle in handles.iter() {
            self.renderer
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

    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(screen_width, screen_height))
        .with_title("Rust Renderer")
        .build(&event_loop)
        .unwrap();

    let mut project = TurretGame::new(window, &event_loop);

    let mut initial_resize = true;

    let mut frame_timer = FrameTimer::new();

    profiling::scope!("Game Event Loop");
    {
        event_loop.run(move |event, _, control_flow| {
            match event {
                Event::MainEventsCleared => {
                    frame_timer.update();

                    while frame_timer.sub_frame_update() {
                        project.delta_time = frame_timer.delta_time();
                        project.time_passed = frame_timer.total_time_elapsed();

                        project.update();
                    }

                    project.draw_ui();

                    project.renderer.render().unwrap();
                }
                Event::NewEvents(_) => {
                    project
                        .input
                        .prev_keys
                        .copy_from_slice(&project.input.now_keys);
                }
                Event::WindowEvent { ref event, .. } => {
                    let response = project.on_window_event(event);
                    if !response.consumed {
                        project.input.update_input_from_event(event);
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
                                project.renderer.resize(*physical_size).unwrap();
                            }
                        }
                        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                            project.renderer.resize(**new_inner_size).unwrap();
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

fn spawn_model(renderer: &mut Renderer, model: &Model) -> Vec<RenderModelHandle> {
    let mut model_handles = Vec::new();
    for mesh in model.mesh.submeshes.iter() {
        let renderer_handle = renderer.add_render_model(mesh.mesh, mesh.material_instance);
        model_handles.push(renderer_handle);
    }
    model_handles
}
