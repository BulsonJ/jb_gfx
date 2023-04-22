use std::mem::size_of;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use ash::vk;
use ash::vk::{
    AccessFlags2, ClearDepthStencilValue, DeviceSize, Handle, ImageAspectFlags, ImageLayout,
    IndexType, ObjectType, PipelineStageFlags2,
};
use bytemuck::{offset_of, Zeroable};
use cgmath::{
    Array, Deg, EuclideanSpace, Matrix, Matrix4, Quaternion, Rotation3, SquareMatrix, Vector3,
    Vector4, Zero,
};
use image::EncodableLayout;
use log::{error, info, trace, warn};
use slotmap::{new_key_type, SlotMap};
use winit::{dpi::PhysicalSize, window::Window};

use crate::barrier::{ImageBarrier, ImageBarrierBuilder, ImageHandleType};
use crate::descriptor::{DescriptorAllocator, DescriptorBuilder, DescriptorLayoutCache};
use crate::device::{
    cmd_copy_buffer, GraphicsDevice, ImageFormatType, FRAMES_IN_FLIGHT, SHADOWMAP_SIZE,
};
use crate::gpu_structs::{
    CameraUniform, LightUniform, MaterialParamSSBO, PushConstants, TransformSSBO, UIUniformData,
    UIVertexData,
};
use crate::pipeline::{
    PipelineColorAttachment, PipelineCreateInfo, PipelineHandle, PipelineManager,
    VertexInputDescription,
};
use crate::renderpass::{AttachmentHandle, AttachmentInfo, RenderPassBuilder};
use crate::resource::{BufferCreateInfo, BufferHandle, BufferStorageType, ImageHandle};
use crate::targets::{RenderImageType, RenderTargetHandle, RenderTargetSize, RenderTargets};
use crate::{Camera, Colour, DirectionalLight, Light, MeshData, Vertex};

const MAX_OBJECTS: u64 = 1000u64;
const MAX_QUADS: u64 = 100000u64;

/// The renderer for the GameEngine.
/// Used to draw objects using the GPU.
pub struct Renderer {
    device: Arc<GraphicsDevice>,
    pso_layout: vk::PipelineLayout,
    pso: PipelineHandle,
    camera_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    camera_uniform: CameraUniform,
    default_camera: Camera,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    pub clear_colour: Colour,
    pipeline_manager: PipelineManager,
    meshes: SlotMap<MeshHandle, RenderMesh>,
    render_models: SlotMap<RenderModelHandle, RenderModel>,
    light_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    transform_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    material_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    pub light_mesh: Option<MeshHandle>,
    stored_lights: SlotMap<LightHandle, Light>,
    stored_cameras: SlotMap<CameraHandle, Camera>,
    pub active_camera: Option<CameraHandle>,
    shadow_pso: PipelineHandle,
    pub sun: DirectionalLight,
    ui_pso_layout: vk::PipelineLayout,
    ui_pso: PipelineHandle,
    ui_descriptor_set_layout: vk::DescriptorSetLayout,
    ui_descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    quad_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    index_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    ui_uniform_data: [BufferHandle; FRAMES_IN_FLIGHT],
    ui_to_draw: Vec<UIMesh>,
    render_targets: RenderTargets,
    render_image: RenderTargetHandle,
    depth_image: RenderTargetHandle,
    directional_light_shadow_image: RenderTargetHandle,
    descriptor_layout_cache: DescriptorLayoutCache,
    descriptor_allocator: DescriptorAllocator,
}

impl Renderer {
    pub fn new(window: &Window) -> Result<Self> {
        profiling::scope!("Renderer::new");

        let device = Arc::new(GraphicsDevice::new(window)?);
        let mut render_targets = RenderTargets::new(device.clone());
        let mut pipeline_manager = PipelineManager::new(device.clone());

        let mut descriptor_layout_cache = DescriptorLayoutCache::new(device.vk_device.clone());
        let mut descriptor_allocator = DescriptorAllocator::new(device.vk_device.clone());

        let render_image_format = vk::Format::R8G8B8A8_SRGB;
        let depth_image_format = vk::Format::D32_SFLOAT;
        let resource_manager = device.resource_manager.clone();
        let render_image = render_targets.create_render_target(
            render_image_format,
            RenderTargetSize::Fullscreen,
            RenderImageType::Colour,
        )?;
        let depth_image = render_targets.create_render_target(
            depth_image_format,
            RenderTargetSize::Fullscreen,
            RenderImageType::Depth,
        )?;
        let directional_light_shadow_image = render_targets.create_render_target(
            depth_image_format,
            RenderTargetSize::Static(SHADOWMAP_SIZE, SHADOWMAP_SIZE),
            RenderImageType::Depth,
        )?;
        device.bindless_manager.borrow_mut().add_image_to_bindless(
            &render_targets
                .get_render_target(directional_light_shadow_image)
                .unwrap()
                .image(),
        );

        let camera = Camera {
            position: (-8.0, 100.0, 0.0).into(),
            direction: (1.0, 0.0, 0.0).into(),
            aspect: device.size().width as f32 / device.size().height as f32,
            fovy: 90.0,
            znear: 0.1,
            zfar: 4000.0,
        };

        let sun = DirectionalLight::new((0.0, -1.0, -0.1).into(), (1.0, 1.0, 1.0).into(), 400f32);

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_proj(&camera);
        camera_uniform.update_light(&sun);
        camera_uniform.ambient_light = Vector4::new(1.0, 1.0, 1.0, 0.0).into();

        let camera_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: size_of::<CameraUniform>(),
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                storage_type: BufferStorageType::HostLocal,
            };

