use std::ops::RangeInclusive;

use cgmath::Vector3;
use egui::panel::TopBottomSide;
use egui::{Context, Ui};
use winit::event::VirtualKeyCode;

use jb_gfx::renderer::Renderer;

use crate::components::{CameraComponent, LightComponent};
use crate::input::Input;

pub struct Editor {
    camera_controls_show: bool,
    light_controls_show: bool,
    engine_utils_show: bool,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            camera_controls_show: false,
            light_controls_show: false,
            engine_utils_show: false,
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

        egui::TopBottomPanel::new(TopBottomSide::Top, "Test").show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.top_bar(ui);
            });
            egui::Window::new("Camera Controls")
                .vscroll(false)
                .resizable(false)
                .open(&mut self.camera_controls_show)
                .show(ctx, |ui| {
                    Editor::camera_panel(ui, dependencies);
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
    }

    pub fn camera_panel(ui: &mut Ui, dependencies: &mut EditorDependencies) {
        if ui.button("Camera One").clicked() {
            if let Some(camera) = dependencies.cameras.get(0) {
                dependencies.renderer.active_camera = Some(camera.handle);
            }
        }
        if ui.button("Camera Two").clicked() {
            if let Some(camera) = dependencies.cameras.get(1) {
                dependencies.renderer.active_camera = Some(camera.handle);
            }
        }
    }

    pub fn light_panel(ui: &mut Ui, dependencies: &mut EditorDependencies) {
        ui.horizontal(|ui| {
            ui.label("Point Lights");
            ui.color_edit_button_rgb(dependencies.lights[0].light.colour.as_mut());
            ui.color_edit_button_rgb(dependencies.lights[1].light.colour.as_mut());
            ui.color_edit_button_rgb(dependencies.lights[2].light.colour.as_mut());
            ui.color_edit_button_rgb(dependencies.lights[3].light.colour.as_mut());
        });

        let mut direction = dependencies.renderer.sun.direction;
        ui.horizontal(|ui| {
            ui.label("Sun");
            ui.color_edit_button_rgb(dependencies.renderer.sun.colour.as_mut());
            ui.add(
                egui::DragValue::new(&mut direction.x)
                    .clamp_range(RangeInclusive::new(-1, 1))
                    .speed(0.005),
            );
            ui.add(
                egui::DragValue::new(&mut direction.y)
                    .clamp_range(RangeInclusive::new(-1, 1))
                    .speed(0.005),
            );
            ui.add(
                egui::DragValue::new(&mut direction.z)
                    .clamp_range(RangeInclusive::new(-1, 1))
                    .speed(0.005),
            );
        });
        dependencies.renderer.sun.direction = direction;

        let mut colour: Vector3<f32> = dependencies.renderer.clear_colour.into();
        ui.horizontal(|ui| {
            ui.label("Sky");
            ui.color_edit_button_rgb(colour.as_mut());
        });
        dependencies.renderer.clear_colour = colour.into();
    }

    fn engine_utils_panel(ui: &mut Ui, dependencies: &mut EditorDependencies) {
        if ui.button("Reload Shaders").clicked() {
            dependencies.renderer.reload_shaders().unwrap();
        }
    }
}

pub struct EditorDependencies<'a> {
    pub input: &'a Input,
    pub renderer: &'a mut Renderer,
    pub cameras: &'a [CameraComponent],
    pub lights: &'a mut [LightComponent],
}
