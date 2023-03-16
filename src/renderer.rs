use std::collections::HashMap;
use std::mem::size_of;

use ash::vk;
use ash::vk::{AccessFlags2, ImageAspectFlags, ImageLayout, PipelineStageFlags2};
use cgmath::{Matrix4, SquareMatrix, Vector2, Vector3, Zero};
use log::error;
use slotmap::{new_key_type, SlotMap};
use winit::{dpi::PhysicalSize, window::Window};

use crate::device::{Device, FRAMES_IN_FLIGHT};
use crate::pipeline::{PipelineCreateInfo, PipelineHandle, PipelineManager};
use crate::resource::{BufferHandle, ImageHandle};

const MAX_QUADS: u64 = 400000;
const MAX_CHARS: u64 = 20000;
const BINDLESS_BINDING_INDEX: u32 = 0u32;

/// The renderer for the GameEngine.
/// Used to draw objects using the GPU.
pub struct Renderer {
    device: Device,
    pso_layout: vk::PipelineLayout,
    pso: PipelineHandle,
    camera_buffer: BufferHandle,
    camera_uniform: Camera2DUniform,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    bindless_textures: Vec<ImageHandle>,
    bindless_indexes: HashMap<ImageHandle, usize>,
    pub clear_colour: Colour,
    pipeline_manager: PipelineManager,
}

