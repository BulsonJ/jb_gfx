use std::mem::size_of;

use anyhow::{anyhow, Result};
use ash::vk;
use ash::vk::{
    AccessFlags2, ClearDepthStencilValue, DeviceSize, Handle, ImageAspectFlags, ImageLayout,
    IndexType, ObjectType, PipelineStageFlags2,
};
use bytemuck::{offset_of, Zeroable};
use cgmath::{
    Array, Deg, InnerSpace, Matrix, Matrix4, Quaternion, Rotation3, SquareMatrix, Vector3, Vector4,
    Zero,
};
use image::EncodableLayout;
use log::{error, info, trace, warn};
use slotmap::{new_key_type, SlotMap};
use winit::{dpi::PhysicalSize, window::Window};

use crate::barrier::{ImageBarrier, ImageBarrierBuilder, ImageHandleType};
use crate::bindless::BindlessImage;
use crate::device::{cmd_copy_buffer, GraphicsDevice, ImageFormatType, FRAMES_IN_FLIGHT};
use crate::gpu_structs::{
    CameraUniform, LightUniform, MaterialParamSSBO, PushConstants, TransformSSBO,
};
use crate::pipeline::{PipelineCreateInfo, PipelineHandle, PipelineManager};
use crate::resource::{BufferCreateInfo, BufferHandle, BufferStorageType, ImageHandle};
use crate::{Camera, Colour, MeshData, Vertex};

const MAX_OBJECTS: u64 = 1000u64;

/// The renderer for the GameEngine.
/// Used to draw objects using the GPU.
pub struct Renderer {
    device: GraphicsDevice,
    pso_layout: vk::PipelineLayout,
    pso: PipelineHandle,
    camera_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    camera_uniform: CameraUniform,
    default_camera: Camera,
    descriptor_pool: vk::DescriptorPool,
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
}

