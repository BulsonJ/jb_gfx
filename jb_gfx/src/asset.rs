use crate::renderer::{MaterialTextures, MeshHandle, Renderer, Texture};
use crate::{Mesh, Vertex};
use gltf::buffer::Data;
use gltf::image::Source;
use gltf::{Gltf, Semantic};
use std::collections::HashMap;

pub struct AssetManager {}

impl AssetManager {
    pub fn load_model(renderer: &mut Renderer, file: &str) -> Vec<Model> {
        let mut models = Vec::new();

        let (gltf, buffers, _) = gltf::import(file).unwrap();
        let mut textures = HashMap::new();

        let (source_folder, asset_name) = file.rsplit_once('/').unwrap();

        for image in gltf.images() {
            let location = image.source();
            match location {
                Source::View { .. } => {}
                Source::Uri { uri, mime_type } => {
                    let image_asset = String::from(source_folder) + "/" + uri;
                    if let Ok(loaded_texture) = renderer.load_texture(&image_asset) {
                        textures.insert(uri, loaded_texture);
                    }
                }
            };
        }

        for mesh in gltf.meshes() {
            for primitive in mesh.primitives() {
                let mut positions = Vec::new();
                let mut tex_coords = Vec::new();
                let mut normals = Vec::new();
                let mut colors = Vec::new();
                let mut possible_indices = Vec::new();

                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
                if let Some(iter) = reader.read_positions() {
                    for vertex_position in iter {
                        positions.push(vertex_position);
                    }
                }
                if let Some(iter) = reader.read_tex_coords(0u32) {
                    for tex_coord in iter.into_f32() {
                        tex_coords.push(tex_coord);
                    }
                }
                if let Some(iter) = reader.read_normals() {
                    for normal in iter {
                        normals.push(normal);
                    }
                }
                if let Some(iter) = reader.read_colors(0u32) {
                    for color in iter.into_rgb_f32() {
                        colors.push(color);
                    }
                }
                if let Some(iter) = reader.read_indices() {
                    for index in iter.into_u32() {
                        possible_indices.push(index);
                    }
                }

                let material = primitive.material();
                let diffuse = material
                    .pbr_metallic_roughness()
                    .base_color_texture()
                    .unwrap();
                let diffuse_tex = match diffuse.texture().source().source() {
                    Source::View { .. } => None,
                    Source::Uri { uri, .. } => Some(textures.get(uri).unwrap().clone()),
                };

                let mut vertices = Vec::new();
                for i in 0..positions.len() {
                    let position = positions.get(i).unwrap().clone();
                    let tex_coords = tex_coords.get(i).unwrap().clone();
                    let normal = normals.get(i).unwrap().clone();
                    //let color = colors.get(i).unwrap().clone();

                    let vertex = Vertex {
                        position,
                        tex_coords,
                        normal,
                        color: [1f32, 1f32, 1f32],
                    };
                    vertices.push(vertex);
                }

                let indices = {
                    if possible_indices.is_empty() {
                        None
                    } else {
                        Some(possible_indices)
                    }
                };

                let mesh = Mesh { vertices, indices };

                let mesh_handle = renderer.load_mesh(&mesh).unwrap();
                let model = Model {
                    mesh: mesh_handle,
                    diffuse_texture: diffuse_tex,
                };

                models.push(model);
            }
        }

        models
    }
}

pub struct Model {
    pub mesh: MeshHandle,
    pub diffuse_texture: Option<Texture>,
}
