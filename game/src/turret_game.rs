use std::time::Duration;

use cgmath::{
    Array, Deg, EuclideanSpace, InnerSpace, Matrix4, Quaternion, Rotation, Rotation3, Vector3,
    Vector4, Zero,
};
use egui_winit::EventResponse;
use kira::manager::backend::cpal::CpalBackend;
use kira::manager::{AudioManager, AudioManagerSettings};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::tween::{Easing, Tween};
use kira::LoopBehavior;
use kira::Volume::Amplitude;
use rand::{thread_rng, Rng};
use winit::event::{VirtualKeyCode, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::Window;

use engine::prelude::*;
use jb_gfx::prelude::*;
use jb_gfx::renderer::{MaterialInstanceHandle, RenderModelHandle};

use crate::collision::CollisionBox;
use crate::components::LightComponent;
use crate::debug_ui::{draw_timestamps, DebugPanel};
use crate::egui_context::EguiContext;
use crate::input::Input;
use crate::turret_game::player::Player;
use crate::Camera;

pub mod player;

pub struct TurretGame {
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
    bullet_material: MaterialInstanceHandle,
    bullet_tracer_material: MaterialInstanceHandle,
    barrels: Vec<Barrel>,
    terrain_pieces: Vec<Terrain>,
    terrain_settings: TerrainSettings,
}

struct Bullet {
    renderer_handle: RenderModelHandle,
    position: Vector3<f32>,
    velocity: Vector3<f32>,
    scale: Vector3<f32>,
    lifetime: f32,
    collision_box: CollisionBox,
}

struct Barrel {
    renderer_handle: RenderModelHandle,
    position: Vector3<f32>,
    scale: Vector3<f32>,
    collision_box: CollisionBox,
}

struct Terrain {
    renderer_handle: RenderModelHandle,
    position: Vector3<f32>,
    scale: Vector3<f32>,
}

struct TerrainSettings {
    tile_width: u32,
    tile_height: u32,
    tile_size: f32,
}

impl TurretGame {
    pub fn new(window: Window, event_loop: &EventLoop<()>) -> Self {
        let input = Input::default();

        let mut renderer = Renderer::new(&window).unwrap();
        renderer.render().unwrap();
        let mut asset_manager = AssetManager::default();

        renderer
            .load_skybox(
                [
                    "assets/textures/skybox/clouds1_east.bmp",
                    "assets/textures/skybox/clouds1_west.bmp",
                    "assets/textures/skybox/clouds1_up.bmp",
                    "assets/textures/skybox/clouds1_down.bmp",
                    "assets/textures/skybox/clouds1_north.bmp",
                    "assets/textures/skybox/clouds1_south.bmp",
                ],
                &ImageFormatType::Default,
            )
            .unwrap();

        // Spawn plane
        {
            let plane_model = {
                let models = asset_manager
                    .load_gltf(&mut renderer, "assets/models/plane/plane.gltf")
                    .unwrap();
                models[0].clone()
            };
            let plane = spawn_model(&mut renderer, &plane_model)[0];
            renderer
                .set_render_model_transform(
                    &[plane],
                    from_transforms(
                        Vector3::new(0.0f32, -2.0f32, 0.0f32),
                        Quaternion::from_angle_y(Deg(0.0)),
                        Vector3::from_value(1f32),
                    ),
                )
                .unwrap();
        }

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
        let barrel_model = {
            let models = asset_manager
                .load_gltf(&mut renderer, "assets/models/barrel/barrel.gltf")
                .unwrap();
            models[0].clone()
        };
        // Spawn barrels
        let barrels = {
            let barrel_distance = 40.0f32;
            let barrel_height = 6;
            let barrel_width = 6;
            let spacing = 10.0f32;
            let offset = Vector3::new(
                0.0f32,
                -barrel_width as f32 * spacing,
                -barrel_height as f32 * spacing,
            ) / 2f32
                + Vector3::new(0.0f32, 5.0f32, 5.0f32);

            let mut barrels = Vec::new();
            for y in 0..barrel_height {
                for x in 0..barrel_width {
                    let position = offset
                        + Vector3::new(barrel_distance, spacing * y as f32, spacing * x as f32);
                    let scale = Vector3::from_value(5f32);

                    let barrel = Barrel {
                        renderer_handle: spawn_model(&mut renderer, &barrel_model)[0],
                        position,
                        scale,
                        collision_box: CollisionBox {
                            position,
                            size: Vector3::new(2f32, 4f32, 2f32),
                        },
                    };
                    renderer
                        .set_render_model_transform(
                            &[barrel.renderer_handle],
                            from_transforms(
                                barrel.position,
                                Quaternion::from_angle_y(Deg(0.0)),
                                barrel.scale,
                            ),
                        )
                        .unwrap();

                    barrels.push(barrel);
                }
            }
            barrels
        };

        let grass_material = renderer.add_material_instance(MaterialInstance {
            diffuse: Vector4::new(1.0f32, 1.0f32, 1.0f32, 1.0f32),
            diffuse_texture: Some(grass_texture),
            ..Default::default()
        });
        let bullet_material = renderer.add_material_instance(MaterialInstance {
            diffuse: Vector4::new(0.4f32, 0.4f32, 0.4f32, 1.0f32),
            ..Default::default()
        });
        let bullet_tracer_material = renderer.add_material_instance(MaterialInstance {
            diffuse: Vector4::new(0.0f32, 0.0f32, 0.0f32, 1.0f32),
            emissive: Vector3::new(2.0f32, 2.0f32, 0.0f32),
            ..Default::default()
        });

        let terrain_settings = TerrainSettings {
            tile_width: 5,
            tile_height: 9,
            tile_size: 100.0f32,
        };
        let terrain_pieces = {
            let mut terrain_pieces = Vec::new();
            let offset = Vector3::new(
                -(terrain_settings.tile_height as f32 - 1f32) * terrain_settings.tile_size,
                0.0f32,
                -(terrain_settings.tile_width as f32 - 1f32) * terrain_settings.tile_size,
            );
            for y in 0..terrain_settings.tile_height {
                for x in 0..terrain_settings.tile_width {
                    let position = Vector3::new(
                        y as f32 * (terrain_settings.tile_size * 2f32),
                        -100.0f32,
                        x as f32 * (terrain_settings.tile_size * 2f32),
                    ) + offset;
                    let scale =
                        Vector3::new(terrain_settings.tile_size, 1.0, terrain_settings.tile_size);
                    let terrain = Terrain {
                        renderer_handle: spawn_model(&mut renderer, &bullet_model)[0],
                        position,
                        scale,
                    };
                    renderer
                        .set_render_model_material(&[terrain.renderer_handle], grass_material)
                        .unwrap();
                    renderer
                        .set_render_model_transform(
                            &[terrain.renderer_handle],
                            from_transforms(
                                terrain.position,
                                Quaternion::from_angle_y(Deg(0.0)),
                                terrain.scale,
                            ),
                        )
                        .unwrap();
                    terrain_pieces.push(terrain);
                }
            }
            terrain_pieces
        };

        let lights = vec![];

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
            tracer_bullet_rate: 3i32,
            bullets_since_last_tracer: 0i32,
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
            bullet_material,
            bullet_tracer_material,
            barrels,
            terrain_pieces,
            terrain_settings,
        }
    }

    pub fn update(&mut self) {
        if self.input.is_just_pressed(VirtualKeyCode::F1) {
            self.draw_debug_ui = !self.draw_debug_ui
        }
        self.handle_player_input();

        let plane_movement_speed = 50.0f32;

        for terrain in self.terrain_pieces.iter_mut() {
            terrain.position +=
                Vector3::new(plane_movement_speed, 0.0f32, 0.0f32) * self.delta_time;
            let boundary =
                self.terrain_settings.tile_size * self.terrain_settings.tile_height as f32;
            if terrain.position.x >= boundary {
                terrain.position.x = -boundary;
            }
        }

        for bullet in self.bullets.iter_mut() {
            bullet.velocity.x += plane_movement_speed / 100.0f32;
            bullet.velocity.x = bullet.velocity.x.clamp(-800.0f32, 800.0f32);
            bullet.position += bullet.velocity * self.delta_time;
            bullet.collision_box.position = bullet.position;
            bullet.lifetime -= self.delta_time;
        }

        for barrel in self.barrels.iter_mut() {
            barrel.collision_box.position = barrel.position;
        }

        {
            profiling::scope!("Check Collisions");

            let mut destroy_barrels = Vec::new();
            let mut destroy_bullets = Vec::new();
            for (i, bullet) in self.bullets.iter().enumerate() {
                for (j, barrel) in self.barrels.iter().enumerate() {
                    if bullet.collision_box.check_collision(&barrel.collision_box) {
                        destroy_barrels.push(j);
                        destroy_bullets.push(i);
                    }
                }
            }
            for &i in destroy_barrels.iter() {
                let removed_barrel = self.barrels.remove(i);
                self.renderer
                    .remove_render_model(removed_barrel.renderer_handle);
            }
            for &i in destroy_bullets.iter() {
                let removed = self.bullets.remove(i);
                self.renderer.remove_render_model(removed.renderer_handle);
            }
        }

        {
            profiling::scope!("Remove old bullets");

            // Remove any bullets that need deleting and remove render handles;
            let old_handles: Vec<RenderModelHandle> = self
                .bullets
                .iter()
                .map(|bullet| bullet.renderer_handle)
                .collect();
            self.bullets.retain(|bullet| bullet.lifetime >= 0.0f32);
            let new_handles: Vec<RenderModelHandle> = self
                .bullets
                .iter()
                .map(|bullet| bullet.renderer_handle)
                .collect();
            for handle in old_handles.into_iter() {
                if !new_handles.contains(&handle) {
                    self.renderer.remove_render_model(handle);
                }
            }
        }

        // Update render objects & then render
        self.update_renderer_object_states();
        self.renderer.set_camera(&self.player.camera);
    }

    fn handle_player_input(&mut self) {
        self.player.update_camera(&self.input, self.delta_time);

        // TODO : Should move this into player? How to access Renderer, AudioManager etc in that case
        self.player.time_since_fired += self.delta_time;
        if self.input.is_just_pressed(VirtualKeyCode::Space) {
            self.firing_sound_handle =
                Some(self.audio_manager.play(self.fire_sound.clone()).unwrap());
        }
        if self.input.is_held(VirtualKeyCode::Space)
            && self.player.time_since_fired >= 1.0f32 / self.player.rate_of_fire
        {
            self.player.time_since_fired = 0.0f32;
            let tracer = {
                if self.player.bullets_since_last_tracer >= self.player.tracer_bullet_rate {
                    self.player.bullets_since_last_tracer = 0;
                    true
                } else {
                    false
                }
            };

            let spread = 0.05f32;
            let y_direction = thread_rng().gen_range(-spread..spread);
            let z_direction = thread_rng().gen_range(-spread..spread);
            let offset = Vector3::new(0.0f32, y_direction, z_direction);

            let bullet = self.spawn_bullet(
                self.player.camera.position.to_vec() + Vector3::new(0f32, -1f32, 0f32),
                self.player.camera.direction + offset,
                500f32,
                tracer,
            );
            self.bullets.push(bullet);
            self.player.bullets_since_last_tracer += 1;
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
        for terrain in self.terrain_pieces.iter() {
            self.renderer
                .set_render_model_transform(
                    &[terrain.renderer_handle],
                    from_transforms(
                        terrain.position,
                        Quaternion::from_angle_y(Deg(0f32)),
                        terrain.scale,
                    ),
                )
                .unwrap();
        }
        for bullet in self.bullets.iter() {
            self.renderer
                .set_render_model_transform(
                    &[bullet.renderer_handle],
                    from_transforms(
                        bullet.position,
                        Quaternion::from_angle_y(Deg(0f32)),
                        bullet.scale,
                    ),
                )
                .unwrap();
        }
        for barrel in self.barrels.iter() {
            self.renderer
                .set_render_model_transform(
                    &[barrel.renderer_handle],
                    from_transforms(
                        barrel.position,
                        Quaternion::from_angle_y(Deg(0f32)),
                        barrel.scale,
                    ),
                )
                .unwrap()
        }
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) -> EventResponse {
        self.egui.on_event(event)
    }

    fn spawn_bullet(
        &mut self,
        position: Vector3<f32>,
        direction: Vector3<f32>,
        speed: f32,
        tracer: bool,
    ) -> Bullet {
        let handles = spawn_model(&mut self.renderer, &self.bullet_model);
        let material = {
            if tracer {
                self.bullet_tracer_material
            } else {
                self.bullet_material
            }
        };
        self.renderer
            .set_render_model_material(&handles, material)
            .unwrap();

        let scale = {
            if tracer {
                Vector3::new(0.2f32, 0.2f32, 0.2f32)
            } else {
                Vector3::new(0.1f32, 0.1f32, 0.1f32)
            }
        };

        let collision_box = CollisionBox {
            position,
            size: Vector3::new(1f32, 1f32, 1f32),
        };

        Bullet {
            renderer_handle: handles[0],
            position,
            velocity: direction.normalize() * speed,
            scale,
            lifetime: 10.0,
            collision_box,
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

fn spawn_model(renderer: &mut Renderer, model: &Model) -> Vec<RenderModelHandle> {
    let mut model_handles = Vec::new();
    for mesh in model.mesh.submeshes.iter() {
        let renderer_handle = renderer.add_render_model(mesh.mesh, mesh.material_instance);
        model_handles.push(renderer_handle);
    }
    model_handles
}

impl TurretGame {
    pub fn draw_ui(&mut self) {
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
                        self.player.draw_debug(ui);
                    });
                egui::Window::new("Timings")
                    .vscroll(false)
                    .resizable(false)
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
                    .show(ctx, |ui| {
                        let timestamps = self.renderer.timestamps();
                        draw_timestamps(ui, timestamps);
                    });
            });
            self.egui.paint(&mut self.renderer);
        }
    }
}
