use obj::Obj;
use std::path::Path;

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Option<Vec<u32>>,
}

impl Mesh {
    pub fn from_file(file: &str) -> Mesh {
        let path = Path::new(file);
        let object = Obj::load(path).unwrap();

        let mut vertices = Vec::<Vertex>::new();

        let mut indexed_vertices = Vec::<Vertex>::new();
        let mut indices = Vec::<u32>::new();
        for obj in object.data.objects.iter() {
            for group in obj.groups.iter() {
                for poly in group.polys.iter() {
                    for tuple in poly.0.iter() {
                        let position = object.data.position[tuple.0];
                        let tex_coords = match tuple.1 {
                            Some(tex_coord_index) => object.data.texture[tex_coord_index],
                            None => [0.0f32, 0.0f32],
                        };
                        let normal = match tuple.2 {
                            Some(normal_index) => object.data.normal[normal_index],
                            None => [0.0f32, 0.0f32, 0.0f32],
                        };
                        let color = [1.0f32, 1.0f32, 1.0f32];

                        let vertex = Vertex {
                            position,
                            tex_coords,
                            normal,
                            color,
                        };

                        vertices.push(vertex);

                        if !indexed_vertices.contains(&vertex) {
                            indexed_vertices.push(vertex);
                        }
                        let vertex_index =
                            indexed_vertices.iter().position(|&x| x == vertex).unwrap() as u32;
                        indices.push(vertex_index);
                    }
                }
            }
        }

        if indices.len() == indexed_vertices.len() {
            Mesh {
                vertices,
                indices: None,
            }
        } else {
            Mesh {
                vertices: indexed_vertices,
                indices: Some(indices),
            }
        }
    }
}
