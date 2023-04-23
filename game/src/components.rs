use crate::camera::DirectionCamera;
use crate::Camera;
use jb_gfx::renderer::LightHandle;
use jb_gfx::Light;

#[derive(Copy, Clone)]
pub struct LightComponent {
    pub handle: LightHandle,
    pub light: Light,
}

pub struct CameraComponent {
    pub camera: Camera,
}
