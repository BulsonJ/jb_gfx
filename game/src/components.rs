use jb_gfx::prelude::*;

use crate::Camera;

#[derive(Copy, Clone)]
pub struct LightComponent {
    pub handle: LightHandle,
    pub light: Light,
}

pub struct CameraComponent {
    pub camera: Camera,
}
