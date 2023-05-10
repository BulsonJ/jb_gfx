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
    pub indices: Option<Vec<Index>>,
    pub faces: Vec<Face>,
}

impl MeshData {
    pub fn quad() -> MeshData {
        let vertices_simple: [([f32; 3], [f32; 2]); 6] = [
            ([0.0f32, 0.0f32, 0.0f32], [0.0f32, 0.0f32]),
            ([0.0f32, 1.0f32, 0.0f32], [0.0f32, 1.0f32]),
            ([1.0f32, 1.0f32, 0.0f32], [1.0f32, 1.0f32]),
            ([0.0f32, 0.0f32, 0.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, 1.0f32, 0.0f32], [1.0f32, 1.0f32]),
            ([1.0f32, 0.0f32, 0.0f32], [1.0f32, 0.0f32]),
        ];
        let vertices = vertices_simple
            .iter()
            .map(|(position, tex_coords)| Vertex {
                position: position.clone(),
                tex_coords: tex_coords.clone(),
                normal: [0.0, 0.0, 0.0],
                color: [0.0, 0.0, 0.0],
                tangent: [0.0, 0.0, 0.0, 0.0],
            })
            .collect();

        let indices = {
            let mut indices = Vec::new();
            for i in 0..6 {
                let vert_index = (i * 4) as Index;
                indices.push(vert_index + 0);
                indices.push(vert_index + 1);
                indices.push(vert_index + 2);
                indices.push(vert_index + 1);
                indices.push(vert_index + 3);
                indices.push(vert_index + 2);
            }
            indices
        };
        MeshData {
            vertices,
            indices: Some(indices),
            faces: vec![],
        }
    }

    pub fn cube() -> MeshData {
        let vertices_simple: [([f32; 3], [f32; 2]); 24] = [
            ([-1.0f32, 1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, 1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, -1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, -1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, 1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, 1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, -1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, -1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, -1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, -1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, -1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, -1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, 1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, 1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, 1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, 1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, -1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, 1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, -1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([-1.0f32, 1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, -1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, 1.0f32, 1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, -1.0f32, -1.0f32], [0.0f32, 0.0f32]),
            ([1.0f32, 1.0f32, -1.0f32], [0.0f32, 0.0f32]),
        ];
        let vertices = vertices_simple
            .iter()
            .map(|(position, tex_coords)| Vertex {
                position: position.clone(),
                tex_coords: tex_coords.clone(),
                normal: [0.0, 0.0, 0.0],
                color: [0.0, 0.0, 0.0],
                tangent: [0.0, 0.0, 0.0, 0.0],
            })
            .collect();

        let indices = {
            let mut indices = Vec::new();
            for i in 0..6 {
                let vert_index = (i * 4) as Index;
                indices.push(vert_index + 0);
                indices.push(vert_index + 1);
                indices.push(vert_index + 2);
                indices.push(vert_index + 1);
                indices.push(vert_index + 3);
                indices.push(vert_index + 2);
            }
            indices
        };
        MeshData {
            vertices,
            indices: Some(indices),
            faces: vec![],
        }
    }
}

impl MeshData {
    pub fn generate_tangents(&mut self) -> bool {
        mikktspace::generate_tangents(self)
    }
}

pub type Face = [u32; 3];
pub type Index = u32;

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
        vertex(self, face, vert).position
    }

    fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
        vertex(self, face, vert).normal
    }

    fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
        vertex(self, face, vert).tex_coords
    }

    fn set_tangent_encoded(&mut self, tangent: [f32; 4], face: usize, vert: usize) {
        let face = self.faces[face];
        let vert_index = face[vert];
        let vert = self.vertices.get_mut(vert_index as usize).unwrap();
        vert.tangent = tangent;
    }
}
