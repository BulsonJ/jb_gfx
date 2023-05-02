use crate::Camera;
use engine::prelude::*;
use jb_gfx::prelude::*;

#[derive(Copy, Clone)]
pub struct LightComponent {
    pub handle: LightHandle,
    pub light: Light,
}

pub struct CameraComponent {
    pub camera: Camera,
}