impl Renderer {
    pub fn new(window: &Window) -> Self {
        let mut device = Device::new(window);

        let vertex_input_desc = Vertex::get_vertex_input_desc();

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&vertex_input_desc.bindings)
            .vertex_attribute_descriptions(&vertex_input_desc.attributes);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(false)
            .depth_write_enable(false)
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
                .descriptor_count(100u32)
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        ];

        let pool_create_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(2u32)
            .pool_sizes(&pool_sizes);

        let descriptor_pool = unsafe {
            device
                .vk_device
                .create_descriptor_pool(&pool_create_info, None)
        }
        .unwrap();

        let binding_flags = [
            vk::DescriptorBindingFlags::empty(),
            vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT
                | vk::DescriptorBindingFlags::PARTIALLY_BOUND,
        ];

        let mut descriptor_set_binding_flags_create_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::builder().binding_flags(&binding_flags);

        let descriptor_set_bindings = [
            *vk::DescriptorSetLayoutBinding::builder()
                .binding(0u32)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1u32)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            *vk::DescriptorSetLayoutBinding::builder()
                .binding(BINDLESS_BINDING_INDEX)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(10u32)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];

        let descriptor_set_layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .push_next(&mut descriptor_set_binding_flags_create_info)
            .bindings(&descriptor_set_bindings);

        let descriptor_set_layout = unsafe {
            device
                .vk_device
                .create_descriptor_set_layout(&descriptor_set_layout_create_info, None)
        }
        .unwrap();

        let layouts = [descriptor_set_layout];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder().set_layouts(&layouts);

        let pso_layout = unsafe {
            device
                .vk_device
                .create_pipeline_layout(&pipeline_layout_info, None)
        }
        .unwrap();

        let pso_build_info = PipelineCreateInfo {
            pipeline_layout: pso_layout,
            vertex_shader: "assets/shaders/default.vert".to_string(),
            fragment_shader: "assets/shaders/default.frag".to_string(),
            vertex_input_state: *vertex_input_state,
            color_attachment_formats: vec![device.render_image_format],
            depth_attachment_format: None,
            depth_stencil_state: *depth_stencil_state,
        };

        // TODO : Move more into PipelineManager
        let mut pipeline_manager = PipelineManager::new();
        let pso = pipeline_manager.create_pipeline(&mut device.vk_device, &pso_build_info);

        let mut camera_uniform = Camera2DUniform::new();
        camera_uniform.update_proj(Vector2::new(
            device.size.width as f32,
            device.size.height as f32,
        ));

        let camera_buffer = {
            let buffer_create_info = vk::BufferCreateInfo {
                size: size_of::<Camera2DUniform>() as u64,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                ..Default::default()
            };

            let buffer_allocation_create_info = vk_mem_alloc::AllocationCreateInfo {
                flags: vk_mem_alloc::AllocationCreateFlags::MAPPED
                    | vk_mem_alloc::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
                usage: vk_mem_alloc::MemoryUsage::AUTO,
                ..Default::default()
            };

            device
                .resource_manager
                .create_buffer(&buffer_create_info, &buffer_allocation_create_info)
        };

        unsafe {
            std::ptr::copy_nonoverlapping(
                &[camera_uniform],
                device
                    .resource_manager
                    .get_buffer(&camera_buffer)
                    .unwrap()
                    .allocation_info
                    .mapped_data
                    .cast(),
                size_of::<Camera2DUniform>(),
            )
        };

        let descriptor_set = {
            let mut descriptor_set_counts =
                vk::DescriptorSetVariableDescriptorCountAllocateInfo::builder()
                    .descriptor_counts(&[10u32]);

            let set_layouts = [descriptor_set_layout];
            let create_info = vk::DescriptorSetAllocateInfo::builder()
                .push_next(&mut descriptor_set_counts)
                .descriptor_pool(descriptor_pool)
                .set_layouts(&set_layouts);

            let descriptor_sets =
                unsafe { device.vk_device.allocate_descriptor_sets(&create_info) }.unwrap();
            let first = *descriptor_sets.get(0).unwrap();
            let descriptor_sets =
                unsafe { device.vk_device.allocate_descriptor_sets(&create_info) }.unwrap();
            let second = *descriptor_sets.get(0).unwrap();

            [first, second]
        };

        let camera_buffer_write = vk::DescriptorBufferInfo::builder()
            .buffer(
                device
                    .resource_manager
                    .get_buffer(&camera_buffer)
                    .unwrap()
                    .buffer,
            )
            .range(
                device
                    .resource_manager
                    .get_buffer(&camera_buffer)
                    .unwrap()
                    .allocation_info
                    .size,
            );

        let mut bindless_textures = Vec::new();
        let mut bindless_indexes = HashMap::new();

        Self {
            device,
            pso_layout,
            pso,
            camera_buffer,
            camera_uniform,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            bindless_textures,
            bindless_indexes,
            clear_colour: Colour::BLACK,
            pipeline_manager,
        }
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.device.resize(new_size);

        // TODO : Decided how to update camera when size changes
        //self.camera_uniform.update_proj(Vector2::new(
        //    self.device.size.width as f32,
        //    self.device.size.height as f32,
        //));
        //
        //unsafe {
        //    std::ptr::copy_nonoverlapping(
        //        &[self.camera_uniform],
        //        self.device
        //            .resource_manager
        //            .get_buffer(&self.camera_buffer)
        //            .unwrap()
        //            .allocation_info
        //            .mapped_data
        //            .cast(),
        //        std::mem::size_of::<Camera2DUniform>(),
        //    )
        //};
    }

    pub fn reload_shaders(&mut self) {
        self.pipeline_manager
            .reload_shaders(&mut self.device.vk_device)
    }

    pub fn render(&mut self) {
        let present_index = self.device.start_frame();

        let clear_colour: Vector3<f32> = self.clear_colour.into();
        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [clear_colour.x, clear_colour.y, clear_colour.z, 0.0],
            },
        };

        // Begin command buffer

        let cmd_begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            self.device.vk_device.begin_command_buffer(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                &cmd_begin_info,
            )
        }
        .unwrap();

        let viewport = vk::Viewport::builder()
            .x(0.0f32)
            .y(0.0f32)
            .width(self.device.size.width as f32)
            .height(self.device.size.height as f32)
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

        let present_to_dst_barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(PipelineStageFlags2::NONE)
            .src_access_mask(AccessFlags2::NONE)
            .dst_stage_mask(PipelineStageFlags2::BLIT)
            .dst_access_mask(AccessFlags2::TRANSFER_WRITE)
            .old_layout(ImageLayout::UNDEFINED)
            .new_layout(ImageLayout::TRANSFER_DST_OPTIMAL)
            .image(self.device.present_images[present_index as usize])
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let render_image_attachment_barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(PipelineStageFlags2::NONE)
            .src_access_mask(AccessFlags2::NONE)
            .dst_stage_mask(PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(ImageLayout::UNDEFINED)
            .new_layout(ImageLayout::ATTACHMENT_OPTIMAL)
            .image(
                self.device
                    .resource_manager
                    .get_image(&self.device.render_image)
                    .unwrap()
                    .image,
            )
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_memory_barriers = [*render_image_attachment_barrier, *present_to_dst_barrier];
        let graphics_barrier_dependency_info =
            vk::DependencyInfo::builder().image_memory_barriers(&image_memory_barriers);

        unsafe {
            self.device.vk_device.cmd_pipeline_barrier2(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                &graphics_barrier_dependency_info,
            )
        };

        // Start dynamic rendering

        let color_attach_info = vk::RenderingAttachmentInfo::builder()
            .image_view(
                self.device
                    .resource_manager
                    .get_image(&self.device.render_image)
                    .unwrap()
                    .default_view,
            )
            .image_layout(ImageLayout::ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value);

        let color_attachments = [*color_attach_info];
        let render_info = vk::RenderingInfo::builder()
            .render_area(*scissor)
            .layer_count(1u32)
            .color_attachments(&color_attachments);

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
                &[self.descriptor_set[self.device.buffered_resource_number()]],
                &[],
            );
        }

        unsafe {
            self.device.vk_device.cmd_end_rendering(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
            )
        };

        // Transition render image to transfer src

        let render_to_src_barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(PipelineStageFlags2::NONE)
            .dst_access_mask(AccessFlags2::NONE)
            .old_layout(ImageLayout::ATTACHMENT_OPTIMAL)
            .new_layout(ImageLayout::TRANSFER_SRC_OPTIMAL)
            .image(
                self.device
                    .resource_manager
                    .get_image(&self.device.render_image)
                    .unwrap()
                    .image,
            )
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_memory_barriers = [*render_to_src_barrier];
        let render_to_src_barrier_dependency_info =
            vk::DependencyInfo::builder().image_memory_barriers(&image_memory_barriers);

        unsafe {
            self.device.vk_device.cmd_pipeline_barrier2(
                self.device.graphics_command_buffer[self.device.buffered_resource_number()],
                &render_to_src_barrier_dependency_info,
            )
        };

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
                    .get_image(&self.device.render_image)
                    .unwrap()
                    .image,
                ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.device.present_images[present_index as usize],
                ImageLayout::TRANSFER_DST_OPTIMAL,
                &regions,
                vk::Filter::NEAREST,
            )
        }

        // Transition to present

        let present_attachment_barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(PipelineStageFlags2::TRANSFER)
            .src_access_mask(AccessFlags2::MEMORY_WRITE)
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
        }
        .unwrap();

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

        self.device.end_frame(present_index);
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
    pub fn load_texture(&mut self, file_location: &str) -> Result<Texture, String> {
        let img = image::open(file_location);

        if let Err(error) = img {
            return Err(error.to_string());
        }

        let img = img.unwrap();
        let img_bytes = img.as_bytes();
        let image = self
            .device
            .load_image(img_bytes, img.width(), img.height())
            .unwrap();

        self.bindless_textures.push(image);
        let bindless_index = self.bindless_textures.len();
        self.bindless_indexes.insert(image, bindless_index);

        let bindless_image_info = vk::DescriptorImageInfo::builder()
            .sampler(self.device.default_sampler)
            .image_view(
                self.device
                    .resource_manager
                    .get_image(&image)
                    .unwrap()
                    .default_view,
            )
            .image_layout(ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let image_info = [*bindless_image_info];
        let desc_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set[0])
            .dst_binding(BINDLESS_BINDING_INDEX)
            .dst_array_element(bindless_index as u32 - 1u32)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info);
        let desc_write_two = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set[1])
            .dst_binding(BINDLESS_BINDING_INDEX)
            .dst_array_element(bindless_index as u32 - 1u32)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info);

        unsafe {
            self.device
                .vk_device
                .update_descriptor_sets(&[*desc_write, *desc_write_two], &[]);
        }

        let texture = Texture {
            image_handle: image,
            width: img.width(),
            height: img.height(),
        };

        Ok(texture)
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

