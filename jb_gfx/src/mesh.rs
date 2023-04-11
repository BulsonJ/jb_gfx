use std::path::Path;

use obj::Obj;

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub tangent: [f32; 4],
}

pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Option<Vec<u32>>,
}
