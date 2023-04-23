use crate::camera::DirectionCamera;
use jb_gfx::renderer::LightHandle;
use jb_gfx::Light;

#[derive(Copy, Clone)]
pub struct LightComponent {
    pub handle: LightHandle,
    pub light: Light,
}

#[derive(Copy, Clone)]
pub struct CameraComponent {
    pub camera: DirectionCamera,
}
