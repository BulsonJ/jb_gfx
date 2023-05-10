use cgmath::{EuclideanSpace, InnerSpace, Matrix4, Point3, SquareMatrix, Vector3, Vector4, Zero};

use crate::light::Light;
use crate::{CameraTrait, DirectionalLight};

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PushConstants {
    pub handles: [i32; 4],
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TransformSSBO {
    pub model: [[f32; 4]; 4],
    pub normal: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct MaterialParamSSBO {
    pub diffuse: [f32; 4],
    pub emissive: [f32; 4],
    pub textures: [i32; 8],
}

/// The Camera Matrix that is given to the GPU.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CameraUniform {
    pub proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub inv_proj_view: [[f32; 4]; 4],
    pub position: [f32; 4],
    pub ambient_light: [f32; 4],
    pub directional_light_colour: [f32; 4],
    pub directional_light_direction: [f32; 4],
    pub directional_light_proj: [[f32; 4]; 4],
    pub directional_light_view: [[f32; 4]; 4],
    pub point_light_count: i32,
    pub padding: [i32; 3],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            proj: Matrix4::identity().into(),
            view: Matrix4::identity().into(),
            inv_proj_view: Matrix4::identity().into(),
            position: Vector4::zero().into(),
            ambient_light: Vector4::zero().into(),
            directional_light_colour: Vector4::zero().into(),
            directional_light_direction: Vector4::zero().into(),
            directional_light_proj: Matrix4::identity().into(),
            directional_light_view: Matrix4::identity().into(),
            point_light_count: 0,
            padding: [0, 0, 0],
        }
    }

    pub fn update_proj<T: CameraTrait>(&mut self, camera: &T) {
        let proj = camera.build_projection_matrix();
        let view = camera.build_view_matrix();

        self.proj = proj.into();
        self.view = view.into();
        self.inv_proj_view = (proj * view).invert().unwrap().into();
        self.position = camera.position().to_vec().extend(0f32).into();
    }

    pub fn update_light(&mut self, light: &DirectionalLight) {
        self.directional_light_proj = light.build_projection_matrix().into();
        self.directional_light_view = light.build_view_matrix().into();
        self.directional_light_colour = light.colour.extend(light.intensity).into();
        self.directional_light_direction = light.direction.normalize().extend(0f32).into();
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct LightUniform {
    pub pos: [f32; 4],
    pub colour: [f32; 4],
}

impl LightUniform {
    pub fn new(position: Point3<f32>, colour: Vector3<f32>, intensity: f32) -> Self {
        let position = position.to_vec().extend(0f32);
        let colour = colour.extend(intensity);

        Self {
            pos: position.into(),
            colour: colour.into(),
        }
    }
}

impl From<Light> for LightUniform {
    fn from(value: Light) -> Self {
        LightUniform::new(value.position, value.colour, value.intensity)
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UIUniformData {
    pub screen_size: [f32; 2],
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UIVertexData {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub colour: [f32; 4],
    pub texture_id: [i32; 4],
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WorldDebugUIDrawData {
    pub position: [f32; 3],
    pub texture_index: i32,
    pub colour: [f32; 3],
    pub size: f32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleDrawData {
    pub position: [f32; 3],
    pub texture_index: i32,
    pub colour: [f32; 3],
    pub size: f32,
}
