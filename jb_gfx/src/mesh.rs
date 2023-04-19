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
    pub faces: Vec<Face>,
}

pub type Face = [u32; 3];

fn vertex(mesh: &MeshData, face: usize, vert: usize) -> &Vertex {
    let vs: &[u32; 3] = &mesh.faces[face];
    &mesh.vertices[vs[vert] as usize]
}

impl mikktspace::Geometry for MeshData {
    fn num_faces(&self) -> usize {
        self.faces.len()
    }

    fn num_vertices_of_face(&self, _face: usize) -> usize {
        3
    }

    fn position(&self, face: usize, vert: usize) -> [f32; 3] {
        vertex(self, face, vert).position.into()
    }

    fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
        vertex(self, face, vert).normal.into()
    }

    fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
        vertex(self, face, vert).tex_coords.into()
    }

    fn set_tangent_encoded(&mut self, tangent: [f32; 4], face: usize, vert: usize) {
        let face = self.faces[face];
        let vert_index = face[vert];
        let vert = self.vertices.get_mut(vert_index as usize).unwrap();
        vert.tangent = tangent;
    }
}