            [
                device.resource_manager.create_buffer(&buffer_create_info),
                device.resource_manager.create_buffer(&buffer_create_info),
            ]
        };

        let transform_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: size_of::<TransformSSBO>() * MAX_OBJECTS as usize,
                usage: vk::BufferUsageFlags::STORAGE_BUFFER,
                storage_type: BufferStorageType::HostLocal,
            };

            [
                device.resource_manager.create_buffer(&buffer_create_info),
                device.resource_manager.create_buffer(&buffer_create_info),
            ]
        };

        let material_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: size_of::<MaterialParamSSBO>() * MAX_OBJECTS as usize,
                usage: vk::BufferUsageFlags::STORAGE_BUFFER,
                storage_type: BufferStorageType::HostLocal,
            };

            [
                device.resource_manager.create_buffer(&buffer_create_info),
                device.resource_manager.create_buffer(&buffer_create_info),
            ]
        };

        let light_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: size_of::<LightUniform>() * 4usize,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                storage_type: BufferStorageType::HostLocal,
            };

            [
                device.resource_manager.create_buffer(&buffer_create_info),
                device.resource_manager.create_buffer(&buffer_create_info),
            ]
        };

        let (descriptor_set, descriptor_set_layout) = {
            let mut sets = [vk::DescriptorSet::null(); FRAMES_IN_FLIGHT];
            let mut layout = None;
            for i in 0..FRAMES_IN_FLIGHT {
                let (set, set_layout) =
                    DescriptorBuilder::new(&mut descriptor_layout_cache, &mut descriptor_allocator)
                        .bind_buffer(
                            0,
                            &[device
                                .resource_manager
                                .get_buffer(camera_buffer[i])
                                .unwrap()
                                .buffer_write()],
                            vk::DescriptorType::UNIFORM_BUFFER,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        )
                        .bind_buffer(
                            1,
                            &[device
                                .resource_manager
                                .get_buffer(light_buffer[i])
                                .unwrap()
                                .buffer_write()],
                            vk::DescriptorType::UNIFORM_BUFFER,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        )
                        .bind_buffer(
                            2,
                            &[device
                                .resource_manager
                                .get_buffer(transform_buffer[i])
                                .unwrap()
                                .buffer_write()],
                            vk::DescriptorType::STORAGE_BUFFER,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        )
                        .bind_buffer(
                            3,
                            &[device
                                .resource_manager
                                .get_buffer(material_buffer[i])
                                .unwrap()
                                .buffer_write()],
                            vk::DescriptorType::STORAGE_BUFFER,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        )
                        .build()
                        .unwrap();

                sets[i] = set;
                layout = Some(set_layout);
            }
            (sets, layout.unwrap())
        };

        for set in descriptor_set.iter() {
            device.set_vulkan_debug_name(
                set.as_raw(),
                ObjectType::DESCRIPTOR_SET,
                "Global Descriptor Set(1)",
            )?;
        }

        for (i, set) in descriptor_set.iter().enumerate() {
            let camera_buffer = camera_buffer.get(i).unwrap();
            let light_buffer = light_buffer.get(i).unwrap();
            let transform_buffer = transform_buffer.get(i).unwrap();
            let material_buffer = material_buffer.get(i).unwrap();

            device
                .resource_manager
                .get_buffer(*camera_buffer)
                .unwrap()
                .view()
                .mapped_slice()?
                .copy_from_slice(&[camera_uniform]);

            let uniforms: Vec<LightUniform> = vec![
                LightUniform::zeroed(),
                LightUniform::zeroed(),
                LightUniform::zeroed(),
                LightUniform::zeroed(),
            ];

            device
                .resource_manager
                .get_buffer(*light_buffer)
                .unwrap()
                .view()
                .mapped_slice()?
                .copy_from_slice(&uniforms);
        }

        let push_constant_range = *vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .size(size_of::<PushConstants>() as u32)
            .offset(0u32);

        let pso_layout = pipeline_manager.create_pipeline_layout(
            &[
                *device.bindless_descriptor_set_layout(),
                descriptor_set_layout,
            ],
            &[push_constant_range],
        )?;

        let pso = {
            let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
                .depth_bounds_test_enable(false)
                .stencil_test_enable(false)
                .min_depth_bounds(0.0f32)
                .max_depth_bounds(1.0f32);

            let pso_build_info = PipelineCreateInfo {
                pipeline_layout: pso_layout,
                vertex_shader: "assets/shaders/default.vert".to_string(),
                fragment_shader: "assets/shaders/default.frag".to_string(),
                vertex_input_state: Vertex::get_vertex_input_desc(),
                color_attachment_formats: vec![PipelineColorAttachment {
                    format: render_image_format,
                    blend: false,
                    ..Default::default()
                }],
                depth_attachment_format: Some(depth_image_format),
                depth_stencil_state: *depth_stencil_state,
                cull_mode: vk::CullModeFlags::BACK,
            };

            pipeline_manager.create_pipeline(&pso_build_info)?
        };

        let shadow_pso = {
            let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
                .depth_bounds_test_enable(false)
                .stencil_test_enable(false)
                .min_depth_bounds(0.0f32)
                .max_depth_bounds(1.0f32);

            let pso_build_info = PipelineCreateInfo {
                pipeline_layout: pso_layout,
                vertex_shader: "assets/shaders/shadow.vert".to_string(),
                fragment_shader: "assets/shaders/shadow.frag".to_string(),
                vertex_input_state: Vertex::get_vertex_input_desc(),
                color_attachment_formats: vec![],
                depth_attachment_format: Some(depth_image_format),
                depth_stencil_state: *depth_stencil_state,
                cull_mode: vk::CullModeFlags::FRONT,
            };

            pipeline_manager.create_pipeline(&pso_build_info)?
        };

        let quad_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: size_of::<UIVertexData>() * MAX_QUADS as usize,
                usage: vk::BufferUsageFlags::STORAGE_BUFFER,
                storage_type: BufferStorageType::HostLocal,
            };

            [
                device.resource_manager.create_buffer(&buffer_create_info),
                device.resource_manager.create_buffer(&buffer_create_info),
            ]
        };

        let index_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: size_of::<u32>() * MAX_QUADS as usize * 3,
                usage: vk::BufferUsageFlags::INDEX_BUFFER,
                storage_type: BufferStorageType::HostLocal,
            };

            [
                device.resource_manager.create_buffer(&buffer_create_info),
                device.resource_manager.create_buffer(&buffer_create_info),
            ]
        };

        let ui_uniform_data = {
            let buffer_create_info = BufferCreateInfo {
                size: size_of::<UIUniformData>(),
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                storage_type: BufferStorageType::HostLocal,
            };

            [
                device.resource_manager.create_buffer(&buffer_create_info),
                device.resource_manager.create_buffer(&buffer_create_info),
            ]
        };

        let (ui_descriptor_set, ui_descriptor_set_layout) = {
            let mut sets = [vk::DescriptorSet::null(); FRAMES_IN_FLIGHT];
            let mut layout = None;
            for i in 0..FRAMES_IN_FLIGHT {
                let (set, set_layout) =
                    DescriptorBuilder::new(&mut descriptor_layout_cache, &mut descriptor_allocator)
                        .bind_buffer(
                            0,
                            &[device
                                .resource_manager
                                .get_buffer(ui_uniform_data[i])
                                .unwrap()
                                .buffer_write()],
                            vk::DescriptorType::UNIFORM_BUFFER,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        )
                        .bind_buffer(
                            1,
                            &[device
                                .resource_manager
                                .get_buffer(quad_buffer[i])
                                .unwrap()
                                .buffer_write()],
                            vk::DescriptorType::STORAGE_BUFFER,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        )
                        .build()
                        .unwrap();

                sets[i] = set;
                layout = Some(set_layout);
            }
            (sets, layout.unwrap())
        };

        let (ui_pso, ui_pso_layout) = {
            let pso_layout = pipeline_manager.create_pipeline_layout(
                &[
                    *device.bindless_descriptor_set_layout(),
                    ui_descriptor_set_layout,
                ],
                &[],
            )?;

            let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(false)
                .depth_write_enable(false)
                .depth_compare_op(vk::CompareOp::ALWAYS)
                .depth_bounds_test_enable(false)
                .stencil_test_enable(false)
                .min_depth_bounds(0.0f32)
                .max_depth_bounds(1.0f32);

            let pso_build_info = PipelineCreateInfo {
                pipeline_layout: pso_layout,
                vertex_shader: "assets/shaders/ui.vert".to_string(),
                fragment_shader: "assets/shaders/ui.frag".to_string(),
                vertex_input_state: Vertex::get_ui_vertex_input_desc(),
                color_attachment_formats: vec![PipelineColorAttachment {
                    format: render_image_format,
                    blend: true,
                    src_blend_factor_color: vk::BlendFactor::ONE,
                    dst_blend_factor_color: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                }],
                depth_attachment_format: None,
                depth_stencil_state: *depth_stencil_state,
                cull_mode: vk::CullModeFlags::NONE,
            };

            let pso = pipeline_manager.create_pipeline(&pso_build_info)?;
            (pso, pso_layout)
        };

        info!("Renderer Created");
        Ok(Self {
            device,
            pso_layout,
            pso,
            camera_buffer,
            camera_uniform,
            default_camera: camera,
            descriptor_set_layout,
            descriptor_set,
            clear_colour: Colour::black(),
            pipeline_manager,
            meshes: SlotMap::default(),
            render_models: SlotMap::default(),
            light_buffer,
            transform_buffer,
            material_buffer,
            light_mesh: None,
            stored_lights: SlotMap::default(),
            stored_cameras: SlotMap::default(),
            active_camera: None,
            shadow_pso,
            sun,
            ui_pso,
            ui_pso_layout,
            ui_descriptor_set_layout,
            ui_descriptor_set,
            quad_buffer,
            index_buffer,
            ui_to_draw: Vec::new(),
            ui_uniform_data,
            render_image,
            depth_image,
            directional_light_shadow_image,
            render_targets,
            descriptor_layout_cache,
            descriptor_allocator,
        })
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) -> Result<()> {
        if self.device.resize(new_size)? {
            self.render_targets.recreate_render_targets()?;
        }

        Ok(())
    }

    pub fn reload_shaders(&mut self) -> Result<()> {
        profiling::scope!("Reload shaders");
        self.pipeline_manager.reload_shaders(&self.device)?;
        info!("Shaders reloaded!");
        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        profiling::scope!("Render Frame");

        let present_index = self.device.start_frame()?;

        // Get images

        let render_image = self
            .render_targets
            .get_render_target(self.render_image)
            .unwrap()
            .image();
        let depth_image = self
            .render_targets
            .get_render_target(self.depth_image)
            .unwrap()
            .image();
        let shadow_image = self
            .render_targets
            .get_render_target(self.directional_light_shadow_image)
            .unwrap()
            .image();

        // Copy camera
        if let Some(camera) = self.active_camera {
            if let Some(found_camera) = self.stored_cameras.get(camera) {
                self.camera_uniform.update_proj(found_camera);
            } else {
                self.active_camera = None;
                error!("Unable to find stored camera, resetting to default")
            }
        } else {
            self.camera_uniform.update_proj(&self.default_camera);
        }
        self.camera_uniform.update_light(&self.sun);

        self.device
            .resource_manager
            .get_buffer(self.camera_buffer[self.device.buffered_resource_number()])
            .unwrap()
            .view()
            .mapped_slice()?
            .copy_from_slice(&[self.camera_uniform]);

        let test = self.stored_lights.values();
        let uniforms: Vec<LightUniform> = test.map(|&light| LightUniform::from(light)).collect();

        self.device
            .resource_manager
            .get_buffer(self.light_buffer[self.device.buffered_resource_number()])
            .unwrap()
            .view_custom::<LightUniform>(0, uniforms.len())?
            .mapped_slice()?
            .copy_from_slice(&uniforms);

        // Copy objects model matrix

        let mut transform_matrices = Vec::new();
        for model in self.render_models.values() {
            let transform = TransformSSBO {
                model: model.transform.into(),
                normal: model.transform.invert().unwrap().transpose().into(),
            };
            transform_matrices.push(transform);
        }
        for light in self.stored_lights.values() {
            let transform = TransformSSBO {
                model: Matrix4::from_translation(light.position.to_vec()).into(),
                normal: Matrix4::from_translation(light.position.to_vec())
                    .invert()
                    .unwrap()
                    .transpose()
                    .into(),
            };
            transform_matrices.push(transform);
        }

        self.device
            .resource_manager
            .get_buffer(self.transform_buffer[self.device.buffered_resource_number()])
            .unwrap()
            .view_custom(0, transform_matrices.len())?
            .mapped_slice()?
            .copy_from_slice(&transform_matrices);

        let mut materials = Vec::new();
        for model in self.render_models.values() {
            let material_params = self.get_material_ssbo_from_instance(&model.material_instance);
            materials.push(material_params);
        }
        // Push light materials
        for light in self.stored_lights.values() {
            materials.push(self.get_material_ssbo_from_instance(&MaterialInstance {
                diffuse: Vector4::zero(),
                emissive: light.colour,
                ..Default::default()
            }));
        }

        self.device
            .resource_manager
            .get_buffer(self.material_buffer[self.device.buffered_resource_number()])
            .unwrap()
            .view_custom(0, materials.len())?
            .mapped_slice()?
            .copy_from_slice(&materials);

        // Fill draw commands
        let mut draw_data: Vec<DrawData> = Vec::new();
        for (i, model) in self.render_models.keys().enumerate() {
            let model = self.render_models.get(model).unwrap();
            if let Some(mesh) = self.meshes.get(model.mesh_handle) {
                draw_data.push(DrawData {
                    vertex_buffer: mesh.vertex_buffer,
                    index_buffer: mesh.index_buffer,
                    index_count: mesh.vertex_count,
                    transform_index: i,
                    material_index: i,
                });
            }
        }
        for i in 0..self.stored_lights.len() {
            let i = i + self.render_models.len();
            if let Some(mesh) = self.meshes.get(self.light_mesh.unwrap()) {
                draw_data.push(DrawData {
                    vertex_buffer: mesh.vertex_buffer,
                    index_buffer: mesh.index_buffer,
                    index_count: mesh.vertex_count,
                    transform_index: i,
                    material_index: i,
                });
            }
        }

        // Barrier images

        ImageBarrierBuilder::default()
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::SwapchainImage(),
                dst_stage_mask: PipelineStageFlags2::BLIT,
                dst_access_mask: AccessFlags2::TRANSFER_WRITE,
                new_layout: ImageLayout::TRANSFER_DST_OPTIMAL,
                ..Default::default()
            })
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::Image(render_image),
                dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                ..Default::default()
            })
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::Image(depth_image),
                dst_stage_mask: PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                dst_access_mask: AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                ..Default::default()
            })
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::Image(
                    self.render_targets
                        .get_render_target(self.directional_light_shadow_image)
                        .unwrap()
                        .image(),
                ),
                dst_stage_mask: PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                dst_access_mask: AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                ..Default::default()
            })
            .build(
                &self.device,
                &self.device.graphics_command_buffer[self.device.buffered_resource_number()],
            )?;

        // Shadow pass
        RenderPassBuilder::new((SHADOWMAP_SIZE, SHADOWMAP_SIZE))
            .set_depth_attachment(AttachmentInfo {
                target: AttachmentHandle::Image(shadow_image),
                clear_value: vk::ClearValue {
                    depth_stencil: ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
                ..Default::default()
            })
            .start(
                &self.device,
                &self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                |_render_pass| {
                    profiling::scope!("Shadow Pass");

                    let pipeline = self.pipeline_manager.get_pipeline(self.shadow_pso);
                    unsafe {
                        self.device.vk_device.cmd_bind_pipeline(
                            self.device.graphics_command_buffer
                                [self.device.buffered_resource_number()],
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline,
                        );
                        self.device.vk_device.cmd_bind_descriptor_sets(
                            self.device.graphics_command_buffer
                                [self.device.buffered_resource_number()],
                            vk::PipelineBindPoint::GRAPHICS,
                            self.pso_layout,
                            0u32,
                            &[
                                self.device.bindless_descriptor_set()
                                    [self.device.buffered_resource_number()],
                                self.descriptor_set[self.device.buffered_resource_number()],
                            ],
                            &[],
                        );
                    };

                    // Draw commands
                    self.draw_objects(&draw_data)?;
                    Ok(())
                },
            )?;

        ImageBarrierBuilder::default()
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::Image(
                    self.render_targets
                        .get_render_target(self.directional_light_shadow_image)
                        .unwrap()
                        .image(),
                ),
                src_stage_mask: PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                src_access_mask: AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                dst_access_mask: AccessFlags2::SHADER_READ,
                old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ..Default::default()
            })
            .build(
                &self.device,
                &self.device.graphics_command_buffer[self.device.buffered_resource_number()],
            )?;

        // Normal Pass
        let clear_colour: Vector3<f32> = self.clear_colour.into();
        RenderPassBuilder::new((self.device.size().width, self.device.size().height))
            .add_colour_attachment(AttachmentInfo {
                target: AttachmentHandle::Image(render_image),
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: clear_colour.extend(0f32).into(),
                    },
                },
                ..Default::default()
            })
            .set_depth_attachment(AttachmentInfo {
                target: AttachmentHandle::Image(depth_image),
                clear_value: vk::ClearValue {
                    depth_stencil: ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
                ..Default::default()
            })
            .start(
                &self.device,
                &self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                |_render_pass| {
                    profiling::scope!("Forward Pass");
                    let pipeline = self.pipeline_manager.get_pipeline(self.pso);

                    unsafe {
                        self.device.vk_device.cmd_bind_pipeline(
                            self.device.graphics_command_buffer
                                [self.device.buffered_resource_number()],
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline,
                        );
                        self.device.vk_device.cmd_bind_descriptor_sets(
                            self.device.graphics_command_buffer
                                [self.device.buffered_resource_number()],
                            vk::PipelineBindPoint::GRAPHICS,
                            self.pso_layout,
                            0u32,
                            &[
                                self.device.bindless_descriptor_set()
                                    [self.device.buffered_resource_number()],
                                self.descriptor_set[self.device.buffered_resource_number()],
                            ],
                            &[],
                        );
                    };

                    // Draw commands

                    self.draw_objects(&draw_data)?;
                    Ok(())
                },
            )?;

        // Copy UI

        let ui_uniform = UIUniformData {
            screen_size: [
                self.device.size().width as f32,
                self.device.size().height as f32,
            ],
        };
        self.device
            .resource_manager
            .get_buffer(self.ui_uniform_data[self.device.buffered_resource_number()])
            .unwrap()
            .view()
            .mapped_slice()?
            .copy_from_slice(&[ui_uniform]);

        let ui_draw_calls = {
            let mut ui_draw_calls = Vec::new();

            let mut vertex_offset = 0usize;
            let mut index_offset = 0usize;
            for element in self.ui_to_draw.iter_mut() {
                let texture_id = {
                    if let Some(index) = self.device.get_descriptor_index(&element.texture_id) {
                        index as i32
                    } else {
                        0
                    }
                };

                let verts: Vec<UIVertexData> = element
                    .vertices
                    .iter()
                    .map(|vert| UIVertexData {
                        pos: vert.pos,
                        uv: vert.uv,
                        colour: vert.colour,
                        texture_id: [texture_id, 0, 0, 0],
                    })
                    .collect();

                self.device
                    .resource_manager
                    .get_buffer(self.quad_buffer[self.device.buffered_resource_number()])
                    .unwrap()
                    .view_custom(vertex_offset, verts.len())?
                    .mapped_slice()?
                    .copy_from_slice(&verts);

                self.device
                    .resource_manager
                    .get_buffer(self.index_buffer[self.device.buffered_resource_number()])
                    .unwrap()
                    .view_custom(index_offset, element.indices.len())?
                    .mapped_slice()?
                    .copy_from_slice(&element.indices);

                ui_draw_calls.push(UIDrawCall {
                    vertex_offset,
                    index_offset,
                    amount: element.indices.len(),
                    scissor: element.scissor,
                });

                vertex_offset += verts.len();
                index_offset += element.indices.len() + 16000;
            }
            self.ui_to_draw.clear();
            ui_draw_calls
        };

        // UI Pass
        RenderPassBuilder::new((self.device.size().width, self.device.size().height))
            .add_colour_attachment(AttachmentInfo {
                target: AttachmentHandle::Image(render_image),
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: clear_colour.extend(0f32).into(),
                    },
                },
                load_op: vk::AttachmentLoadOp::LOAD,
                ..Default::default()
            })
            .start(
                &self.device,
                &self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                |render_pass| {
                    profiling::scope!("UI Pass");
                    let pipeline = self.pipeline_manager.get_pipeline(self.ui_pso);

                    unsafe {
                        self.device.vk_device.cmd_bind_pipeline(
                            self.device.graphics_command_buffer
                                [self.device.buffered_resource_number()],
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline,
                        );
                        self.device.vk_device.cmd_bind_descriptor_sets(
                            self.device.graphics_command_buffer
                                [self.device.buffered_resource_number()],
                            vk::PipelineBindPoint::GRAPHICS,
                            self.ui_pso_layout,
                            0u32,
                            &[
                                self.device.bindless_descriptor_set()
                                    [self.device.buffered_resource_number()],
                                self.ui_descriptor_set[self.device.buffered_resource_number()],
                            ],
                            &[],
                        );
                    };

                    let index_buffer = self
                        .device
                        .resource_manager
                        .get_buffer(self.index_buffer[self.device.buffered_resource_number()])
                        .unwrap();

                    unsafe {
                        self.device.vk_device.cmd_bind_index_buffer(
                            self.device.graphics_command_buffer
                                [self.device.buffered_resource_number()],
                            index_buffer.buffer(),
                            0u64,
                            vk::IndexType::UINT32,
                        );
                    }

                    for draw in ui_draw_calls.iter() {
                        let max = [
                            draw.scissor.1[0] - draw.scissor.0[0],
                            draw.scissor.1[1] - draw.scissor.0[1],
                        ];
                        render_pass.set_scissor(draw.scissor.0, max);
                        // Draw commands
                        unsafe {
                            self.device.vk_device.cmd_draw_indexed(
                                self.device.graphics_command_buffer
                                    [self.device.buffered_resource_number()],
                                draw.amount as u32,
                                1u32,
                                draw.index_offset as u32,
                                draw.vertex_offset as i32,
                                0u32,
                            );
                        };
                    }
                    Ok(())
                },
            )?;

        // Transition render image to transfer src

        ImageBarrierBuilder::default()
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::Image(
                    self.render_targets
                        .get_render_target(self.render_image)
                        .unwrap()
                        .image(),
                ),
                src_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                src_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                dst_stage_mask: PipelineStageFlags2::BLIT,
                dst_access_mask: AccessFlags2::TRANSFER_READ,
                old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                new_layout: ImageLayout::TRANSFER_SRC_OPTIMAL,
                ..Default::default()
            })
            .build(
                &self.device,
                &self.device.graphics_command_buffer[self.device.buffered_resource_number()],
            )?;

        // Blit to swapchain

        let image_blit = vk::ImageBlit::builder()
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D {
                    x: self.device.size().width as i32,
                    y: self.device.size().height as i32,
                    z: 1,
                },
            ])
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .dst_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D {
                    x: self.device.size().width as i32,
                    y: self.device.size().height as i32,
                    z: 1,
                },
            ]);
        let regions = [*image_blit];
        unsafe {
            self.device.vk_device.cmd_blit_image(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                self.device
                    .resource_manager
                    .get_image(
                        self.render_targets
                            .get_render_target(self.render_image)
                            .unwrap()
                            .image(),
                    )
                    .unwrap()
                    .image(),
                ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.device.get_present_image(),
                ImageLayout::TRANSFER_DST_OPTIMAL,
                &regions,
                vk::Filter::NEAREST,
            )
        }

        ImageBarrierBuilder::default()
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::SwapchainImage(),
                src_stage_mask: PipelineStageFlags2::BLIT,
                src_access_mask: AccessFlags2::TRANSFER_WRITE,
                dst_stage_mask: PipelineStageFlags2::NONE,
                dst_access_mask: AccessFlags2::NONE,
                old_layout: ImageLayout::TRANSFER_DST_OPTIMAL,
                new_layout: ImageLayout::PRESENT_SRC_KHR,
                ..Default::default()
            })
            .build(
                &self.device,
                &self.device.graphics_command_buffer[self.device.buffered_resource_number()],
            )?;

        //Submit buffer

        unsafe {
            self.device.vk_device.end_command_buffer(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
            )
        }?;

        let wait_semaphores =
            [self.device.present_complete_semaphore[self.device.buffered_resource_number()]];
        let wait_dst_stage_mask = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers =
            [self.device.graphics_command_buffer[self.device.buffered_resource_number()]];
        let signal_semaphores =
            [self.device.rendering_complete_semaphore[self.device.buffered_resource_number()]];
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_dst_stage_mask)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);

        let submits = [*submit_info];
        let result = unsafe {
            self.device.vk_device.queue_submit(
                self.device.graphics_queue,
                &submits,
                self.device.draw_commands_reuse_fence[self.device.buffered_resource_number()],
            )
        };
        if let Some(error) = result.err() {
            error!("{}", error);
        }

        self.device.end_frame()?;
        Ok(())
    }

    fn draw_objects(&self, draws: &[DrawData]) -> Result<()> {
        for draw in draws.iter() {
            let push_constants = PushConstants {
                handles: [
                    draw.transform_index as i32,
                    draw.material_index as i32,
                    self.device
                        .get_descriptor_index(
                            &self
                                .render_targets
                                .get_render_target(self.directional_light_shadow_image)
                                .unwrap()
                                .image(),
                        )
                        .unwrap() as i32,
                    0,
                ],
            };
            unsafe {
                self.device.vk_device.cmd_push_constants(
                    self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                    self.pso_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0u32,
                    bytemuck::cast_slice(&[push_constants]),
                )
            };

            let vertex_buffer = self
                .device
                .resource_manager
                .get_buffer(draw.vertex_buffer)
                .unwrap()
                .buffer();
            let index_buffer = self
                .device
                .resource_manager
                .get_buffer(draw.index_buffer.unwrap())
                .unwrap()
                .buffer();

            unsafe {
                self.device.vk_device.cmd_bind_vertex_buffers(
                    self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                    0u32,
                    &[vertex_buffer],
                    &[0u64],
                );
                self.device.vk_device.cmd_bind_index_buffer(
                    self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                    index_buffer,
                    DeviceSize::zero(),
                    IndexType::UINT32,
                );
                self.device.vk_device.cmd_draw_indexed(
                    self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                    draw.index_count,
                    1u32,
                    0u32,
                    0i32,
                    0u32,
                );
            }
        }
        Ok(())
    }

    /// Loads a texture into GPU memory and returns back a Texture or an error.
    ///
    /// # Arguments
    ///
    /// * `file_location`: The location of the texture. This is from where the .exe is stored.
    ///
    /// returns: Result<Texture, String>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn load_texture(
        &mut self,
        file_location: &str,
        image_type: &ImageFormatType,
    ) -> Result<ImageHandle> {
        profiling::scope!("Renderer: Load Texture");

        let img = {
            profiling::scope!("image::open");
            image::open(file_location)
        };

        if let Err(error) = img {
            return Err(anyhow!(error.to_string()));
        }

        let img = img?;
        let rgba_img = img.to_rgba8();
        let img_bytes = rgba_img.as_bytes();
        let mip_levels = (img.width().max(img.height()) as f32).log2().floor() as u32 + 1u32;

        let image = self.load_texture_from_bytes(
            img_bytes,
            img.width(),
            img.height(),
            image_type,
            mip_levels,
        )?;

        // Debug name image
        {
            let image_name = file_location.rsplit_once('/').unwrap().1;
            let name = "Image:".to_string() + image_name;
            let image_handle = self
                .device
                .resource_manager
                .get_image(image)
                .unwrap()
                .image()
                .as_raw();
            self.device
                .set_vulkan_debug_name(image_handle, ObjectType::IMAGE, &name)?;

            info!(
                "Texture Loaded: {} | Size: [{},{}] | Mip Levels:[{}]",
                image_name,
                img.width(),
                img.height(),
                mip_levels
            );
        }

        Ok(image)
    }

    pub fn load_texture_from_bytes(
        &self,
        img_bytes: &[u8],
        img_width: u32,
        img_height: u32,
        image_type: &ImageFormatType,
        mip_levels: u32,
    ) -> Result<ImageHandle> {
        profiling::scope!("Renderer: Load Texture(From Bytes)");

        let image = self
            .device
            .load_image(img_bytes, img_width, img_height, image_type, mip_levels)?;

        Ok(image)
    }

    pub fn load_mesh(&mut self, mesh: &MeshData) -> Result<MeshHandle> {
        profiling::scope!("Load Mesh");

        let vertex_buffer = {
            let staging_buffer_create_info = BufferCreateInfo {
                size: (size_of::<Vertex>() * mesh.vertices.len()),
                usage: vk::BufferUsageFlags::TRANSFER_SRC,
                storage_type: BufferStorageType::HostLocal,
            };

            let staging_buffer = self
                .device
                .resource_manager
                .create_buffer(&staging_buffer_create_info);

            self.device
                .resource_manager
                .get_buffer(staging_buffer)
                .unwrap()
                .view()
                .mapped_slice()?
                .copy_from_slice(mesh.vertices.as_slice());

            let vertex_buffer_create_info = BufferCreateInfo {
                size: (size_of::<Vertex>() * mesh.vertices.len()),
                usage: vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER,
                storage_type: BufferStorageType::Device,
            };

            let buffer = self
                .device
                .resource_manager
                .create_buffer(&vertex_buffer_create_info);

            self.device.immediate_submit(|device, cmd| {
                cmd_copy_buffer(device, cmd, staging_buffer, buffer)?;
                Ok(())
            })?;

            buffer
        };
        match &mesh.indices {
            None => {
                let render_mesh = RenderMesh {
                    vertex_buffer,
                    index_buffer: None,
                    vertex_count: mesh.vertices.len() as u32,
                };
                trace!(
                    "Mesh Loaded. Vertex Count:{}|Faces:{}",
                    mesh.vertices.len(),
                    mesh.faces.len()
                );
                Ok(self.meshes.insert(render_mesh))
            }
            Some(indices) => {
                let index_buffer = {
                    let buffer_size = size_of::<u32>() * indices.len();
                    let staging_buffer_create_info = BufferCreateInfo {
                        size: buffer_size,
                        usage: vk::BufferUsageFlags::TRANSFER_SRC,
                        storage_type: BufferStorageType::HostLocal,
                    };

                    let staging_buffer = self
                        .device
                        .resource_manager
                        .create_buffer(&staging_buffer_create_info);

                    self.device
                        .resource_manager
                        .get_buffer(staging_buffer)
                        .unwrap()
                        .view()
                        .mapped_slice()?
                        .copy_from_slice(indices.as_slice());

                    let index_buffer_create_info = BufferCreateInfo {
                        size: buffer_size,
                        usage: vk::BufferUsageFlags::TRANSFER_DST
                            | vk::BufferUsageFlags::INDEX_BUFFER,
                        storage_type: BufferStorageType::Device,
                    };

                    let buffer = self
                        .device
                        .resource_manager
                        .create_buffer(&index_buffer_create_info);

                    self.device.immediate_submit(|device, cmd| {
                        cmd_copy_buffer(device, cmd, staging_buffer, buffer)?;
                        Ok(())
                    })?;

                    buffer
                };
                let render_mesh = RenderMesh {
                    vertex_buffer,
                    index_buffer: Some(index_buffer),
                    vertex_count: indices.len() as u32,
                };
                trace!(
                    "Mesh Loaded. Vertex Count:{}|Index Count:{}|Faces:{}",
                    mesh.vertices.len(),
                    mesh.indices.as_ref().unwrap().len(),
                    mesh.faces.len()
                );
                Ok(self.meshes.insert(render_mesh))
            }
        }
    }

    fn get_material_ssbo_from_instance(&self, instance: &MaterialInstance) -> MaterialParamSSBO {
        let diffuse_tex = {
            if let Some(tex) = instance.diffuse_texture {
                self.device.get_descriptor_index(&tex).unwrap()
            } else {
                0usize
            }
        };

        let normal_tex = {
            if let Some(tex) = instance.normal_texture {
                self.device.get_descriptor_index(&tex).unwrap()
            } else {
                0usize
            }
        };

        let metallic_roughness_tex = {
            if let Some(tex) = instance.metallic_roughness_texture {
                self.device.get_descriptor_index(&tex).unwrap()
            } else {
                0usize
            }
        };

        let emissive_tex = {
            if let Some(tex) = instance.emissive_texture {
                self.device.get_descriptor_index(&tex).unwrap()
            } else {
                0usize
            }
        };

        let occlusion_tex = {
            if let Some(tex) = instance.occlusion_texture {
                self.device.get_descriptor_index(&tex).unwrap()
            } else {
                0usize
            }
        };

        MaterialParamSSBO {
            diffuse: instance.diffuse.into(),
            emissive: instance.emissive.extend(0f32).into(),
            textures: [
                diffuse_tex as i32,
                normal_tex as i32,
                metallic_roughness_tex as i32,
                occlusion_tex as i32,
                emissive_tex as i32,
                0,
                0,
                0,
            ],
        }
    }

    pub fn add_render_model(
        &mut self,
        handle: MeshHandle,
        textures: MaterialInstance,
    ) -> RenderModelHandle {
        self.render_models.insert(RenderModel {
            mesh_handle: handle,
            material_instance: textures,
            transform: from_transforms(
                Vector3::from_value(0f32),
                Quaternion::from_axis_angle(Vector3::new(0.0f32, 1.0f32, 0.0f32), Deg(0f32)),
                Vector3::from_value(1f32),
            ),
        })
    }

    pub fn set_render_model_transform(
        &mut self,
        handle: RenderModelHandle,
        transform: Matrix4<f32>,
    ) -> Result<()> {
        if let Some(model) = self.render_models.get_mut(handle) {
            model.transform = transform;
            Ok(())
        } else {
            Err(anyhow!("Unable to find Render Model!"))
        }
    }

    pub fn create_light(&mut self, light: &Light) -> Option<LightHandle> {
        if self.stored_lights.len() >= 4 {
            warn!("Tried to create light, but reached max limit of 4.");
            return None;
        }

        let handle = self.stored_lights.insert(*light);
        Some(handle)
    }

    pub fn set_light(&mut self, light_handle: LightHandle, light: &Light) -> Result<()> {
        if let Some(modified_light) = self.stored_lights.get_mut(light_handle) {
            let _old = std::mem::replace(modified_light, *light);
            return Ok(());
        }
        Err(anyhow!("No light exists"))
    }

    pub fn create_camera(&mut self, camera: &Camera) -> CameraHandle {
        self.stored_cameras.insert(*camera)
    }

    pub fn set_camera(&mut self, handle: CameraHandle, camera: &Camera) -> Result<()> {
        if let Some(modified_camera) = self.stored_cameras.get_mut(handle) {
            let _old = std::mem::replace(modified_camera, *camera);
            return Ok(());
        }
        Err(anyhow!("No camera exists"))
    }

    pub fn draw_ui(&mut self, ui: UIMesh) -> Result<()> {
        self.ui_to_draw.push(ui);
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.device.vk_device.device_wait_idle().unwrap();
            self.pipeline_manager.deinit();
            self.descriptor_layout_cache.cleanup();
            self.descriptor_allocator.cleanup();
        }
    }
}