impl Renderer {
    pub fn new(window: &Window) -> Result<Self> {
        profiling::scope!("Renderer::new");

        let mut device = GraphicsDevice::new(window)?;

        let vertex_input_desc = Vertex::get_vertex_input_desc();

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&vertex_input_desc.bindings)
            .vertex_attribute_descriptions(&vertex_input_desc.attributes);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false)
            .min_depth_bounds(0.0f32)
            .max_depth_bounds(1.0f32);

        let pool_sizes = [
            *vk::DescriptorPoolSize::builder()
                .descriptor_count(100u32)
                .ty(vk::DescriptorType::UNIFORM_BUFFER),
            *vk::DescriptorPoolSize::builder()
                .descriptor_count(100u32)
                .ty(vk::DescriptorType::STORAGE_BUFFER),
            *vk::DescriptorPoolSize::builder()
                .descriptor_count(1000u32)
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        ];

        let pool_create_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(4u32)
            .pool_sizes(&pool_sizes);

        let descriptor_pool = unsafe {
            device
                .vk_device
                .create_descriptor_pool(&pool_create_info, None)
        }?;

        let descriptor_set_bindings = [
            *vk::DescriptorSetLayoutBinding::builder()
                .binding(0u32)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1u32)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            *vk::DescriptorSetLayoutBinding::builder()
                .binding(1u32)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1u32)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            *vk::DescriptorSetLayoutBinding::builder()
                .binding(2u32)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1u32)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            *vk::DescriptorSetLayoutBinding::builder()
                .binding(3u32)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1u32)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        let descriptor_set_layout_create_info =
            vk::DescriptorSetLayoutCreateInfo::builder().bindings(&descriptor_set_bindings);

        let descriptor_set_layout = unsafe {
            device
                .vk_device
                .create_descriptor_set_layout(&descriptor_set_layout_create_info, None)
        }?;

        let push_constant_range = *vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .size(size_of::<PushConstants>() as u32)
            .offset(0u32);

        let layouts = [
            *device.bindless_descriptor_set_layout(),
            descriptor_set_layout,
        ];
        let push_constant_ranges = [push_constant_range];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&layouts)
            .push_constant_ranges(&push_constant_ranges);

        let pso_layout = unsafe {
            device
                .vk_device
                .create_pipeline_layout(&pipeline_layout_info, None)
        }?;

        let render_image_format = {
            device
                .resource_manager
                .get_image(
                    device
                        .render_targets()
                        .get_render_target(device.render_image)
                        .unwrap()
                        .image(),
                )
                .unwrap()
                .format()
        };
        let depth_image_format = {
            device
                .resource_manager
                .get_image(
                    device
                        .render_targets()
                        .get_render_target(device.depth_image)
                        .unwrap()
                        .image(),
                )
                .unwrap()
                .format()
        };

        let pso_build_info = PipelineCreateInfo {
            pipeline_layout: pso_layout,
            vertex_shader: "assets/shaders/default.vert".to_string(),
            fragment_shader: "assets/shaders/default.frag".to_string(),
            vertex_input_state: *vertex_input_state,
            color_attachment_formats: vec![render_image_format],
            depth_attachment_format: Some(depth_image_format),
            depth_stencil_state: *depth_stencil_state,
        };

        // TODO : Move more into PipelineManager
        let mut pipeline_manager = PipelineManager::new();
        let pso = pipeline_manager.create_pipeline(&mut device, &pso_build_info)?;

        let camera = Camera {
            position: (0.0, -100.0, -2.0).into(),
            rotation: 90f32,
            aspect: device.size.width as f32 / device.size.height as f32,
            fovy: 90.0,
            znear: 0.1,
            zfar: 4000.0,
        };

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_proj(&camera);
        camera_uniform.ambient_light = Vector4::new(1.0, 1.0, 1.0, 0.0).into();
        camera_uniform.directional_light_direction =
            Vector4::new(0.0, 1.0, 0.25, 0.0).normalize().into();
        camera_uniform.directional_light_colour = Vector4::new(1.0, 1.0, 1.0, 0.0).into();

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

        let descriptor_set = {
            let set_layouts = [descriptor_set_layout];
            let create_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&set_layouts);

            let descriptor_sets =
                unsafe { device.vk_device.allocate_descriptor_sets(&create_info) }?;
            let first = *descriptor_sets.get(0).unwrap();
            let descriptor_sets =
                unsafe { device.vk_device.allocate_descriptor_sets(&create_info) }?;
            let second = *descriptor_sets.get(0).unwrap();

            [first, second]
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
                .get_buffer_mut(*camera_buffer)
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
                .get_buffer_mut(*light_buffer)
                .unwrap()
                .view()
                .mapped_slice()?
                .copy_from_slice(&uniforms);

            let camera_buffer_write = vk::DescriptorBufferInfo::builder()
                .buffer(
                    device
                        .resource_manager
                        .get_buffer(*camera_buffer)
                        .unwrap()
                        .buffer(),
                )
                .range(size_of::<CameraUniform>() as DeviceSize);

            let transform_buffer_write = {
                let buffer = device
                    .resource_manager
                    .get_buffer(*transform_buffer)
                    .unwrap();

                vk::DescriptorBufferInfo::builder()
                    .buffer(buffer.buffer())
                    .range(buffer.size())
            };

            let material_buffer_write = {
                let buffer = device
                    .resource_manager
                    .get_buffer(*material_buffer)
                    .unwrap();

                vk::DescriptorBufferInfo::builder()
                    .buffer(buffer.buffer())
                    .range(buffer.size())
            };

            let light_buffer_write = {
                let buffer = device.resource_manager.get_buffer(*light_buffer).unwrap();

                vk::DescriptorBufferInfo::builder()
                    .buffer(buffer.buffer())
                    .range(buffer.size())
            };

            let desc_set_writes = [
                *vk::WriteDescriptorSet::builder()
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .dst_binding(0)
                    .dst_set(*set)
                    .buffer_info(&[*camera_buffer_write]),
                *vk::WriteDescriptorSet::builder()
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .dst_binding(1)
                    .dst_set(*set)
                    .buffer_info(&[*light_buffer_write]),
                *vk::WriteDescriptorSet::builder()
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .dst_binding(2)
                    .dst_set(*set)
                    .buffer_info(&[*transform_buffer_write]),
                *vk::WriteDescriptorSet::builder()
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .dst_binding(3)
                    .dst_set(*set)
                    .buffer_info(&[*material_buffer_write]),
            ];

            unsafe {
                device
                    .vk_device
                    .update_descriptor_sets(&desc_set_writes, &[])
            };
        }

        info!("Renderer Created");
        Ok(Self {
            device,
            pso_layout,
            pso,
            camera_buffer,
            camera_uniform,
            default_camera: camera,
            descriptor_pool,
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
        })
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) -> Result<()> {
        self.device.resize(new_size)?;
        Ok(())
    }

    pub fn reload_shaders(&mut self) -> Result<()> {
        profiling::scope!("Reload shaders");
        self.pipeline_manager.reload_shaders(&mut self.device)?;
        info!("Shaders reloaded!");
        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        profiling::scope!("Render Frame");

        let present_index = self.device.start_frame()?;

        let clear_colour: Vector3<f32> = self.clear_colour.into();
        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [clear_colour.x, clear_colour.y, clear_colour.z, 0.0],
            },
        };
        let depth_clear_value = vk::ClearValue {
            depth_stencil: ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        };

        let viewport = vk::Viewport::builder()
            .x(0.0f32)
            .y(self.device.size.height as f32)
            .width(self.device.size.width as f32)
            .height(-(self.device.size.height as f32))
            .min_depth(0.0f32)
            .max_depth(1.0f32);

        let scissor = vk::Rect2D::builder()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(vk::Extent2D {
                width: self.device.size.width,
                height: self.device.size.height,
            });

        unsafe {
            self.device.vk_device.cmd_set_viewport(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                0u32,
                &[*viewport],
            )
        };
        unsafe {
            self.device.vk_device.cmd_set_scissor(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                0u32,
                &[*scissor],
            )
        };

        // Memory barrier attachments

        ImageBarrierBuilder::default()
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::SwapchainImage(present_index as usize),
                dst_stage_mask: PipelineStageFlags2::BLIT,
                dst_access_mask: AccessFlags2::TRANSFER_WRITE,
                new_layout: ImageLayout::TRANSFER_DST_OPTIMAL,
                ..Default::default()
            })
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::RenderTarget(self.device.render_image),
                dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                ..Default::default()
            })
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::RenderTarget(self.device.depth_image),
                dst_stage_mask: PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                dst_access_mask: AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                ..Default::default()
            })
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::RenderTarget(self.device.directional_light_shadow_image),
                dst_stage_mask: PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                dst_access_mask: AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                ..Default::default()
            })
            .build(
                &self.device,
                &self.device.graphics_command_buffer[self.device.buffered_resource_number()],
            )?;

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

        self.device
            .resource_manager
            .get_buffer_mut(self.camera_buffer[self.device.buffered_resource_number()])
            .unwrap()
            .view()
            .mapped_slice()?
            .copy_from_slice(&[self.camera_uniform]);

        let test = self.stored_lights.values();
        let uniforms: Vec<LightUniform> = test.map(|&light| LightUniform::from(light)).collect();

        self.device
            .resource_manager
            .get_buffer_mut(self.light_buffer[self.device.buffered_resource_number()])
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
                model: Matrix4::from_translation(light.position).into(),
                normal: Matrix4::from_translation(light.position)
                    .invert()
                    .unwrap()
                    .transpose()
                    .into(),
            };
            transform_matrices.push(transform);
        }

        self.device
            .resource_manager
            .get_buffer_mut(self.transform_buffer[self.device.buffered_resource_number()])
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
                diffuse: Vector3::zero(),
                emissive: light.colour,
                ..Default::default()
            }));
        }

        self.device
            .resource_manager
            .get_buffer_mut(self.material_buffer[self.device.buffered_resource_number()])
            .unwrap()
            .view_custom(0, materials.len())?
            .mapped_slice()?
            .copy_from_slice(&materials);

        // Fill draw commands
        let mut draw_data: Vec<DrawData> = Vec::new();
        for (i, model) in self.render_models.keys().enumerate() {
            let model = self.render_models.get(model).unwrap();
            draw_data.push(DrawData {
                mesh_handle: model.mesh_handle,
                transform_index: i,
                material_index: i,
            });
        }
        if let Some(light_model) = self.light_mesh {
            for i in 0..self.stored_lights.len() {
                let i = i + self.render_models.len();

                draw_data.push(DrawData {
                    mesh_handle: light_model,
                    transform_index: i,
                    material_index: i,
                });
            }
        }

        // Shadow pass
        {
            ImageBarrierBuilder::default()
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::RenderTarget(
                        self.device.directional_light_shadow_image,
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
        }

        // Start dynamic rendering

        let color_attach_info = vk::RenderingAttachmentInfo::builder()
            .image_view(
                self.device
                    .resource_manager
                    .get_image(
                        self.device
                            .render_targets()
                            .get_render_target(self.device.render_image)
                            .unwrap()
                            .image(),
                    )
                    .unwrap()
                    .image_view(),
            )
            .image_layout(ImageLayout::ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value);

        let depth_attach_info = vk::RenderingAttachmentInfo::builder()
            .image_view(
                self.device
                    .resource_manager
                    .get_image(
                        self.device
                            .render_targets()
                            .get_render_target(self.device.depth_image)
                            .unwrap()
                            .image(),
                    )
                    .unwrap()
                    .image_view(),
            )
            .image_layout(ImageLayout::ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(depth_clear_value);

        let color_attachments = [*color_attach_info];
        let render_info = vk::RenderingInfo::builder()
            .render_area(*scissor)
            .layer_count(1u32)
            .color_attachments(&color_attachments)
            .depth_attachment(&depth_attach_info);

        let pipeline = self.pipeline_manager.get_pipeline(self.pso);

        unsafe {
            self.device.vk_device.cmd_begin_rendering(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                &render_info,
            );
            self.device.vk_device.cmd_bind_pipeline(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
            self.device.vk_device.cmd_bind_descriptor_sets(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                vk::PipelineBindPoint::GRAPHICS,
                self.pso_layout,
                0u32,
                &[self.device.bindless_descriptor_set()[self.device.buffered_resource_number()]],
                &[],
            );
            self.device.vk_device.cmd_bind_descriptor_sets(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                vk::PipelineBindPoint::GRAPHICS,
                self.pso_layout,
                1u32,
                &[self.descriptor_set[self.device.buffered_resource_number()]],
                &[],
            );
        };

        // Draw commands

        self.draw_objects(&draw_data)?;

        unsafe {
            self.device.vk_device.cmd_end_rendering(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
            )
        };

        // Transition render image to transfer src

        ImageBarrierBuilder::default()
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::RenderTarget(self.device.render_image),
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
                    x: self.device.size.width as i32,
                    y: self.device.size.height as i32,
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
                    x: self.device.size.width as i32,
                    y: self.device.size.height as i32,
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
                        self.device
                            .render_targets()
                            .get_render_target(self.device.render_image)
                            .unwrap()
                            .image(),
                    )
                    .unwrap()
                    .image(),
                ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.device.present_images[present_index as usize],
                ImageLayout::TRANSFER_DST_OPTIMAL,
                &regions,
                vk::Filter::NEAREST,
            )
        }

        // Transition to present

        let present_attachment_barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(PipelineStageFlags2::BLIT)
            .src_access_mask(AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(PipelineStageFlags2::NONE)
            .dst_access_mask(AccessFlags2::NONE)
            .old_layout(ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(ImageLayout::PRESENT_SRC_KHR)
            .image(self.device.present_images[present_index as usize])
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_memory_barriers = [*present_attachment_barrier];
        let present_barrier_dependency_info =
            vk::DependencyInfo::builder().image_memory_barriers(&image_memory_barriers);

        unsafe {
            self.device.vk_device.cmd_pipeline_barrier2(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                &present_barrier_dependency_info,
            )
        };

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

        self.device.end_frame(present_index)?;
        Ok(())
    }

    fn draw_objects(&self, draws: &[DrawData]) -> Result<()> {
        for draw in draws.iter() {
            if let Some(mesh) = self.meshes.get(draw.mesh_handle) {
                let push_constants = PushConstants {
                    handles: [
                        draw.transform_index as i32,
                        draw.material_index as i32,
                        0,
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
                    .get_buffer(mesh.vertex_buffer)
                    .unwrap()
                    .buffer();
                let index_buffer = self
                    .device
                    .resource_manager
                    .get_buffer(mesh.index_buffer.unwrap())
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
                        mesh.vertex_count,
                        1u32,
                        0u32,
                        0i32,
                        0u32,
                    );
                }
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

        let image =
            self.device
                .load_image(img_bytes, img.width(), img.height(), image_type, mip_levels)?;

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
                .get_buffer_mut(staging_buffer)
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
                        .get_buffer_mut(staging_buffer)
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
                self.device
                    .get_descriptor_index(&BindlessImage::Image(tex))
                    .unwrap()
            } else {
                0usize
            }
        };

        let normal_tex = {
            if let Some(tex) = instance.normal_texture {
                self.device
                    .get_descriptor_index(&BindlessImage::Image(tex))
                    .unwrap()
            } else {
                0usize
            }
        };

        let metallic_roughness_tex = {
            if let Some(tex) = instance.metallic_roughness_texture {
                self.device
                    .get_descriptor_index(&BindlessImage::Image(tex))
                    .unwrap()
            } else {
                0usize
            }
        };

        let emissive_tex = {
            if let Some(tex) = instance.emissive_texture {
                self.device
                    .get_descriptor_index(&BindlessImage::Image(tex))
                    .unwrap()
            } else {
                0usize
            }
        };

        let occlusion_tex = {
            if let Some(tex) = instance.occlusion_texture {
                self.device
                    .get_descriptor_index(&BindlessImage::Image(tex))
                    .unwrap()
            } else {
                0usize
            }
        };

        MaterialParamSSBO {
            diffuse: instance.diffuse.extend(0f32).into(),
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
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.device.vk_device.device_wait_idle().unwrap();
            self.pipeline_manager.deinit(&mut self.device.vk_device);
            self.device
                .vk_device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device
                .vk_device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .vk_device
                .destroy_pipeline_layout(self.pso_layout, None);
        }
    }
}

struct VertexInputDescription {
    bindings: Vec<vk::VertexInputBindingDescription>,
    attributes: Vec<vk::VertexInputAttributeDescription>,
}

impl Vertex {
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
    pub diffuse: Vector3<f32>,
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
            diffuse: Vector3::from_value(1.0f32),
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

#[derive(Copy, Clone)]
pub struct Light {
    pub position: Vector3<f32>,
    pub colour: Vector3<f32>,
}

impl Light {
    pub fn new(position: Vector3<f32>, colour: Vector3<f32>) -> Self {
        Self { position, colour }
    }
}

struct DrawData {
    mesh_handle: MeshHandle,
    transform_index: usize,
    material_index: usize,
}
