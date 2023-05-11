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
    pub fn new(window_size: (f32, f32)) -> Self {
        Self {
            camera: Camera {
                position: (0.0, 0.0, 0.0).into(),
                rotation: (0.0, 90.0, 0.0).into(),
                aspect: window_size.0 / window_size.1,
                fovy: 90.0,
                znear: 0.1,
                zfar: 4000.0,
            },
            rate_of_fire: 8f32,
            time_since_fired: 100f32,
            tracer_bullet_rate: 3i32,
            bullets_since_last_tracer: 0i32,
        }
    }
    pub fn update_camera(&mut self, input: &Input, delta_time: f32) {
        let speed = 50.0f32;
        let movement = speed * delta_time;
        let pitch_speed = 50.0f32;
        let pitch_movement = pitch_speed * delta_time;
        if input.is_held(VirtualKeyCode::A) {
            self.camera.rotation.y -= movement;
        }
        if input.is_held(VirtualKeyCode::D) {
            self.camera.rotation.y += movement;
        }
        if input.is_held(VirtualKeyCode::W) {
            self.camera.rotation.x -= pitch_movement;
        }
        if input.is_held(VirtualKeyCode::S) {
            self.camera.rotation.x += pitch_movement;
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
