use std::mem::size_of;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use ash::vk;
use ash::vk::{
    AccessFlags2, ClearDepthStencilValue, DeviceSize, Handle, ImageLayout, IndexType, ObjectType,
    PipelineStageFlags2,
};
use bytemuck::{offset_of, Zeroable};
use cgmath::{
    Array, Deg, Matrix, Matrix4, Quaternion, Rotation3, SquareMatrix, Vector3, Vector4, Zero,
};
use image::EncodableLayout;
use log::{info, trace, warn};
use slotmap::{new_key_type, SlotMap};
use winit::{dpi::PhysicalSize, window::Window};

use crate::camera::DefaultCamera;
use crate::core::device::cmd_copy_buffer;
use crate::gpu_structs::{
    CameraUniform, LightUniform, MaterialParamSSBO, PushConstants, TransformSSBO, UIUniformData,
    UIVertexData, WorldDebugUIDrawData,
};
use crate::mesh::Index;
use crate::pipeline::{
    PipelineColorAttachment, PipelineCreateInfo, PipelineHandle, PipelineLayoutCache,
    PipelineManager, VertexInputDescription,
};
use crate::renderpass::barrier::{ImageBarrier, ImageBarrierBuilder, ImageHandleType};
use crate::renderpass::builder::{AttachmentHandle, AttachmentInfo, RenderPassBuilder};
use crate::resource::{BufferCreateInfo, BufferHandle, BufferStorageType, ImageHandle};
use crate::util::descriptor::{
    BufferDescriptorInfo, DescriptorAllocator, DescriptorLayoutBuilder, DescriptorLayoutCache,
    ImageDescriptorInfo, JBDescriptorBuilder,
};
use crate::util::targets::{RenderImageType, RenderTargetHandle, RenderTargetSize, RenderTargets};
use crate::{
    Camera, Colour, DirectionalLight, GraphicsDevice, ImageFormatType, Light, MeshData, Vertex,
    FRAMES_IN_FLIGHT, SHADOWMAP_SIZE,
};

const MAX_OBJECTS: u64 = 1000u64;
const MAX_QUADS: u64 = 100000u64;
const MAX_DEBUG_UI: u64 = 100u64;
const LARGE_BUFFER_SIZE: u32 = 16000000; // 128mb

const DEFERRED_POSITION_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const DEFERRED_NORMAL_FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;
const DEFERRED_COLOR_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

/// The renderer for the GameEngine.
/// Used to draw objects using the GPU.
pub struct Renderer {
    device: Arc<GraphicsDevice>,
    render_targets: RenderTargets,
    descriptor_layout_cache: DescriptorLayoutCache,
    descriptor_allocator: DescriptorAllocator,
    frame_descriptor_allocator: [DescriptorAllocator; FRAMES_IN_FLIGHT],
    pipeline_layout_cache: PipelineLayoutCache,
    pipeline_manager: PipelineManager,
    timestamps: TimeStamp,

    vertex_buffer: BufferHandle,
    vertex_buffer_offset: usize,
    index_buffer: BufferHandle,
    index_buffer_offset: usize,

    shadow_pso: PipelineHandle,
    directional_light_shadow_image: RenderTargetHandle,

    depth_image: RenderTargetHandle,
    forward: ForwardPass,
    deferred_fill: DeferredPass,
    deferred_lighting_combine: DeferredLightingCombinePass,
    bright_extracted_image: RenderTargetHandle,

    bloom_pass: BloomPass,
    combine_pso: PipelineHandle,
    combine_pso_layout: vk::PipelineLayout,
    world_debug_pso: PipelineHandle,
    world_debug_pso_layout: vk::PipelineLayout,
    world_debug_desc_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    world_debug_draw_data: [BufferHandle; FRAMES_IN_FLIGHT],

    meshes: SlotMap<MeshHandle, RenderMesh>,
    render_models: SlotMap<RenderModelHandle, RenderModel>,
    descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    camera_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    camera_uniform: CameraUniform,
    light_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    stored_lights: SlotMap<LightHandle, Light>,
    transform_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    material_buffer: [BufferHandle; FRAMES_IN_FLIGHT],

    ui_pass: UiPass,
    ui_to_draw: Vec<UIMesh>,

    pub sun: DirectionalLight,
    pub draw_debug_ui: bool,
    pub debug_ui_size: f32,
    pub enable_bloom_pass: bool,
    pub light_texture: Option<ImageHandle>,
    pub clear_colour: Colour,
}

impl Renderer {
    pub fn new(window: &Window) -> Result<Self> {
        profiling::scope!("Renderer::new");

        let device = Arc::new(GraphicsDevice::new(window)?);
        let mut render_targets = RenderTargets::new(device.clone());
        let mut pipeline_manager = PipelineManager::new(device.clone());

        let mut descriptor_layout_cache = DescriptorLayoutCache::new(device.vk_device.clone());
        let mut descriptor_allocator = DescriptorAllocator::new(device.vk_device.clone());
        let mut frame_descriptor_allocator = [
            DescriptorAllocator::new(device.vk_device.clone()),
            DescriptorAllocator::new(device.vk_device.clone()),
        ];
        let mut pipeline_layout_cache = PipelineLayoutCache::new(device.vk_device.clone());

        let swapchain_image_format = vk::Format::B8G8R8A8_SRGB;
        let depth_image_format = vk::Format::D32_SFLOAT;
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
        let render_image_format = vk::Format::R8G8B8A8_SRGB;
        let forward_image = render_targets.create_render_target(
            render_image_format,
            RenderTargetSize::Fullscreen,
            RenderImageType::Colour,
        )?;
        let bright_extracted_image = render_targets.create_render_target(
            render_image_format,
            RenderTargetSize::Fullscreen,
            RenderImageType::Colour,
        )?;

        let bloom_pass = {
            let bloom_image = [
                render_targets.create_render_target(
                    render_image_format,
                    RenderTargetSize::Fullscreen,
                    RenderImageType::Colour,
                )?,
                render_targets.create_render_target(
                    render_image_format,
                    RenderTargetSize::Fullscreen,
                    RenderImageType::Colour,
                )?,
            ];

            let bloom_set_layout = DescriptorLayoutBuilder::new(&mut descriptor_layout_cache)
                .bind_image(
                    0,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                )
                .build()
                .unwrap();

            let (bloom_pso, bloom_pso_layout) = {
                let pso_layout = pipeline_layout_cache.create_pipeline_layout(
                    &[bloom_set_layout],
                    &[*vk::PushConstantRange::builder()
                        .size(size_of::<i32>() as u32)
                        .stage_flags(vk::ShaderStageFlags::FRAGMENT)],
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
                    vertex_shader: "assets/shaders/quad.vert".to_string(),
                    fragment_shader: "assets/shaders/blur.frag".to_string(),
                    vertex_input_state: Vertex::get_ui_vertex_input_desc(),
                    color_attachment_formats: vec![PipelineColorAttachment {
                        format: render_image_format,
                        blend: false,
                        ..Default::default()
                    }],
                    depth_attachment_format: None,
                    depth_stencil_state: *depth_stencil_state,
                    cull_mode: vk::CullModeFlags::NONE,
                };

                let pso = pipeline_manager.create_pipeline(&pso_build_info)?;
                (pso, pso_layout)
            };

            BloomPass {
                bloom_image,
                bloom_pso,
                bloom_pso_layout,
            }
        };

        let combine_set_layout = DescriptorLayoutBuilder::new(&mut descriptor_layout_cache)
            .bind_image(
                0,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            )
            .bind_image(
                1,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            )
            .build()
            .unwrap();

        let (combine_pso, combine_pso_layout) = {
            let pso_layout =
                pipeline_layout_cache.create_pipeline_layout(&[combine_set_layout], &[])?;

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
                vertex_shader: "assets/shaders/quad.vert".to_string(),
                fragment_shader: "assets/shaders/combine.frag".to_string(),
                vertex_input_state: Vertex::get_ui_vertex_input_desc(),
                color_attachment_formats: vec![PipelineColorAttachment {
                    format: swapchain_image_format,
                    blend: false,
                    ..Default::default()
                }],
                depth_attachment_format: None,
                depth_stencil_state: *depth_stencil_state,
                cull_mode: vk::CullModeFlags::NONE,
            };

            let pso = pipeline_manager.create_pipeline(&pso_build_info)?;
            (pso, pso_layout)
        };

