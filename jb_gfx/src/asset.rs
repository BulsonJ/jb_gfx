use anyhow::anyhow;
use std::collections::HashMap;

use gltf::image::Source;

use crate::device::ImageFormatType;
use crate::renderer::{MaterialInstance, MeshHandle, Renderer, Texture};
use crate::{Face, MeshData, Vertex};

#[derive(Default)]
pub struct AssetManager {
    loaded_textures: HashMap<String, Texture>,
}

impl AssetManager {
    pub fn load_texture(
        &mut self,
        renderer: &mut Renderer,
        file: &str,
        format: &ImageFormatType,
    ) -> anyhow::Result<Texture> {
        if let Some(texture) = self.loaded_textures.get(file) {
            Ok(*texture)
        } else if let Ok(loaded_texture) = renderer.load_texture(file, format) {
            self.loaded_textures
                .insert(file.to_string(), loaded_texture);
            Ok(loaded_texture)
        } else {
            Err(anyhow!("Cant load texture or find it!"))
        }
    }

    pub fn load_gltf(&mut self, renderer: &mut Renderer, file: &str) -> anyhow::Result<Vec<Model>> {
        let mut models = Vec::new();

        let (gltf, buffers, _) = gltf::import(file)?;

        let (source_folder, _asset_name) = file.rsplit_once('/').unwrap();

        // TODO : Add image load to vec when iterating through materials, then for normal maps upload them as normal
        for image in gltf.images() {
            let location = image.source();
            match location {
                Source::View { .. } => {}
                Source::Uri {
                    uri,
                    mime_type: _mime_type,
                } => {}
            };
        }

        for mesh in gltf.meshes() {
            for primitive in mesh.primitives() {
                let mut positions = Vec::new();
                let mut tex_coords = Vec::new();
                let mut normals = Vec::new();
                let mut colors = Vec::new();
                let mut tangents = Vec::new();
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
                if let Some(iter) = reader.read_tangents() {
                    for tangent in iter {
                        tangents.push(tangent);
                    }
                }

                let material = primitive.material();
                let diffuse_tex = {
                    if let Some(info) = material.pbr_metallic_roughness().base_color_texture() {
                        match info.texture().source().source() {
                            Source::View { .. } => None,
                            Source::Uri { uri, .. } => {
                                let image_asset = String::from(source_folder) + "/" + uri;
                                Some(self.load_texture(
                                    renderer,
                                    &image_asset,
                                    &ImageFormatType::Default,
                                )?)
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
                                let image_asset = String::from(source_folder) + "/" + uri;
                                Some(self.load_texture(
                                    renderer,
                                    &image_asset,
                                    &ImageFormatType::Normal,
                                )?)
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
                                let image_asset = String::from(source_folder) + "/" + uri;
                                Some(self.load_texture(
                                    renderer,
                                    &image_asset,
                                    &ImageFormatType::Default,
                                )?)
                            }
                        }
                    } else {
                        None
                    }
                };
                let occlusion_tex = {
                    if let Some(emissive) = material.occlusion_texture() {
                        match emissive.texture().source().source() {
                            Source::View { .. } => None,
                            Source::Uri { uri, .. } => {
                                let image_asset = String::from(source_folder) + "/" + uri;
                                let format_type = ImageFormatType::Default;
                                Some(self.load_texture(
                                    renderer,
                                    &image_asset,
                                    &ImageFormatType::Default,
                                )?)
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
                                let image_asset = String::from(source_folder) + "/" + uri;
                                let format_type = ImageFormatType::Default;
                                Some(self.load_texture(
                                    renderer,
                                    &image_asset,
                                    &ImageFormatType::Default,
                                )?)
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
                    let tangent = {
                        if let Some(tang) = tangents.get(i) {
                            *tang
                        } else {
                            [0f32, 0f32, 0f32, 0f32]
                        }
                    };
                    let color = {
                        if let Some(colour) = colors.get(i) {
                            *colour
                        } else {
                            [1f32, 1f32, 1f32]
                        }
                    };
                    //let color = colors.get(i).unwrap().clone();

                    let vertex = Vertex {
                        position,
                        tex_coords,
                        normal,
                        color,
                        tangent,
                    };
                    vertices.push(vertex);
                }

                let faces = {
                    let mut faces = Vec::new();
                    for i in 0..possible_indices.len() / 3 {
                        let index = i * 3;
                        faces.push([
                            possible_indices[index],
                            possible_indices[index + 1],
                            possible_indices[index + 2],
                        ]);
                    }
                    faces
                };

                let indices = {
                    if possible_indices.is_empty() {
                        None
                    } else {
                        Some(possible_indices)
                    }
                };

                let mut mesh = MeshData {
                    vertices,
                    indices,
                    faces,
                };
                if tangents.is_empty() {
                    let ret = mikktspace::generate_tangents(&mut mesh);
                }

                let mesh_handle = renderer.load_mesh(&mesh)?;
                let model = Model {
                    mesh: mesh_handle,
                    material_instance: MaterialInstance {
                        diffuse_texture: diffuse_tex,
                        emissive_texture: emissive_tex,
                        normal_texture: normal_tex,
                        metallic_roughness_texture: metallic_roughness_tex,
                        occlusion_texture: occlusion_tex,
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
    pub material_instance: MaterialInstance,
}
