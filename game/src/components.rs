use jb_gfx::renderer::{CameraHandle, LightHandle};
use jb_gfx::{Camera, Light};

#[derive(Copy, Clone)]
pub struct LightComponent {
    pub handle: LightHandle,
    pub light: Light,
}

#[derive(Copy, Clone)]
pub struct CameraComponent {
    pub handle: CameraHandle,
    pub camera: Camera,
}