        let sun = DirectionalLight::new((0.0, -1.0, -0.1).into(), (1.0, 1.0, 1.0).into(), 200f32);
        let mut camera_uniform = {
            // Create default camera so that scene is at least rendered initially
            let camera = DefaultCamera {
                position: (-8.0, 100.0, 0.0).into(),
                direction: (1.0, 0.0, 0.0).into(),
                aspect: device.size().width as f32 / device.size().height as f32,
                fovy: 90.0,
                znear: 0.1,
                zfar: 4000.0,
            };

            let mut uniform = CameraUniform::new();
            uniform.update_proj(&camera);
            uniform.update_light(&sun);
            uniform.ambient_light = Vector4::new(1.0, 1.0, 1.0, 0.0).into();
            uniform
        };

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
                let (set, set_layout) = JBDescriptorBuilder::new(
                    &device.resource_manager,
                    &mut descriptor_layout_cache,
                    &mut descriptor_allocator,
                )
                .bind_buffer(BufferDescriptorInfo {
                    binding: 0,
                    buffer: camera_buffer[i],
                    desc_type: vk::DescriptorType::UNIFORM_BUFFER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .bind_buffer(BufferDescriptorInfo {
                    binding: 1,
                    buffer: light_buffer[i],
                    desc_type: vk::DescriptorType::UNIFORM_BUFFER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .bind_buffer(BufferDescriptorInfo {
                    binding: 2,
                    buffer: transform_buffer[i],
                    desc_type: vk::DescriptorType::STORAGE_BUFFER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .bind_buffer(BufferDescriptorInfo {
                    binding: 3,
                    buffer: material_buffer[i],
                    desc_type: vk::DescriptorType::STORAGE_BUFFER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .bind_image(ImageDescriptorInfo {
                    binding: 4,
                    image: render_targets.get(directional_light_shadow_image).unwrap(),
                    sampler: device.shadow_sampler(),
                    desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .build()
                .unwrap();

                sets[i] = set;
                layout = Some(set_layout);
            }
            (sets, layout.unwrap())
        };

        for (i, set) in descriptor_set.iter().enumerate() {
            device.set_vulkan_debug_name(
                set.as_raw(),
                ObjectType::DESCRIPTOR_SET,
                "Global Descriptor Set(1)",
            )?;

            let camera_buffer = camera_buffer.get(i).unwrap();
            let light_buffer = light_buffer.get(i).unwrap();

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

        let (forward_pass, shadow_pso) = {
            let push_constant_range = *vk::PushConstantRange::builder()
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
                .size(size_of::<PushConstants>() as u32)
                .offset(0u32);

            let pso_layout = pipeline_layout_cache.create_pipeline_layout(
                &[
                    device.bindless_descriptor_set_layout(),
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
                    color_attachment_formats: vec![
                        PipelineColorAttachment {
                            format: render_image_format,
                            blend: false,
                            ..Default::default()
                        },
                        PipelineColorAttachment {
                            format: render_image_format,
                            blend: false,
                            ..Default::default()
                        },
                    ],
                    depth_attachment_format: Some(depth_image_format),
                    depth_stencil_state: *depth_stencil_state,
                    cull_mode: vk::CullModeFlags::FRONT,
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

            (
                ForwardPass {
                    pso_layout,
                    pso,
                    forward_image,
                },
                shadow_pso,
            )
        };

        let ui_pass = {
            let vertex_data_buffer = {
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
                    size: size_of::<Index>() * MAX_QUADS as usize * 3,
                    usage: vk::BufferUsageFlags::INDEX_BUFFER,
                    storage_type: BufferStorageType::HostLocal,
                };

                [
                    device.resource_manager.create_buffer(&buffer_create_info),
                    device.resource_manager.create_buffer(&buffer_create_info),
                ]
            };

            let uniform_buffer = {
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

            let (desc_set, ui_descriptor_set_layout) = {
                let mut sets = [vk::DescriptorSet::null(); FRAMES_IN_FLIGHT];
                let mut layout = None;
                for i in 0..FRAMES_IN_FLIGHT {
                    let (set, set_layout) = JBDescriptorBuilder::new(
                        &device.resource_manager,
                        &mut descriptor_layout_cache,
                        &mut descriptor_allocator,
                    )
                    .bind_buffer(BufferDescriptorInfo {
                        binding: 0,
                        buffer: uniform_buffer[i],
                        desc_type: vk::DescriptorType::UNIFORM_BUFFER,
                        stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    })
                    .bind_buffer(BufferDescriptorInfo {
                        binding: 1,
                        buffer: vertex_data_buffer[i],
                        desc_type: vk::DescriptorType::STORAGE_BUFFER,
                        stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    })
                    .build()
                    .unwrap();

                    sets[i] = set;
                    layout = Some(set_layout);
                }
                (sets, layout.unwrap())
            };

            let (ui_pso, ui_pso_layout) = {
                let pso_layout = pipeline_layout_cache.create_pipeline_layout(
                    &[
                        device.bindless_descriptor_set_layout(),
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
                        format: swapchain_image_format,
                        blend: true,
                        src_blend_factor_color: vk::BlendFactor::ONE,
                        dst_blend_factor_color: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                    }],
                    depth_attachment_format: Some(depth_image_format),
                    depth_stencil_state: *depth_stencil_state,
                    cull_mode: vk::CullModeFlags::NONE,
                };

                let pso = pipeline_manager.create_pipeline(&pso_build_info)?;
                (pso, pso_layout)
            };

            UiPass {
                pso_layout: ui_pso_layout,
                pso: ui_pso,
                desc_set,
                vertex_data_buffer,
                index_buffer,
                uniform_buffer,
            }
        };

        let world_debug_draw_data = {
            let buffer_create_info = BufferCreateInfo {
                size: size_of::<WorldDebugUIDrawData>() * MAX_DEBUG_UI as usize,
                usage: vk::BufferUsageFlags::STORAGE_BUFFER,
                storage_type: BufferStorageType::HostLocal,
            };

            [
                device.resource_manager.create_buffer(&buffer_create_info),
                device.resource_manager.create_buffer(&buffer_create_info),
            ]
        };

        let (world_debug_desc_set, world_debug_desc_layout) = {
            let mut sets = [vk::DescriptorSet::null(); FRAMES_IN_FLIGHT];
            let mut layout = None;
            for i in 0..FRAMES_IN_FLIGHT {
                let (set, set_layout) = JBDescriptorBuilder::new(
                    &device.resource_manager,
                    &mut descriptor_layout_cache,
                    &mut descriptor_allocator,
                )
                .bind_buffer(BufferDescriptorInfo {
                    binding: 0,
                    buffer: camera_buffer[i],
                    desc_type: vk::DescriptorType::UNIFORM_BUFFER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .bind_buffer(BufferDescriptorInfo {
                    binding: 1,
                    buffer: world_debug_draw_data[i],
                    desc_type: vk::DescriptorType::STORAGE_BUFFER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .build()
                .unwrap();

                sets[i] = set;
                layout = Some(set_layout);
            }
            (sets, layout.unwrap())
        };

        let (world_debug_pso, world_debug_pso_layout) = {
            let pso_layout = pipeline_layout_cache.create_pipeline_layout(
                &[
                    device.bindless_descriptor_set_layout(),
                    world_debug_desc_layout,
                ],
                &[],
            )?;

            let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
                .depth_test_enable(true)
                .depth_write_enable(false)
                .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
                .depth_bounds_test_enable(false)
                .stencil_test_enable(false)
                .min_depth_bounds(0.0f32)
                .max_depth_bounds(1.0f32);

            let pso_build_info = PipelineCreateInfo {
                pipeline_layout: pso_layout,
                vertex_shader: "assets/shaders/diagetic_ui.vert".to_string(),
                fragment_shader: "assets/shaders/diagetic_ui.frag".to_string(),
                vertex_input_state: Vertex::get_ui_vertex_input_desc(),
                color_attachment_formats: vec![PipelineColorAttachment {
                    format: swapchain_image_format,
                    blend: true,
                    src_blend_factor_color: vk::BlendFactor::ONE,
                    dst_blend_factor_color: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                }],
                depth_attachment_format: Some(depth_image_format),
                depth_stencil_state: *depth_stencil_state,
                cull_mode: vk::CullModeFlags::NONE,
            };

            let pso = pipeline_manager.create_pipeline(&pso_build_info)?;
            (pso, pso_layout)
        };

        let vertex_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: LARGE_BUFFER_SIZE as usize,
                usage: vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER,
                storage_type: BufferStorageType::Device,
            };

            device.resource_manager.create_buffer(&buffer_create_info)
        };

        let index_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: LARGE_BUFFER_SIZE as usize,
                usage: vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::INDEX_BUFFER,
                storage_type: BufferStorageType::Device,
            };

            device.resource_manager.create_buffer(&buffer_create_info)
        };

        let deferred_fill = {
            let positions = render_targets.create_render_target(
                DEFERRED_POSITION_FORMAT,
                RenderTargetSize::Fullscreen,
                RenderImageType::Colour,
            )?;
            let normals = render_targets.create_render_target(
                DEFERRED_NORMAL_FORMAT,
                RenderTargetSize::Fullscreen,
                RenderImageType::Colour,
            )?;
            let color_specs = render_targets.create_render_target(
                DEFERRED_COLOR_FORMAT,
                RenderTargetSize::Fullscreen,
                RenderImageType::Colour,
            )?;

            let pso_layout = pipeline_layout_cache.create_pipeline_layout(
                &[
                    device.bindless_descriptor_set_layout(),
                    descriptor_set_layout,
                ],
                &[],
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
                    fragment_shader: "assets/shaders/deferred.frag".to_string(),
                    vertex_input_state: Vertex::get_vertex_input_desc(),
                    color_attachment_formats: vec![
                        PipelineColorAttachment {
                            format: DEFERRED_POSITION_FORMAT,
                            blend: false,
                            ..Default::default()
                        },
                        PipelineColorAttachment {
                            format: DEFERRED_NORMAL_FORMAT,
                            blend: false,
                            ..Default::default()
                        },
                        PipelineColorAttachment {
                            format: DEFERRED_COLOR_FORMAT,
                            blend: false,
                            ..Default::default()
                        },
                    ],
                    depth_attachment_format: Some(depth_image_format),
                    depth_stencil_state: *depth_stencil_state,
                    cull_mode: vk::CullModeFlags::FRONT,
                };

                pipeline_manager.create_pipeline(&pso_build_info)?
            };

            DeferredPass {
                positions,
                normals,
                color_specs,
                pso,
                pso_layout,
            }
        };

        let deferred_lighting_combine = {
            let deferred_lighting_desc_layout =
                DescriptorLayoutBuilder::new(&mut descriptor_layout_cache)
                    .bind_image(
                        0,
                        vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        vk::ShaderStageFlags::FRAGMENT,
                    )
                    .bind_image(
                        1,
                        vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        vk::ShaderStageFlags::FRAGMENT,
                    )
                    .bind_image(
                        2,
                        vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        vk::ShaderStageFlags::FRAGMENT,
                    )
                    .build()
                    .unwrap();

            let pso_layout = pipeline_layout_cache.create_pipeline_layout(
                &[
                    device.bindless_descriptor_set_layout(),
                    descriptor_set_layout,
                    deferred_lighting_desc_layout,
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
                vertex_shader: "assets/shaders/deferred_lighting.vert".to_string(),
                fragment_shader: "assets/shaders/deferred_lighting.frag".to_string(),
                vertex_input_state: Vertex::get_ui_vertex_input_desc(),
                color_attachment_formats: vec![
                    PipelineColorAttachment {
                        format: render_image_format,
                        blend: false,
                        ..Default::default()
                    },
                    PipelineColorAttachment {
                        format: render_image_format,
                        blend: false,
                        ..Default::default()
                    },
                ],
                depth_attachment_format: None,
                depth_stencil_state: *depth_stencil_state,
                cull_mode: vk::CullModeFlags::NONE,
            };

            let pso = pipeline_manager.create_pipeline(&pso_build_info)?;

            DeferredLightingCombinePass { pso, pso_layout }
        };

        info!("Renderer Created");
        let result = Ok(Self {
            device,
            camera_buffer,
            camera_uniform,
            descriptor_set,
            clear_colour: Colour::black(),
            pipeline_manager,
            meshes: SlotMap::default(),
            render_models: SlotMap::default(),
            light_buffer,
            transform_buffer,
            material_buffer,
            light_texture: None,
            stored_lights: SlotMap::default(),
            shadow_pso,
            sun,
            ui_pass,
            ui_to_draw: Vec::new(),
            depth_image,
            directional_light_shadow_image,
            render_targets,
            descriptor_layout_cache,
            descriptor_allocator,
            timestamps: TimeStamp::default(),
            pipeline_layout_cache,
            bright_extracted_image,
            bloom_pass,
            frame_descriptor_allocator,
            combine_pso,
            combine_pso_layout,
            enable_bloom_pass: true,
            world_debug_pso,
            world_debug_pso_layout,
            draw_debug_ui: true,
            world_debug_desc_set,
            world_debug_draw_data,
            debug_ui_size: 2.5f32,
            vertex_buffer,
            vertex_buffer_offset: 0usize,
            index_buffer,
            index_buffer_offset: 0usize,
            forward: forward_pass,
            deferred_fill,
            deferred_lighting_combine,
        });
        result
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) -> Result<()> {
        if self.device.resize(new_size)? {
            self.render_targets.recreate_render_targets()?;

            JBDescriptorBuilder::new(
                &self.device.resource_manager,
                &mut self.descriptor_layout_cache,
                &mut self.descriptor_allocator,
            )
            .bind_image(ImageDescriptorInfo {
                binding: 4,
                image: self
                    .render_targets
                    .get(self.directional_light_shadow_image)
                    .unwrap(),
                sampler: self.device.shadow_sampler(),
                desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            })
            .update(&self.descriptor_set)
            .unwrap();
        }

        Ok(())
    }

    pub fn reload_shaders(&mut self) -> Result<()> {
        profiling::scope!("Reload shaders");
        self.pipeline_manager.reload_shaders(&self.device);
        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        profiling::scope!("Render Frame");

        self.device.start_frame()?;

        let resource_index = self.device.buffered_resource_number();

        // Reset desc allocator
        self.frame_descriptor_allocator[resource_index].reset_pools();

        // Get images

        let forward_image = self.render_targets.get(self.forward.forward_image).unwrap();
        let bright_extracted_image = self
            .render_targets
            .get(self.bright_extracted_image)
            .unwrap();
        let depth_image = self.render_targets.get(self.depth_image).unwrap();
        let shadow_image = self
            .render_targets
            .get(self.directional_light_shadow_image)
            .unwrap();
        let bloom_image = [
            self.render_targets
                .get(self.bloom_pass.bloom_image[0])
                .unwrap(),
            self.render_targets
                .get(self.bloom_pass.bloom_image[1])
                .unwrap(),
        ];

        let deferred_positions = self
            .render_targets
            .get(self.deferred_fill.positions)
            .unwrap();
        let deferred_normals = self.render_targets.get(self.deferred_fill.normals).unwrap();
        let deferred_color_specs = self
            .render_targets
            .get(self.deferred_fill.color_specs)
            .unwrap();

        // Copy gpu data
        {
            self.camera_uniform.update_light(&self.sun);

            self.device
                .resource_manager
                .get_buffer(self.camera_buffer[resource_index])
                .unwrap()
                .view()
                .mapped_slice()?
                .copy_from_slice(&[self.camera_uniform]);

            let test = self.stored_lights.values();
            let uniforms: Vec<LightUniform> =
                test.map(|&light| LightUniform::from(light)).collect();

            self.device
                .resource_manager
                .get_buffer(self.light_buffer[resource_index])
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

            self.device
                .resource_manager
                .get_buffer(self.transform_buffer[resource_index])
                .unwrap()
                .view_custom(0, transform_matrices.len())?
                .mapped_slice()?
                .copy_from_slice(&transform_matrices);

            let mut materials = Vec::new();
            for model in self.render_models.values() {
                let material_params =
                    self.get_material_ssbo_from_instance(&model.material_instance);
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
                .get_buffer(self.material_buffer[resource_index])
                .unwrap()
                .view_custom(0, materials.len())?
                .mapped_slice()?
                .copy_from_slice(&materials);
        }

        // Fill draw commands
        let draw_data = {
            let mut draw_data = Vec::new();
            for (i, model) in self.render_models.keys().enumerate() {
                let model = self.render_models.get(model).unwrap();
                if let Some(mesh) = self.meshes.get(model.mesh_handle) {
                    draw_data.push(DrawData {
                        vertex_offset: mesh.vertex_offset,
                        vertex_count: mesh.vertex_count,
                        index_offset: mesh.index_offset,
                        index_count: mesh.index_count,
                        transform_index: i,
                        material_index: i,
                    });
                }
            }
            draw_data
        };

        // Copy debug UI
        let debug_ui_draw_amount = {
            if self.draw_debug_ui {
                let mut debug_ui_draw_data = Vec::new();
                if let Some(texture) = self.light_texture {
                    for light in self.stored_lights.values() {
                        let draw = WorldDebugUIDrawData {
                            position: light.position.into(),
                            texture_index: self.device.get_descriptor_index(&texture).unwrap()
                                as i32,
                            colour: light.colour.into(),
                            size: self.debug_ui_size,
                        };
                        debug_ui_draw_data.push(draw);
                    }
                }

                self.device
                    .resource_manager
                    .get_buffer(self.world_debug_draw_data[resource_index])
                    .unwrap()
                    .view_custom(0, debug_ui_draw_data.len())?
                    .mapped_slice()?
                    .copy_from_slice(&debug_ui_draw_data);

                debug_ui_draw_data.len()
            } else {
                0usize
            }
        };

        // Copy UI
        {
            let ui_uniform = UIUniformData {
                screen_size: [
                    self.device.size().width as f32,
                    self.device.size().height as f32,
                ],
            };
            self.device
                .resource_manager
                .get_buffer(self.ui_pass.uniform_buffer[resource_index])
                .unwrap()
                .view()
                .mapped_slice()?
                .copy_from_slice(&[ui_uniform]);
        }

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
                    .get_buffer(self.ui_pass.vertex_data_buffer[resource_index])
                    .unwrap()
                    .view_custom(vertex_offset, verts.len())?
                    .mapped_slice()?
                    .copy_from_slice(&verts);

                self.device
                    .resource_manager
                    .get_buffer(self.ui_pass.index_buffer[resource_index])
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

        // Barrier images

        // Shadow pass
        let shadow_pass_start = self.device.write_timestamp(
            self.device.graphics_command_buffer(),
            vk::PipelineStageFlags2::TOP_OF_PIPE,
        );
        {
            ImageBarrierBuilder::default()
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(shadow_image),
                    dst_stage_mask: PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                    dst_access_mask: AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    ..Default::default()
                })
                .build(&self.device, &self.device.graphics_command_buffer())?;

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
                    &self.device.graphics_command_buffer(),
                    |_render_pass| {
                        profiling::scope!("Shadow Pass");

                        let pipeline = self.pipeline_manager.get_pipeline(self.shadow_pso);
                        unsafe {
                            self.device.vk_device.cmd_bind_pipeline(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline,
                            );
                            self.device.vk_device.cmd_bind_descriptor_sets(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                self.forward.pso_layout,
                                0u32,
                                &[
                                    self.device.bindless_descriptor_set(),
                                    self.descriptor_set[resource_index],
                                ],
                                &[],
                            );
                        };

                        // Draw commands
                        self.draw_objects(&draw_data)?;
                        Ok(())
                    },
                )?;
        }
        let shadow_pass_end = self.device.write_timestamp(
            self.device.graphics_command_buffer(),
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
        );

        // Deferred pass
        {
            ImageBarrierBuilder::default()
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(deferred_positions),
                    dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    ..Default::default()
                })
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(deferred_normals),
                    dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    ..Default::default()
                })
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(deferred_color_specs),
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
                .build(&self.device, &self.device.graphics_command_buffer())?;

            RenderPassBuilder::new((self.device.size().width, self.device.size().height))
                .add_colour_attachment(AttachmentInfo {
                    target: AttachmentHandle::Image(deferred_positions),
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    },
                    ..Default::default()
                })
                .add_colour_attachment(AttachmentInfo {
                    target: AttachmentHandle::Image(deferred_normals),
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    },
                    ..Default::default()
                })
                .add_colour_attachment(AttachmentInfo {
                    target: AttachmentHandle::Image(deferred_color_specs),
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
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
                    &self.device.graphics_command_buffer(),
                    |_render_pass| {
                        profiling::scope!("Deferred Pass");
                        let pipeline = self.pipeline_manager.get_pipeline(self.deferred_fill.pso);

                        unsafe {
                            self.device.vk_device.cmd_bind_pipeline(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline,
                            );
                            self.device.vk_device.cmd_bind_descriptor_sets(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                self.deferred_fill.pso_layout,
                                0u32,
                                &[
                                    self.device.bindless_descriptor_set(),
                                    self.descriptor_set[resource_index],
                                ],
                                &[],
                            );
                        };

                        // Draw commands

                        self.draw_objects(&draw_data)?;
                        Ok(())
                    },
                )?;
        }
        // Deferred Lighting Pass
        {
            let (render_target_set, _) = JBDescriptorBuilder::new(
                &self.device.resource_manager,
                &mut self.descriptor_layout_cache,
                &mut self.frame_descriptor_allocator[resource_index],
            )
            .bind_image(ImageDescriptorInfo {
                binding: 0,
                image: deferred_positions,
                sampler: self.device.ui_sampler(),
                desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
            })
            .bind_image(ImageDescriptorInfo {
                binding: 1,
                image: deferred_normals,
                sampler: self.device.ui_sampler(),
                desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
            })
            .bind_image(ImageDescriptorInfo {
                binding: 2,
                image: deferred_color_specs,
                sampler: self.device.ui_sampler(),
                desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
            })
            .build()
            .unwrap();

            ImageBarrierBuilder::default()
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(deferred_positions),
                    src_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    src_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                    dst_access_mask: AccessFlags2::SHADER_READ,
                    old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ..Default::default()
                })
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(deferred_normals),
                    src_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    src_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                    dst_access_mask: AccessFlags2::SHADER_READ,
                    old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ..Default::default()
                })
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(deferred_color_specs),
                    src_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    src_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                    dst_access_mask: AccessFlags2::SHADER_READ,
                    old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ..Default::default()
                })
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(
                        self.render_targets
                            .get(self.directional_light_shadow_image)
                            .unwrap(),
                    ),
                    src_stage_mask: PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    src_access_mask: AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                    dst_access_mask: AccessFlags2::SHADER_READ,
                    old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ..Default::default()
                })
                .build(&self.device, &self.device.graphics_command_buffer())?;

            RenderPassBuilder::new((self.device.size().width, self.device.size().height))
                .add_colour_attachment(AttachmentInfo {
                    target: AttachmentHandle::Image(forward_image),
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    },
                    ..Default::default()
                })
                .add_colour_attachment(AttachmentInfo {
                    target: AttachmentHandle::Image(bright_extracted_image),
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    },
                    ..Default::default()
                })
                .start(
                    &self.device,
                    &self.device.graphics_command_buffer(),
                    |_render_pass| {
                        profiling::scope!("Combine Deferred Pass");

                        let pipeline = self
                            .pipeline_manager
                            .get_pipeline(self.deferred_lighting_combine.pso);

                        unsafe {
                            self.device.vk_device.cmd_bind_pipeline(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline,
                            );
                            self.device.vk_device.cmd_bind_descriptor_sets(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                self.deferred_lighting_combine.pso_layout,
                                0u32,
                                &[
                                    self.device.bindless_descriptor_set(),
                                    self.descriptor_set[resource_index],
                                    render_target_set,
                                ],
                                &[],
                            );
                        };

                        // Draw commands

                        unsafe {
                            self.device.vk_device.cmd_draw(
                                self.device.graphics_command_buffer(),
                                6u32,
                                1u32,
                                0u32,
                                0u32,
                            );
                        };

                        Ok(())
                    },
                )?;
        }

        ImageBarrierBuilder::default()
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::Image(forward_image),
                dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                ..Default::default()
            })
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::Image(bright_extracted_image),
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
            .build(&self.device, &self.device.graphics_command_buffer())?;

        // Normal Pass
        //{
        //    let clear_colour: Vector3<f32> = self.clear_colour.into();
        //    RenderPassBuilder::new((self.device.size().width, self.device.size().height))
        //        .add_colour_attachment(AttachmentInfo {
        //            target: AttachmentHandle::Image(forward_image),
        //            clear_value: vk::ClearValue {
        //                color: vk::ClearColorValue {
        //                    float32: clear_colour.extend(0f32).into(),
        //                },
        //            },
        //            ..Default::default()
        //        })
        //        .add_colour_attachment(AttachmentInfo {
        //            target: AttachmentHandle::Image(bright_extracted_image),
        //            clear_value: vk::ClearValue {
        //                color: vk::ClearColorValue {
        //                    float32: [0.0, 0.0, 0.0, 1.0],
        //                },
        //            },
        //            ..Default::default()
        //        })
        //        .set_depth_attachment(AttachmentInfo {
        //            target: AttachmentHandle::Image(depth_image),
        //            clear_value: vk::ClearValue {
        //                depth_stencil: ClearDepthStencilValue {
        //                    depth: 1.0,
        //                    stencil: 0,
        //                },
        //            },
        //            ..Default::default()
        //        })
        //        .start(
        //            &self.device,
        //            &self.device.graphics_command_buffer(),
        //            |_render_pass| {
        //                profiling::scope!("Forward Pass");
        //                let pipeline = self.pipeline_manager.get_pipeline(self.pso);
        //
        //                unsafe {
        //                    self.device.vk_device.cmd_bind_pipeline(
        //                        self.device.graphics_command_buffer(),
        //                        vk::PipelineBindPoint::GRAPHICS,
        //                        pipeline,
        //                    );
        //                    self.device.vk_device.cmd_bind_descriptor_sets(
        //                        self.device.graphics_command_buffer(),
        //                        vk::PipelineBindPoint::GRAPHICS,
        //                        self.pso_layout,
        //                        0u32,
        //                        &[
        //                            self.device.bindless_descriptor_set(),
        //                            self.descriptor_set[resource_index],
        //                        ],
        //                        &[],
        //                    );
        //                };
        //
        //                // Draw commands
        //
        //                self.draw_objects(&draw_data)?;
        //                Ok(())
        //            },
        //        )?;
        //}
        let forward_pass_end = self.device.write_timestamp(
            self.device.graphics_command_buffer(),
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
        );

        // Bloom pass
        {
            if self.enable_bloom_pass {
                let (first_bloom_set, _) = JBDescriptorBuilder::new(
                    &self.device.resource_manager,
                    &mut self.descriptor_layout_cache,
                    &mut self.frame_descriptor_allocator[resource_index],
                )
                .bind_image(ImageDescriptorInfo {
                    binding: 0,
                    image: bright_extracted_image,
                    sampler: self.device.ui_sampler(),
                    desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .build()
                .unwrap();
                let (bloom_set, _) = JBDescriptorBuilder::new(
                    &self.device.resource_manager,
                    &mut self.descriptor_layout_cache,
                    &mut self.frame_descriptor_allocator[resource_index],
                )
                .bind_image(ImageDescriptorInfo {
                    binding: 0,
                    image: bloom_image[0],
                    sampler: self.device.ui_sampler(),
                    desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .build()
                .unwrap();
                let (bloom_set_two, _) = JBDescriptorBuilder::new(
                    &self.device.resource_manager,
                    &mut self.descriptor_layout_cache,
                    &mut self.frame_descriptor_allocator[resource_index],
                )
                .bind_image(ImageDescriptorInfo {
                    binding: 0,
                    image: bloom_image[1],
                    sampler: self.device.ui_sampler(),
                    desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                })
                .build()
                .unwrap();
                let bloom_sets = [bloom_set, bloom_set_two];

                let mut horizontal = true;
                for i in 0..10 {
                    if i == 0 {
                        ImageBarrierBuilder::default()
                            .add_image_barrier(ImageBarrier {
                                image: ImageHandleType::Image(bright_extracted_image),
                                src_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                src_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                                dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                                dst_access_mask: AccessFlags2::SHADER_READ,
                                old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                                new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                ..Default::default()
                            })
                            .add_image_barrier(ImageBarrier {
                                image: ImageHandleType::Image(bloom_image[1]),
                                dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                                new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                                ..Default::default()
                            })
                            .add_image_barrier(ImageBarrier {
                                image: ImageHandleType::Image(bloom_image[0]),
                                dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                                dst_access_mask: AccessFlags2::SHADER_READ,
                                new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                ..Default::default()
                            })
                            .build(&self.device, &self.device.graphics_command_buffer())?;
                    } else {
                        ImageBarrierBuilder::default()
                            .add_image_barrier(ImageBarrier {
                                image: ImageHandleType::Image(bloom_image[horizontal as usize]),
                                src_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                                src_access_mask: AccessFlags2::SHADER_READ,
                                dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                                old_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                                ..Default::default()
                            })
                            .add_image_barrier(ImageBarrier {
                                image: ImageHandleType::Image(bloom_image[!horizontal as usize]),
                                src_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                src_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                                dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                                dst_access_mask: AccessFlags2::SHADER_READ,
                                old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                                new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                ..Default::default()
                            })
                            .build(&self.device, &self.device.graphics_command_buffer())?;
                    }

                    RenderPassBuilder::new((self.device.size().width, self.device.size().height))
                        .add_colour_attachment(AttachmentInfo {
                            target: AttachmentHandle::Image(bloom_image[horizontal as usize]),
                            clear_value: vk::ClearValue {
                                color: vk::ClearColorValue {
                                    float32: [0.0, 0.0, 0.0, 1.0],
                                },
                            },
                            ..Default::default()
                        })
                        .start(
                            &self.device,
                            &self.device.graphics_command_buffer(),
                            |_render_pass| {
                                profiling::scope!("Bloom Pass");

                                let pipeline = self
                                    .pipeline_manager
                                    .get_pipeline(self.bloom_pass.bloom_pso);

                                let set = {
                                    if i == 0 {
                                        first_bloom_set
                                    } else {
                                        bloom_sets[!horizontal as usize]
                                    }
                                };
                                unsafe {
                                    self.device.vk_device.cmd_bind_pipeline(
                                        self.device.graphics_command_buffer(),
                                        vk::PipelineBindPoint::GRAPHICS,
                                        pipeline,
                                    );
                                    self.device.vk_device.cmd_bind_descriptor_sets(
                                        self.device.graphics_command_buffer(),
                                        vk::PipelineBindPoint::GRAPHICS,
                                        self.bloom_pass.bloom_pso_layout,
                                        0u32,
                                        &[set],
                                        &[],
                                    );
                                };

                                // Draw commands

                                unsafe {
                                    self.device.vk_device.cmd_push_constants(
                                        self.device.graphics_command_buffer(),
                                        self.bloom_pass.bloom_pso_layout,
                                        vk::ShaderStageFlags::FRAGMENT,
                                        0u32,
                                        bytemuck::cast_slice(&[horizontal as i32]),
                                    );
                                    self.device.vk_device.cmd_draw(
                                        self.device.graphics_command_buffer(),
                                        6u32,
                                        1u32,
                                        0u32,
                                        0u32,
                                    );
                                };

                                Ok(())
                            },
                        )?;
                    horizontal = !horizontal;
                }
            } else {
                ImageBarrierBuilder::default()
                    .add_image_barrier(ImageBarrier {
                        image: ImageHandleType::Image(bloom_image[0]),
                        dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                        dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                        ..Default::default()
                    })
                    .build(&self.device, &self.device.graphics_command_buffer())?;

                RenderPassBuilder::new((self.device.size().width, self.device.size().height))
                    .add_colour_attachment(AttachmentInfo {
                        target: AttachmentHandle::Image(bloom_image[0]),
                        clear_value: vk::ClearValue {
                            color: vk::ClearColorValue {
                                float32: [0.0, 0.0, 0.0, 1.0],
                            },
                        },
                        ..Default::default()
                    })
                    .start(
                        &self.device,
                        &self.device.graphics_command_buffer(),
                        |_render_pass| {
                            profiling::scope!("Bloom Pass");
                            Ok(())
                        },
                    )?;
            }
        }
        let bloom_pass_end = self.device.write_timestamp(
            self.device.graphics_command_buffer(),
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
        );

        // Combine Pass
        {
            ImageBarrierBuilder::default()
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::SwapchainImage,
                    dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    ..Default::default()
                })
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(forward_image),
                    src_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    src_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                    dst_access_mask: AccessFlags2::SHADER_READ,
                    old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ..Default::default()
                })
                .add_image_barrier(ImageBarrier {
                    image: ImageHandleType::Image(bloom_image[0]),
                    src_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    src_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                    dst_access_mask: AccessFlags2::SHADER_READ,
                    old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ..Default::default()
                })
                .build(&self.device, &self.device.graphics_command_buffer())?;
            let (combine_set, _) = JBDescriptorBuilder::new(
                &self.device.resource_manager,
                &mut self.descriptor_layout_cache,
                &mut self.frame_descriptor_allocator[resource_index],
            )
            .bind_image(ImageDescriptorInfo {
                binding: 0,
                image: forward_image,
                sampler: self.device.ui_sampler(),
                desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            })
            .bind_image(ImageDescriptorInfo {
                binding: 1,
                image: bloom_image[0],
                sampler: self.device.ui_sampler(),
                desc_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            })
            .build()
            .unwrap();

            RenderPassBuilder::new((self.device.size().width, self.device.size().height))
                .add_colour_attachment(AttachmentInfo {
                    target: AttachmentHandle::SwapchainImage,
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 1.0],
                        },
                    },
                    ..Default::default()
                })
                .start(
                    &self.device,
                    &self.device.graphics_command_buffer(),
                    |_render_pass| {
                        profiling::scope!("Combine Pass");

                        let pipeline = self.pipeline_manager.get_pipeline(self.combine_pso);

                        unsafe {
                            self.device.vk_device.cmd_bind_pipeline(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline,
                            );
                            self.device.vk_device.cmd_bind_descriptor_sets(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                self.combine_pso_layout,
                                0u32,
                                &[combine_set],
                                &[],
                            );
                        };

                        // Draw commands

                        unsafe {
                            self.device.vk_device.cmd_draw(
                                self.device.graphics_command_buffer(),
                                6u32,
                                1u32,
                                0u32,
                                0u32,
                            );
                        };

                        Ok(())
                    },
                )?;
        }
        let combine_pass_end = self.device.write_timestamp(
            self.device.graphics_command_buffer(),
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
        );

        // UI Pass
        {
            RenderPassBuilder::new((self.device.size().width, self.device.size().height))
                .add_colour_attachment(AttachmentInfo {
                    target: AttachmentHandle::SwapchainImage,
                    clear_value: vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 0.0],
                        },
                    },
                    load_op: vk::AttachmentLoadOp::LOAD,
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
                    load_op: vk::AttachmentLoadOp::LOAD,
                    ..Default::default()
                })
                .start(
                    &self.device,
                    &self.device.graphics_command_buffer(),
                    |render_pass| {
                        profiling::scope!("UI Pass");

                        if self.draw_debug_ui {
                            let pipeline = self.pipeline_manager.get_pipeline(self.world_debug_pso);

                            unsafe {
                                self.device.vk_device.cmd_bind_pipeline(
                                    self.device.graphics_command_buffer(),
                                    vk::PipelineBindPoint::GRAPHICS,
                                    pipeline,
                                );
                                self.device.vk_device.cmd_bind_descriptor_sets(
                                    self.device.graphics_command_buffer(),
                                    vk::PipelineBindPoint::GRAPHICS,
                                    self.world_debug_pso_layout,
                                    0u32,
                                    &[
                                        self.device.bindless_descriptor_set(),
                                        self.world_debug_desc_set[resource_index],
                                    ],
                                    &[],
                                );
                            };

                            unsafe {
                                self.device.vk_device.cmd_draw(
                                    self.device.graphics_command_buffer(),
                                    6u32 * debug_ui_draw_amount as u32,
                                    1u32,
                                    0u32,
                                    0u32,
                                );
                            };
                        }

                        let pipeline = self.pipeline_manager.get_pipeline(self.ui_pass.pso);

                        unsafe {
                            self.device.vk_device.cmd_bind_pipeline(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline,
                            );
                            self.device.vk_device.cmd_bind_descriptor_sets(
                                self.device.graphics_command_buffer(),
                                vk::PipelineBindPoint::GRAPHICS,
                                self.ui_pass.pso_layout,
                                0u32,
                                &[
                                    self.device.bindless_descriptor_set(),
                                    self.ui_pass.desc_set[resource_index],
                                ],
                                &[],
                            );
                        };

                        let index_buffer = self
                            .device
                            .resource_manager
                            .get_buffer(self.ui_pass.index_buffer[resource_index])
                            .unwrap();

                        unsafe {
                            self.device.vk_device.cmd_bind_index_buffer(
                                self.device.graphics_command_buffer(),
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
                                    self.device.graphics_command_buffer(),
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
        }
        let ui_pass_end = self.device.write_timestamp(
            self.device.graphics_command_buffer(),
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
        );

        // Transition render image to transfer src

        ImageBarrierBuilder::default()
            .add_image_barrier(ImageBarrier {
                image: ImageHandleType::SwapchainImage,
                src_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                src_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                old_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                new_layout: ImageLayout::PRESENT_SRC_KHR,
                ..Default::default()
            })
            .build(&self.device, &self.device.graphics_command_buffer())?;

        self.device.end_frame()?;

        if let Some(time) = self
            .device
            .get_timestamp_result(shadow_pass_start, shadow_pass_end)
        {
            self.timestamps.shadow_pass = time;
        }
        if let Some(time) = self
            .device
            .get_timestamp_result(shadow_pass_end, forward_pass_end)
        {
            self.timestamps.forward_pass = time;
        }
        if let Some(time) = self
            .device
            .get_timestamp_result(forward_pass_end, bloom_pass_end)
        {
            self.timestamps.bloom_pass = time;
        }
        if let Some(time) = self
            .device
            .get_timestamp_result(bloom_pass_end, combine_pass_end)
        {
            self.timestamps.combine_pass = time;
        }
        if let Some(time) = self
            .device
            .get_timestamp_result(combine_pass_end, ui_pass_end)
        {
            self.timestamps.ui_pass = time;
        }
        if let Some(time) = self
            .device
            .get_timestamp_result(shadow_pass_start, ui_pass_end)
        {
            self.timestamps.total = time;
        }

        Ok(())
    }

    fn draw_objects(&self, draws: &[DrawData]) -> Result<()> {
        let vertex_buffer = self
            .device
            .resource_manager
            .get_buffer(self.vertex_buffer)
            .unwrap()
            .buffer();
        unsafe {
            self.device.vk_device.cmd_bind_vertex_buffers(
                self.device.graphics_command_buffer(),
                0u32,
                &[vertex_buffer],
                &[0u64],
            )
        };

        let index_buffer = self
            .device
            .resource_manager
            .get_buffer(self.index_buffer)
            .unwrap()
            .buffer();
        unsafe {
            self.device.vk_device.cmd_bind_index_buffer(
                self.device.graphics_command_buffer(),
                index_buffer,
                DeviceSize::zero(),
                IndexType::UINT32,
            );
        }

        for draw in draws.iter() {
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
                    self.device.graphics_command_buffer(),
                    self.forward.pso_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0u32,
                    bytemuck::cast_slice(&[push_constants]),
                )
            };

            let index_count = {
                if draw.index_count == 0 {
                    draw.vertex_count
                } else {
                    draw.index_count
                }
            };

            unsafe {
                self.device.vk_device.cmd_draw_indexed(
                    self.device.graphics_command_buffer(),
                    index_count as u32,
                    1u32,
                    draw.index_offset as u32,
                    draw.vertex_offset as i32,
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

            trace!(
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

        let vertex_buffer_offset = {
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

            let offset = self.vertex_buffer_offset;
            let buffer_offset = size_of::<Vertex>() * offset;

            assert!(
                size_of::<Vertex>() * (offset + mesh.vertices.len()) <= LARGE_BUFFER_SIZE as usize
            );

            self.device.immediate_submit(|device, cmd| {
                cmd_copy_buffer(
                    device,
                    cmd,
                    staging_buffer,
                    self.vertex_buffer,
                    buffer_offset,
                )?;
                Ok(())
            })?;

            self.vertex_buffer_offset += mesh.vertices.len();
            offset
        };
        match &mesh.indices {
            None => {
                let render_mesh = RenderMesh {
                    vertex_offset: vertex_buffer_offset,
                    vertex_count: mesh.vertices.len(),
                    index_offset: 0,
                    index_count: 0,
                };
                trace!(
                    "Mesh Loaded. Vertex Count:{}|Faces:{}",
                    mesh.vertices.len(),
                    mesh.faces.len()
                );
                Ok(self.meshes.insert(render_mesh))
            }
            Some(indices) => {
                let index_buffer_offset = {
                    let buffer_size = size_of::<Index>() * indices.len();
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

                    let offset = self.index_buffer_offset;
                    let buffer_offset = size_of::<Index>() * offset;

                    assert!(
                        size_of::<Index>() * (offset + indices.len()) <= LARGE_BUFFER_SIZE as usize
                    );

                    self.device.immediate_submit(|device, cmd| {
                        cmd_copy_buffer(
                            device,
                            cmd,
                            staging_buffer,
                            self.index_buffer,
                            buffer_offset,
                        )?;
                        Ok(())
                    })?;

                    self.index_buffer_offset += indices.len();

                    offset
                };
                let render_mesh = RenderMesh {
                    vertex_offset: vertex_buffer_offset,
                    vertex_count: mesh.vertices.len(),
                    index_offset: index_buffer_offset,
                    index_count: indices.len(),
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

    pub fn timestamps(&self) -> TimeStamp {
        self.timestamps
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

    pub fn set_camera<T: Camera>(&mut self, camera: &T) {
        self.camera_uniform.update_proj(camera);
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
            for cache in self.frame_descriptor_allocator.iter_mut() {
                cache.cleanup();
            }
            self.descriptor_layout_cache.cleanup();
            self.descriptor_allocator.cleanup();
            self.pipeline_layout_cache.cleanup();
            self.pipeline_manager.deinit();
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
    vertex_offset: usize,
    vertex_count: usize,
    index_offset: usize,
    index_count: usize,
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

#[derive(Copy, Clone)]
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
    vertex_offset: usize,
    vertex_count: usize,
    index_offset: usize,
    index_count: usize,
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

#[derive(Default, Copy, Clone)]
pub struct TimeStamp {
    pub shadow_pass: f64,
    pub forward_pass: f64,
    pub bloom_pass: f64,
    pub combine_pass: f64,
    pub ui_pass: f64,
    pub total: f64,
}

struct ForwardPass {
    pso_layout: vk::PipelineLayout,
    pso: PipelineHandle,
    forward_image: RenderTargetHandle,
}

struct DeferredPass {
    positions: RenderTargetHandle,
    normals: RenderTargetHandle,
    color_specs: RenderTargetHandle,
    pso: PipelineHandle,
    pso_layout: vk::PipelineLayout,
}

struct DeferredLightingCombinePass {
    pso: PipelineHandle,
    pso_layout: vk::PipelineLayout,
}

struct UiPass {
    pso_layout: vk::PipelineLayout,
    pso: PipelineHandle,
    desc_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    vertex_data_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    index_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
    uniform_buffer: [BufferHandle; FRAMES_IN_FLIGHT],
}

struct BloomPass {
    bloom_image: [RenderTargetHandle; 2],
    bloom_pso: PipelineHandle,
    bloom_pso_layout: vk::PipelineLayout,
}
