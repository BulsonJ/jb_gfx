use cgmath::Vector3;
use egui::Ui;
use winit::event::VirtualKeyCode;

use crate::debug_ui::DebugPanel;
use crate::input::Input;
use crate::Camera;

pub struct Player {
    pub(crate) camera: Camera,
    pub(crate) rate_of_fire: f32,
    pub(crate) time_since_fired: f32,
    pub(crate) tracer_bullet_rate: i32,
    pub(crate) bullets_since_last_tracer: i32,
}

impl Player {
    pub fn update_camera(&mut self, input: &Input, delta_time: f32) {
        let speed = 1.0f32;
        let movement = speed * delta_time;
        let pitch_speed = 1.0f32;
        let pitch_movement = pitch_speed * delta_time;
        if input.is_held(VirtualKeyCode::A) {
            self.camera.direction -= Vector3::new(0.0, 0.0, movement);
        }
        if input.is_held(VirtualKeyCode::D) {
            self.camera.direction += Vector3::new(0.0, 0.0, movement);
        }
        if input.is_held(VirtualKeyCode::W) {
            self.camera.direction += Vector3::new(0.0, pitch_movement, 0.0);
        }
        if input.is_held(VirtualKeyCode::S) {
            self.camera.direction -= Vector3::new(0.0, pitch_movement, 0.0);
        }
    }
}

impl DebugPanel for Player {
    fn draw_debug(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Rate of Fire(per s)");
            ui.add(egui::Slider::new(&mut self.rate_of_fire, 1.0..=20.0).step_by(0.1));
        });
        ui.horizontal(|ui| {
            ui.label("Tracer Rate of Fire");
            ui.add(egui::Slider::new(&mut self.tracer_bullet_rate, 1..=5));
        });
    }
}
