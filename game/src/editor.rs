use std::fmt::Debug;
use std::ops::RangeInclusive;

use cgmath::Vector3;
use egui::panel::TopBottomSide;
use egui::{Context, Ui};
use kira::manager::AudioManager;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::tween::Tween;
use winit::event::VirtualKeyCode;

use jb_gfx::renderer::Renderer;

use crate::components::{CameraComponent, LightComponent};
use crate::input::Input;

#[derive(Default)]
pub struct Editor {
    camera_controls_show: bool,
    light_controls_show: bool,
    engine_utils_show: bool,
    audio_show: bool,
    camera_panel: CameraPanel,
    audio_panel: AudioPanel,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn handle_input(dependencies: &mut EditorDependencies) {
        if dependencies.input.is_just_pressed(VirtualKeyCode::F5) {
            dependencies.renderer.reload_shaders().unwrap();
        }
        if dependencies.input.is_just_pressed(VirtualKeyCode::Key1) {
            if let Some(camera) = dependencies.cameras.get(0) {
                dependencies.renderer.active_camera = Some(camera.handle);
            }
        }
        if dependencies.input.is_just_pressed(VirtualKeyCode::Key2) {
            if let Some(camera) = dependencies.cameras.get(1) {
                dependencies.renderer.active_camera = Some(camera.handle);
            }
        }
    }

    pub fn run(&mut self, ctx: &Context, dependencies: &mut EditorDependencies) {
        Editor::handle_input(dependencies);
        self.audio_panel.update();

        egui::TopBottomPanel::new(TopBottomSide::Top, "Test").show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.top_bar(ui);
            });

            egui::Window::new("Camera Controls")
                .vscroll(false)
                .resizable(false)
                .open(&mut self.camera_controls_show)
                .show(ctx, |ui| {
                    self.camera_panel.draw(ui, dependencies);
                });
            egui::Window::new("Light Controls")
                .vscroll(false)
                .resizable(false)
                .open(&mut self.light_controls_show)
                .show(ctx, |ui| {
                    Editor::light_panel(ui, dependencies);
                });
            egui::Window::new("Engine Utils")
                .vscroll(false)
                .resizable(false)
                .open(&mut self.engine_utils_show)
                .show(ctx, |ui| {
                    Editor::engine_utils_panel(ui, dependencies);
                });
            egui::Window::new("Audio")
                .vscroll(false)
                .resizable(false)
                .open(&mut self.audio_show)
                .show(ctx, |ui| {
                    self.audio_panel.draw(ui, dependencies);
                });
        });
    }

    pub fn top_bar(&mut self, ui: &mut Ui) {
        if ui.button("Camera").clicked() {
            self.camera_controls_show = !self.camera_controls_show;
        }
        if ui.button("Lights").clicked() {
            self.light_controls_show = !self.light_controls_show;
        }
        if ui.button("Utils").clicked() {
            self.engine_utils_show = !self.engine_utils_show;
        }
        if ui.button("Audio").clicked() {
            self.audio_show = !self.audio_show;
        }
    }

    pub fn light_panel(ui: &mut Ui, dependencies: &mut EditorDependencies) {
        ui.vertical(|ui| {
            ui.label("Point Lights");
            for (i, light) in dependencies.lights.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(String::from("Light ") + &i.to_string() + ":");
                    ui.color_edit_button_rgb(light.light.colour.as_mut());
                });
            }
        });
        ui.separator();

        ui.label("Sky");
        ui.horizontal(|ui| {
            let mut colour: Vector3<f32> = dependencies.renderer.clear_colour.into();
            ui.horizontal(|ui| {
                ui.label("Colour");
                ui.color_edit_button_rgb(colour.as_mut());
            });
            dependencies.renderer.clear_colour = colour.into();
        });
        ui.separator();

        ui.label("Sun");
        ui.horizontal(|ui| {
            ui.label("Colour");
            ui.color_edit_button_rgb(dependencies.renderer.sun.colour.as_mut());
        });
        ui.horizontal(|ui| {
            ui.label("Direction");
            ui.add(
                egui::DragValue::new(&mut dependencies.renderer.sun.direction.x)
                    .clamp_range(RangeInclusive::new(-1, 1))
                    .speed(0.005),
            );
            ui.add(
                egui::DragValue::new(&mut dependencies.renderer.sun.direction.y)
                    .clamp_range(RangeInclusive::new(-1, 1))
                    .speed(0.005),
            );
            ui.add(
                egui::DragValue::new(&mut dependencies.renderer.sun.direction.z)
                    .clamp_range(RangeInclusive::new(-1, 1))
                    .speed(0.005),
            );
        });
        ui.separator();
    }

    fn engine_utils_panel(ui: &mut Ui, dependencies: &mut EditorDependencies) {
        let timestamps = dependencies.renderer.timestamps();

        ui.horizontal(|ui| {
            ui.label("Shadow Pass:");
            ui.label(format!("{:.6}", timestamps.shadow_pass.to_string()));
        });
        ui.horizontal(|ui| {
            ui.label("Forward Pass:");
            ui.label(format!("{:.6}", timestamps.forward_pass.to_string()));
        });
        ui.horizontal(|ui| {
            ui.label("UI Pass:");
            ui.label(format!("{:.6}", timestamps.ui_pass.to_string()));
        });
        ui.horizontal(|ui| {
            ui.label("Frametime:");
            let total = timestamps.shadow_pass + timestamps.forward_pass + timestamps.ui_pass;
            ui.label(format!("{:.6}", total));
        });

        if ui.button("Reload Shaders").clicked() {
            dependencies.renderer.reload_shaders().unwrap();
        }
    }
}

