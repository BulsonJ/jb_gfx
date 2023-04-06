use cgmath::{Matrix4, SquareMatrix, Vector3, Vector4, Zero};
use crate::Camera;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PushConstants {
    pub model: [[f32; 4]; 4],
    pub normal: [[f32; 4]; 4],
    pub textures: [i32; 8],
}

/// The Camera Matrix that is given to the GPU.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CameraUniform {
    pub proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub position: [f32; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            proj: Matrix4::identity().into(),
            view: Matrix4::identity().into(),
            position: Vector4::zero().into(),
        }
    }

    pub fn update_proj(&mut self, camera: &Camera) {
        self.proj = camera.build_projection_matrix().into();
        self.view = camera.build_view_matrix().into();
        self.position = camera.position.extend(0f32).into();
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LightUniform {
    pub pos: [f32; 4],
    pub colour: [f32; 4],
}

impl LightUniform {
    pub fn new(position: Vector3<f32>, colour: Vector3<f32>) -> Self {
        let position = position.extend(0f32);
        let colour = colour.extend(0f32);

        Self {
            pos: position.into(),
            colour: colour.into(),
        }
    }
}