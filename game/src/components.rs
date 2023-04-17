use jb_gfx::renderer::{CameraHandle, LightHandle};
use jb_gfx::{Camera, Light};

#[derive(Copy, Clone)]
pub struct LightComponent {
    pub(crate) handle: LightHandle,
    pub light: Light,
}

#[derive(Copy, Clone)]
pub struct CameraComponent {
    pub(crate) handle: CameraHandle,
    pub camera: Camera,
}