/// A vertex.
/// The different attributes are used to draw a mesh on the GPU.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {}

struct VertexInputDescription {
    bindings: Vec<vk::VertexInputBindingDescription>,
    attributes: Vec<vk::VertexInputAttributeDescription>,
}

impl Vertex {
    fn get_vertex_input_desc() -> VertexInputDescription {
        VertexInputDescription {
            bindings: vec![],
            attributes: vec![],
        }
    }
}

/// Transform Matrix that is given to the GPU
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct QuadDrawData {
    transform: [[f32; 4]; 4],
    colour: [f32; 3],
    bindless_index: u32,
    text_index: u32,
    padding: [u32; 3],
}

impl QuadDrawData {
    pub fn new(
        transform: Matrix4<f32>,
        bindless_index: u32,
        colour: Vector3<f32>,
        text_index: u32,
    ) -> Self {
        Self {
            transform: transform.into(),
            colour: colour.into(),
            bindless_index,
            text_index,
            padding: [0, 0, 0],
        }
    }
}

/// Data for drawing text
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextDrawData {
    vertices: [[f32; 2]; 4],
    tex_coords: [[f32; 2]; 4],
}

impl TextDrawData {
    pub fn new(vertices: [[f32; 2]; 4], tex_coords: [[f32; 2]; 4]) -> Self {
        Self {
            vertices,
            tex_coords,
        }
    }
}

