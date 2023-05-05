use std::collections::HashMap;
use std::error::Error;

use anyhow::{anyhow, Result};
use cgmath::Matrix4;
use gltf::image::Source;
use image::EncodableLayout;
use log::info;

use jb_gfx::prelude::*;
use jb_gfx::renderer::{MaterialInstanceHandle, RenderModelHandle};

#[derive(Default)]
pub struct AssetManager {
    loaded_textures: HashMap<String, ImageHandle>,
}

impl AssetManager {
    pub fn load_texture(
        &mut self,
        renderer: &mut Renderer,
        file: impl AsRef<std::path::Path>,
        format: &ImageFormatType,
    ) -> Result<ImageHandle> {
        if let Some(texture) = self.loaded_textures.get(file.as_ref().to_str().unwrap()) {
            Ok(*texture)
        } else if let Ok(loaded_texture) =
            renderer.load_texture(file.as_ref().to_str().unwrap(), format)
        {
            self.loaded_textures
                .insert(file.as_ref().to_str().unwrap().to_string(), loaded_texture);
            Ok(loaded_texture)
        } else {
            Err(anyhow!("Cant load texture or find it!"))
        }
    }

    fn load_embedded_texture(
        &mut self,
        renderer: &mut Renderer,
        buffers: &[gltf::buffer::Data],
        image: &gltf::Image,
        view: &gltf::buffer::View,
    ) -> Result<ImageHandle> {
        if let Some(texture) = self.loaded_textures.get(image.name().unwrap()) {
            Ok(*texture)
        } else {
            let data = &buffers[view.buffer().index()];
            let offset = view.offset();
            let length = view.length();
            let end = offset + length;
            let image_slice = &data[offset..end];
            let img = image::load_from_memory(image_slice).unwrap();

            let rgba_img = img.to_rgba8();
            let img_bytes = rgba_img.as_bytes();
            let mip_levels = (img.width().max(img.height()) as f32).log2().floor() as u32 + 1u32;

            if let Ok(loaded_texture) = renderer.load_texture_from_bytes(
                img_bytes,
                img.width(),
                img.height(),
                &ImageFormatType::Default,
                mip_levels,
                1,
            ) {
                self.loaded_textures
                    .insert(image.name().unwrap().to_string(), loaded_texture);
                Ok(loaded_texture)
            } else {
                Err(anyhow!("Cant load texture or find it!"))
            }
        }
    }

    pub fn load_gltf(
        &mut self,
        renderer: &mut Renderer,
        file: impl AsRef<std::path::Path>,
    ) -> Result<Vec<Model>> {
        profiling::scope!("Load GLTF Asset");
        let file = file.as_ref().to_str().unwrap();

        let (gltf, buffers, _) = {
            profiling::scope!("Load GLTF Asset: Import File");
            gltf::import(file)?
        };

        let (source_folder, asset_name) = file.rsplit_once('/').unwrap();

        // TODO : Add image load to vec when iterating through materials, then for normal maps upload them as normal
        for image in gltf.images() {
            let location = image.source();
            match location {
                Source::View { .. } => {}
                Source::Uri {
                    uri: _uri,
                    mime_type: _mime_type,
                } => {}
            };
        }

        let mut meshes = HashMap::new();
        for mesh in gltf.meshes() {
            let mut submeshes = Vec::new();
            profiling::scope!("Load GLTF Asset: Mesh");
            for primitive in mesh.primitives() {
                profiling::scope!("Load GLTF Asset: Primitive");

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
                            Source::View { mime_type: _, view } => {
                                Some(self.load_embedded_texture(
                                    renderer,
                                    &buffers,
                                    &info.texture().source(),
                                    &view,
                                )?)
                            }
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
                            Source::View { mime_type: _, view } => {
                                Some(self.load_embedded_texture(
                                    renderer,
                                    &buffers,
                                    &info.texture().source(),
                                    &view,
                                )?)
                            }
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
                            Source::View { mime_type: _, view } => {
                                Some(self.load_embedded_texture(
                                    renderer,
                                    &buffers,
                                    &info.texture().source(),
                                    &view,
                                )?)
                            }
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
                    if let Some(occlusion) = material.occlusion_texture() {
                        match occlusion.texture().source().source() {
                            Source::View { mime_type: _, view } => {
                                Some(self.load_embedded_texture(
                                    renderer,
                                    &buffers,
                                    &occlusion.texture().source(),
                                    &view,
                                )?)
                            }
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
                let emissive_tex = {
                    if let Some(emissive) = material.emissive_texture() {
                        match emissive.texture().source().source() {
                            Source::View { mime_type: _, view } => {
                                Some(self.load_embedded_texture(
                                    renderer,
                                    &buffers,
                                    &emissive.texture().source(),
                                    &view,
                                )?)
                            }
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

                let mut mesh_data = MeshData {
                    vertices,
                    indices,
                    faces,
                };
                if tangents.is_empty() {
                    let _ret = mesh_data.generate_tangents();
                }

                let mesh_handle = renderer.load_mesh(&mesh_data)?;
                let material_instance = MaterialInstance {
                    diffuse: material.pbr_metallic_roughness().base_color_factor().into(),
                    diffuse_texture: diffuse_tex,
                    emissive: material.emissive_factor().into(),
                    emissive_texture: emissive_tex,
                    normal_texture: normal_tex,
                    metallic_roughness_texture: metallic_roughness_tex,
                    occlusion_texture: occlusion_tex,
                };
                let material_instance = renderer.add_material_instance(material_instance);

                let model = SubMesh {
                    mesh: mesh_handle,
                    material_instance,
                };

                submeshes.push(model);
            }
            meshes.insert(mesh.index(), Mesh { submeshes });
        }

        let mut models = HashMap::new();
        for node in gltf.nodes() {
            if let Some(mesh) = node.mesh() {
                let mesh_index = mesh.index();
                if let Some(model) = meshes.get(&mesh_index) {
                    let transform = Matrix4::from(node.transform().matrix());

                    models.insert(
                        node.index(),
                        Model {
                            mesh: model.clone(),
                            transform,
                        },
                    );
                }
            }
        }

        for node in gltf.nodes() {
            if let Some(parent) = models.get(&node.index()).cloned() {
                for child in node.children() {
                    let model = models.get_mut(&child.index()).unwrap();
                    model.transform = parent.transform * model.transform;
                }
            }
        }

        let models: Vec<Model> = models.values().cloned().collect();
        let meshes_amount: usize = meshes.values().map(|mesh| mesh.submeshes.len()).sum();
        info!(
            "Loaded GLTF Model. Name: [{}], Models: [{}], Mesh/Submeshes:[{}]",
            asset_name,
            models.len(),
            meshes_amount,
        );

        Ok(models)
    }
}

#[derive(Clone)]
pub struct Model {
    pub mesh: Mesh,
    pub transform: Matrix4<f32>,
}

#[derive(Clone)]
pub struct Mesh {
    pub submeshes: Vec<SubMesh>,
}

#[derive(Copy, Clone)]
pub struct SubMesh {
    pub mesh: MeshHandle,
    pub material_instance: MaterialInstanceHandle,
}