impl Vertex {
    fn get_ui_vertex_input_desc() -> VertexInputDescription {
        VertexInputDescription {
            bindings: vec![],
            attributes: vec![],
        }
    }

    fn get_vertex_input_desc() -> VertexInputDescription {
        let main_binding = vk::VertexInputBindingDescription::builder()
            .input_rate(vk::VertexInputRate::VERTEX)
            .binding(0u32)
            .stride(size_of::<Vertex>() as u32);

        VertexInputDescription {
            bindings: vec![*main_binding],
            attributes: vec![
                vk::VertexInputAttributeDescription {
                    location: 0,
                    binding: 0,
                    format: vk::Format::R32G32B32_SFLOAT,
                    offset: offset_of!(Vertex, position) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 1,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: offset_of!(Vertex, tex_coords) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 2,
                    binding: 0,
                    format: vk::Format::R32G32B32_SFLOAT,
                    offset: offset_of!(Vertex, normal) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 3,
                    binding: 0,
                    format: vk::Format::R32G32B32_SFLOAT,
                    offset: offset_of!(Vertex, color) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 4,
                    binding: 0,
                    format: vk::Format::R32G32B32A32_SFLOAT,
                    offset: offset_of!(Vertex, tangent) as u32,
                },
            ],
        }
    }
}

