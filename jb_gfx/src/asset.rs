use std::collections::HashMap;

use gltf::image::Source;

use crate::renderer::{MaterialTextures, MeshHandle, Renderer, Texture};
use crate::{Mesh, Vertex};

#[derive(Default)]
pub struct AssetManager {
    loaded_textures: HashMap<String, Texture>,
}

impl AssetManager {
    pub fn load_model(
        &mut self,
        renderer: &mut Renderer,
        file: &str,
    ) -> anyhow::Result<Vec<Model>> {
        let mut models = Vec::new();

        let (gltf, buffers, _) = gltf::import(file)?;

        let (source_folder, _asset_name) = file.rsplit_once('/').unwrap();

        for image in gltf.images() {
            let location = image.source();
            match location {
                Source::View { .. } => {}
                Source::Uri {
                    uri,
                    mime_type: _mime_type,
                } => {
                    let image_asset = String::from(source_folder) + "/" + uri;
                    if let Ok(loaded_texture) = renderer.load_texture(&image_asset) {
                        self.loaded_textures.insert(uri.to_string(), loaded_texture);
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
                let diffuse_tex = {
                    if let Some(info) = material.pbr_metallic_roughness().base_color_texture() {
                        match info.texture().source().source() {
                            Source::View { .. } => None,
                            Source::Uri { uri, .. } => {
                                Some(*self.loaded_textures.get(uri).unwrap())
                            }
                        }
                    } else {
                        None
                    }
                };
                let normal_tex = {
                    if let Some(info) = material.normal_texture() {
                        match info.texture().source().source() {
                            Source::View { .. } => None,
                            Source::Uri { uri, .. } => {
                                Some(*self.loaded_textures.get(uri).unwrap())
                            }
                        }
                    } else {
                        None
                    }
                };
                let metallic_roughness_tex = {
                    if let Some(info) = material
                        .pbr_metallic_roughness()
                        .metallic_roughness_texture()
                    {
                        match info.texture().source().source() {
                            Source::View { .. } => None,
                            Source::Uri { uri, .. } => {
                                Some(*self.loaded_textures.get(uri).unwrap())
                            }
                        }
                    } else {
                        None
                    }
                };
                let emissive_tex = {
                    if let Some(emissive) = material.emissive_texture() {
                        match emissive.texture().source().source() {
                            Source::View { .. } => None,
                            Source::Uri { uri, .. } => {
                                Some(*self.loaded_textures.get(uri).unwrap())
                            }
                        }
                    } else {
                        None
                    }
                };

                let mut vertices = Vec::new();
                for i in 0..positions.len() {
                    let position = *positions.get(i).unwrap();
                    let tex_coords = *tex_coords.get(i).unwrap();
                    let normal = *normals.get(i).unwrap();
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

                let mesh_handle = renderer.load_mesh(&mesh)?;
                let model = Model {
                    mesh: mesh_handle,
                    textures: MaterialTextures {
                        diffuse: diffuse_tex,
                        emissive: emissive_tex,
                        normal: normal_tex,
                        metallic_roughness: metallic_roughness_tex,
                        ..Default::default()
                    },
                };

                models.push(model);
            }
        }

        Ok(models)
    }
}

pub struct Model {
    pub mesh: MeshHandle,
    pub textures: MaterialTextures,
}