pub struct EditorDependencies<'a> {
    pub input: &'a Input,
    pub renderer: &'a mut Renderer,
    pub audio_manager: &'a mut AudioManager,
    pub background_music: &'a mut StaticSoundData,
    pub cameras: &'a mut [CameraComponent],
    pub lights: &'a mut [LightComponent],
}

#[derive(Default)]
struct CameraPanel {
    selected_camera_index: usize,
}

impl CameraPanel {
    fn draw(&mut self, ui: &mut Ui, dependencies: &mut EditorDependencies) {
        ui.label("Camera Selection");
        egui::ComboBox::from_label("Take your pick")
            .selected_text(format!("{:?}", self.selected_camera_index))
            .show_ui(ui, |ui| {
                ui.style_mut().wrap = Some(false);
                ui.set_min_width(60.0);
                for i in 0..dependencies.cameras.len() {
                    ui.selectable_value(&mut self.selected_camera_index, i, i.to_string());
                }
            });

        ui.separator();
        ui.label("Controls");
        if let Some(camera) = dependencies.cameras.get_mut(self.selected_camera_index) {
            dependencies.renderer.active_camera = Some(camera.handle);
            ui.horizontal(|ui| {
                ui.label("Position: ");
                ui.add(egui::DragValue::new(&mut camera.camera.position.x).speed(0.1));
                ui.add(egui::DragValue::new(&mut camera.camera.position.y).speed(0.1));
                ui.add(egui::DragValue::new(&mut camera.camera.position.z).speed(0.1));
            });
            ui.horizontal(|ui| {
                ui.label("Direction: ");
                ui.add(
                    egui::DragValue::new(&mut camera.camera.direction.x)
                        .speed(0.01)
                        .clamp_range(RangeInclusive::new(-1, 1)),
                );
                ui.add(
                    egui::DragValue::new(&mut camera.camera.direction.y)
                        .speed(0.01)
                        .clamp_range(RangeInclusive::new(-1, 1)),
                );
                ui.add(
                    egui::DragValue::new(&mut camera.camera.direction.z)
                        .speed(0.01)
                        .clamp_range(RangeInclusive::new(-1, 1)),
                );
            });
            ui.horizontal(|ui| {
                ui.label("FOV: ");
                ui.add(
                    egui::DragValue::new(&mut camera.camera.fovy)
                        .clamp_range(RangeInclusive::new(45, 120)),
                );
            });
        }
    }
}

pub struct AudioPanel {
    music_handle: Option<StaticSoundHandle>,
    volume: f32,
}

impl AudioPanel {
    pub fn new(dependencies: &mut EditorDependencies) -> Self {
        Self {
            music_handle: Some(
                dependencies
                    .audio_manager
                    .play(dependencies.background_music.clone())
                    .unwrap(),
            ),
            ..Default::default()
        }
    }
}

impl AudioPanel {
    fn update(&mut self) {
        if let Some(handle) = &mut self.music_handle {
            handle.set_volume(self.volume as f64, Tween::default());
        }
    }

    fn draw(&mut self, ui: &mut Ui, dependencies: &mut EditorDependencies) {
        ui.horizontal(|ui| {
            if self.music_handle.is_none() {
                if ui.button("Start").clicked() {
                    self.music_handle = Some(
                        dependencies
                            .audio_manager
                            .play(dependencies.background_music.clone())
                            .unwrap(),
                    );
                }
                ui.add(egui::Slider::new(&mut self.volume, 0f32..=1f32));
            } else {
                if ui.button("Stop").clicked() {
                    self.music_handle.as_mut().unwrap().stop(Tween::default());
                    self.music_handle = None;
                }

                ui.add(egui::Slider::new(&mut self.volume, 0f32..=1f32));
            }
        });
    }
}

impl Default for AudioPanel {
    fn default() -> Self {
        Self {
            volume: 0.2f32,
            music_handle: None,
        }
    }
}