new_key_type! {pub struct MeshHandle; pub struct RenderModelHandle; pub struct LightHandle; pub struct CameraHandle;}

// Mesh data stored on the GPU
struct RenderMesh {
    vertex_buffer: BufferHandle,
    index_buffer: Option<BufferHandle>,
    vertex_count: u32,
}

fn from_transforms(
    position: Vector3<f32>,
    rotation: Quaternion<f32>,
    size: Vector3<f32>,
) -> Matrix4<f32> {
    let translation = Matrix4::from_translation(position);
    // TODO : Fix rotation when position is zero
    let rotation = Matrix4::from(rotation);
    let scale = Matrix4::from_nonuniform_scale(size.x, size.y, size.z);

    let mut model = translation;
    model = model * rotation;
    model = model * scale;
    model
}

#[derive(Clone)]
pub struct MaterialInstance {
    pub diffuse: Vector4<f32>,
    pub emissive: Vector3<f32>,

    pub diffuse_texture: Option<ImageHandle>,
    pub normal_texture: Option<ImageHandle>,
    pub metallic_roughness_texture: Option<ImageHandle>,
    pub emissive_texture: Option<ImageHandle>,
    pub occlusion_texture: Option<ImageHandle>,
}

impl Default for MaterialInstance {
    fn default() -> Self {
        Self {
            diffuse: Vector4::from_value(1.0f32),
            emissive: Vector3::from_value(0.0f32),
            diffuse_texture: None,
            normal_texture: None,
            metallic_roughness_texture: None,
            emissive_texture: None,
            occlusion_texture: None,
        }
    }
}

struct RenderModel {
    mesh_handle: MeshHandle,
    material_instance: MaterialInstance,
    transform: Matrix4<f32>,
}

struct DrawData {
    vertex_buffer: BufferHandle,
    index_buffer: Option<BufferHandle>,
    index_count: u32,
    transform_index: usize,
    material_index: usize,
}

pub struct UIVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub colour: [f32; 4],
}

pub struct UIMesh {
    pub indices: Vec<u32>,
    pub vertices: Vec<UIVertex>,
    pub texture_id: ImageHandle,
    pub scissor: ([f32; 2], [f32; 2]),
}

struct UIDrawCall {
    vertex_offset: usize,
    index_offset: usize,
    amount: usize,
    scissor: ([f32; 2], [f32; 2]),
}