/// The Camera Matrix that is given to the GPU.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Camera2DUniform {
    proj: [[f32; 4]; 4],
}

impl Camera2DUniform {
    fn new() -> Self {
        Self {
            proj: Matrix4::identity().into(),
        }
    }

    fn update_proj(&mut self, window_size: Vector2<f32>) {
        self.proj = {
            let proj = cgmath::ortho(
                0.0f32,
                window_size.x,
                window_size.y,
                0.0f32,
                -1.0f32,
                1.0f32,
            );

            let mut proj = proj;
            proj[1][1] *= -1f32;

            proj = proj * Matrix4::from_translation(Vector3::new(0f32, -window_size.y, 0f32));

            proj.into()
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum Colour {
    RED,
    BLUE,
    GREEN,
    WHITE,
    BLACK,
    CUSTOM(f32, f32, f32),
}

impl From<Colour> for Vector3<f32> {
    fn from(color: Colour) -> Self {
        Vector3::from(match color {
            Colour::RED => [1.0f32, 0.0f32, 0.0f32],
            Colour::BLUE => [0.0f32, 0.0f32, 1.0f32],
            Colour::GREEN => [0.0f32, 1.0f32, 0.0f32],
            Colour::WHITE => [1.0f32, 1.0f32, 1.0f32],
            Colour::BLACK => [0.0f32, 0.0f32, 0.0f32],
            Colour::CUSTOM(red, green, blue) => [red, green, blue],
        })
    }
}

/// A [`Quad`] with corresponding [`Texture`].
/// Used to tell the renderer how to draw a sprite.
#[derive(Copy, Clone)]
pub struct Sprite {
    /// The x position of the quad.
    pub x: f32,
    /// The y position of the quad.
    pub y: f32,
    /// The width of the quad.
    pub x_scale: f32,
    /// The height of the quad.
    pub y_scale: f32,
    /// The rotation of the sprite.
    pub rotation: f32,
    /// The texture that is used to render the sprite.
    pub texture: Texture,
    /// Colour to modulate the sprite
    pub colour: Colour,
}

impl Sprite {
    pub fn new(
        x: f32,
        y: f32,
        x_scale: f32,
        y_scale: f32,
        rotation: f32,
        texture: Texture,
    ) -> Self {
        Self {
            x,
            y,
            x_scale,
            y_scale,
            rotation,
            texture,
            colour: Colour::WHITE,
        }
    }
}

new_key_type! {
    /// A handle to a RenderProxy
    pub struct RenderProxyHandle;
}

struct Text {
    text: String,
    position: (f32, f32),
    pub colour: Colour,
}

fn from_transforms(position: Vector2<f32>, rotation: f32, size: Vector2<f32>) -> Matrix4<f32> {
    let translation = Matrix4::from_translation(Vector3::new(position.x, position.y, 0.0f32));
    let rotation = Matrix4::from_angle_z(cgmath::Deg(rotation));
    let scale = Matrix4::from_nonuniform_scale(size.x, size.y, 1.0f32);

    let mut model = translation;
    model = model * rotation;
    model = model * Matrix4::from_translation(Vector3::new(0.5f32, 0.5f32, 0.0f32));
    model = model * scale;
    model = model * Matrix4::from_translation(Vector3::new(-0.5f32, -0.5f32, 0.0f32));
    model
}

/// Texture, stored on the GPU.
#[derive(Copy, Clone)]
pub struct Texture {
    image_handle: ImageHandle,
    width: u32,
    height: u32,
}

impl Texture {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

impl From<Texture> for ImageHandle {
    fn from(tex: Texture) -> ImageHandle {
        tex.image_handle
    }
}

struct RenderProxy {
    texture: Option<Texture>,
    colour: Option<Colour>,
    cached_transform: Matrix4<f32>,
}

impl RenderProxy {
    fn new(texture: Option<Texture>, colour: Option<Colour>) -> Self {
        Self {
            texture,
            colour,
            cached_transform: Matrix4::zero(),
        }
    }
}
