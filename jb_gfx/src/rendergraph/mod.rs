use ash::vk;
use log::info;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use ash::vk::Handle;

use crate::rendergraph::attachment::{AttachmentInfo, SizeClass};
use crate::rendergraph::resource_tracker::{RenderPassTracker, RenderResourceTracker};
use crate::rendergraph::virtual_resource::{
    VirtualRenderPassHandle, VirtualResource, VirtualTextureResourceHandle,
};
use crate::renderpass::barrier::{ImageBarrier, ImageBarrierBuilder};
use crate::{AttachmentHandle, GraphicsDevice, ImageHandle};

pub mod attachment;
pub mod physical_resource;
pub mod resource_tracker;
pub mod virtual_resource;

pub struct RenderList {
    device: Arc<GraphicsDevice>,
    passes: RenderPassTracker,
    resource: RenderResourceTracker,
    order_of_passes: Vec<VirtualRenderPassHandle>,
    physical_passes: HashMap<VirtualRenderPassHandle, PhysicalRenderPass>,
    physical_images: HashMap<VirtualTextureResourceHandle, ImageHandle>,
    pub swapchain_size: (u32,u32)
}

impl RenderList {
    pub fn new(device: Arc<GraphicsDevice>, swapchain_size: (u32,u32)) -> Self {
        Self {
            device,
            passes: RenderPassTracker::default(),
            resource: RenderResourceTracker::default(),
            order_of_passes: Vec::default(),
            physical_passes: HashMap::default(),
            physical_images: HashMap::default(),
            swapchain_size,
        }
    }

    pub fn add_pass(
        &mut self,
        name: &str,
        pass_layout: RenderPassLayout,
    ) -> VirtualRenderPassHandle {
        let (pass_handle, render_pass) = self.passes.get_render_pass(name);
        render_pass.name = name.to_string();
        for attach in pass_layout.color_attachments {
            let (resource_handle, resource) = self.resource.get_texture_resource(&attach.0);
            resource.set_image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);
            resource.write_in_pass(pass_handle);
            resource.set_attachment_info(attach.1);
            render_pass.color_attachments.push(resource_handle);
        }
        if let Some(attach) = pass_layout.depth_attachment {
            let (resource_handle, resource) = self.resource.get_texture_resource(&attach.0);
            resource.set_image_usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);
            resource.write_in_pass(pass_handle);
            resource.set_attachment_info(attach.1);
            render_pass.depth_attachment = Some(resource_handle);
        }
        for input in pass_layout.texture_inputs {
            let (resource_handle, resource) = self.resource.get_texture_resource(&input);
            resource.set_image_usage(vk::ImageUsageFlags::SAMPLED);
            resource.read_in_pass(pass_handle);
            render_pass.texture_inputs.push(resource_handle);
        }

        render_pass.clear_colour = pass_layout.clear_colour;
        render_pass.depth_clear = pass_layout.depth_clear;
        render_pass.stencil_clear = pass_layout.stencil_clear;

        self.order_of_passes.push(pass_handle);

        pass_handle
    }

    pub fn bake(&mut self) {
        // Create physical images
        for (handle, resource) in self.resource.get_resources() {
            let size = {
                match resource.get_attachment_info().size {
                    SizeClass::SwapchainRelative => {
                        self.swapchain_size
                    }
                    SizeClass::Custom(width, height) => (width, height),
                }
            };

            let image_create_info = vk::ImageCreateInfo::builder()
                .format(resource.get_attachment_info().format)
                .usage(resource.get_image_usage())
                .extent(vk::Extent3D {
                    width: size.0,
                    height: size.1,
                    depth: 1,
                })
                .image_type(vk::ImageType::TYPE_2D)
                .array_layers(1)
                .mip_levels(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL);

            let image = self
                .device
                .resource_manager
                .create_image(&image_create_info);

            {
                let image = self
                    .device
                    .resource_manager.get_image(image).unwrap();

                self.device.set_vulkan_debug_name(image.image().as_raw(), vk::ObjectType::IMAGE, resource.name()).unwrap();
            }

            self.physical_images.insert(handle, image);
            info!("Image Created: {}", resource.name());
        }

        for &pass in self.order_of_passes.iter() {
            let mut physical_render_pass = PhysicalRenderPass::default();

            let renderpass = self.passes.retrieve_render_pass(pass);

            physical_render_pass.clear_color = vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: renderpass.clear_colour,
                },
            };
            physical_render_pass.depth_stencil_clear = vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: renderpass.depth_clear,
                    stencil: renderpass.stencil_clear,
                },
            };

            for &color in renderpass.color_attachments.iter() {
                let physical_image = self.physical_images.get(&color).unwrap();
                let physical_image_view = self
                    .device
                    .resource_manager
                    .get_image(*physical_image)
                    .unwrap()
                    .image_view();

                let physical_attachment_info = vk::RenderingAttachmentInfo {
                    image_view: physical_image_view,
                    image_layout: vk::ImageLayout::ATTACHMENT_OPTIMAL,
                    load_op: vk::AttachmentLoadOp::CLEAR, // TODO : Do this based on past usage
                    store_op: vk::AttachmentStoreOp::STORE,
                    clear_value: physical_render_pass.clear_color,
                    ..Default::default()
                };

                let resource = self.resource.retrieve_resource(color);
                let size = match resource.get_attachment_info().size {
                    SizeClass::SwapchainRelative => self.swapchain_size,
                    SizeClass::Custom(width, height) => (width, height),
                };
                let viewport = get_viewport_info(size, false);
                let scissor = vk::Rect2D::builder()
                    .offset(vk::Offset2D { x: 0, y: 0 })
                    .extent(vk::Extent2D {
                        width: size.0,
                        height: size.1,
                    });

                physical_render_pass.viewport = viewport;
                physical_render_pass.scissor = *scissor;
                physical_render_pass
                    .attachments
                    .push(physical_attachment_info);
            }
            if let Some(depth) = renderpass.depth_attachment {
                let physical_image = self.physical_images.get(&depth).unwrap();
                let physical_image_view = self
                    .device
                    .resource_manager
                    .get_image(*physical_image)
                    .unwrap()
                    .image_view();
                let physical_attachment_info = vk::RenderingAttachmentInfo {
                    image_view: physical_image_view,
                    image_layout: vk::ImageLayout::ATTACHMENT_OPTIMAL,
                    load_op: vk::AttachmentLoadOp::CLEAR, // TODO : Do this based on past usage
                    store_op: vk::AttachmentStoreOp::STORE,
                    clear_value: physical_render_pass.depth_stencil_clear,
                    ..Default::default()
                };

                let resource = self.resource.retrieve_resource(depth);
                let size = match resource.get_attachment_info().size {
                    SizeClass::SwapchainRelative => self.swapchain_size,
                    SizeClass::Custom(width, height) => (width, height),
                };
                let viewport = get_viewport_info(size, false);
                let scissor = vk::Rect2D::builder()
                    .offset(vk::Offset2D { x: 0, y: 0 })
                    .extent(vk::Extent2D {
                        width: size.0,
                        height: size.1,
                    });

                physical_render_pass.viewport = viewport;
                physical_render_pass.scissor = *scissor;
                physical_render_pass.depth_attachment = Some(physical_attachment_info);
            }

            self.physical_passes.insert(pass, physical_render_pass);
        }

        // for each renderpass, generate barriers
        for (i, virtual_pass_handle) in self.order_of_passes.iter().enumerate() {
            let renderpass = self.passes.retrieve_render_pass(*virtual_pass_handle);

            let mut barriers = Vec::new();
            for attachment in renderpass.color_attachments.iter() {
                let resource = self.resource.retrieve_resource(*attachment);

                let read_passes = resource.get_read_passes();
                let write_passes = resource.get_write_passes();

                // Get last operation that occured
                let mut last_operation = LastUsage::None;
                for j in 0..i {
                    let previous_pass = self.order_of_passes[j];
                    // Should not be able to be both write and read in same pass(for now)
                    if read_passes.contains(&previous_pass) {
                        last_operation = LastUsage::Read;
                    }
                    if write_passes.contains(&previous_pass) {
                        last_operation = LastUsage::Write;
                    }
                }

                let image = self.physical_images.get(&attachment).unwrap();
                match last_operation {
                    LastUsage::Write => { // DONT NEED TO BARRIER
                    }
                    LastUsage::Read => {
                        let barrier = ImageBarrier::new(AttachmentHandle::Image(*image))
                            .old_usage(vk::ImageUsageFlags::SAMPLED)
                            .new_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);
                        barriers.push(barrier);
                        info!("BARRIER: {},{}", resource.name(), last_operation,);
                    }
                    LastUsage::None => {
                        let barrier = ImageBarrier::new(AttachmentHandle::Image(*image))
                            .new_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);
                        barriers.push(barrier);
                        info!("BARRIER: {},{}", resource.name(), last_operation,);
                    }
                }
            }
            if let Some(attachment) = renderpass.depth_attachment {
                let resource = self.resource.retrieve_resource(attachment);

                let read_passes = resource.get_read_passes();
                let write_passes = resource.get_write_passes();

                // Get last operation that occured
                let mut last_operation = LastUsage::None;
                for j in 0..i {
                    let previous_pass = self.order_of_passes[j];
                    // Should not be able to be both write and read in same pass(for now)
                    if read_passes.contains(&previous_pass) {
                        last_operation = LastUsage::Read;
                    }
                    if write_passes.contains(&previous_pass) {
                        last_operation = LastUsage::Write;
                    }
                }

                let image = self.physical_images.get(&attachment).unwrap();
                match last_operation {
                    LastUsage::Write => { // DONT NEED TO BARRIER
                    }
                    LastUsage::Read => {
                        let barrier = ImageBarrier::new(AttachmentHandle::Image(*image))
                            .old_usage(vk::ImageUsageFlags::SAMPLED)
                            .new_usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);
                        barriers.push(barrier);
                        info!("BARRIER: {},{}", resource.name(), last_operation,);
                    }
                    LastUsage::None => {
                        let barrier = ImageBarrier::new(AttachmentHandle::Image(*image))
                            .new_usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);
                        barriers.push(barrier);
                        info!("BARRIER: {},{}", resource.name(), last_operation,);
                    }
                }
            }
            for input in renderpass.texture_inputs.iter() {
                let resource = self.resource.retrieve_resource(*input);

                let read_passes = resource.get_read_passes();
                let write_passes = resource.get_write_passes();

                // Get last operation that occured
                let mut last_operation = LastUsage::None;
                let mut last_usage = vk::ImageUsageFlags::empty();
                for j in 0..i {
                    let previous_pass = self.order_of_passes[j];
                    let previous_virtual_pass = self.passes.retrieve_render_pass(previous_pass);

                    if previous_virtual_pass.color_attachments.contains(input) {
                        last_operation = LastUsage::Write;
                        last_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT;
                    } else if previous_virtual_pass.depth_attachment == Some(*input) {
                        last_operation = LastUsage::Write;
                        last_usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
                    } else if previous_virtual_pass.texture_inputs.contains(input) {
                        last_operation = LastUsage::Read;
                        last_usage = vk::ImageUsageFlags::SAMPLED;
                    }
                }

                let image = self.physical_images.get(&input).unwrap();
                match last_operation {
                    LastUsage::Write => {
                        let barrier = ImageBarrier::new(AttachmentHandle::Image(*image))
                            .old_usage(last_usage)
                            .new_usage(vk::ImageUsageFlags::SAMPLED);
                        barriers.push(barrier);
                        info!("BARRIER: {},{}", resource.name(), last_operation,);
                    }
                    LastUsage::Read => {}
                    LastUsage::None => {
                        let barrier = ImageBarrier::new(AttachmentHandle::Image(*image))
                            .new_usage(vk::ImageUsageFlags::SAMPLED);
                        barriers.push(barrier);
                        info!("BARRIER: {},{}", resource.name(), last_operation,);
                    }
                }
            }

            let virtual_pass = self.passes.retrieve_render_pass(*virtual_pass_handle);
            info!(
                "Barriers for Renderpass: {},{}",
                virtual_pass.name,
                barriers.len()
            );
            let physical_renderpass = self.physical_passes.get_mut(virtual_pass_handle).unwrap();
            physical_renderpass.barriers = barriers;
        }
    }

    pub fn run_pass<F>(&mut self, render_pass: VirtualRenderPassHandle, commands: F)
    where
        F: FnOnce(&mut Self, vk::CommandBuffer),
    {
        // DO IMAGE BARRIERS NEEDED
        // START RENDERPASS

        let physical_render_pass = self.get_physical_pass(render_pass);

        let mut barrier_builder = ImageBarrierBuilder::default();
        for barrier in physical_render_pass.barriers.iter() {
            barrier_builder = barrier_builder.add_image_barrier(barrier.clone());
        }
        barrier_builder
            .build(&self.device, &self.device.graphics_command_buffer())
            .unwrap();

        unsafe {
            self.device.vk_device.cmd_set_viewport(
                self.device.graphics_command_buffer(),
                0u32,
                &[physical_render_pass.viewport],
            )
        };
        unsafe {
            self.device.vk_device.cmd_set_scissor(
                self.device.graphics_command_buffer(),
                0u32,
                &[physical_render_pass.scissor],
            )
        };

        let depth_attachment = physical_render_pass.depth_attachment.as_ref();
        let render_info = {
            if physical_render_pass.depth_attachment.is_some() {
                vk::RenderingInfo::builder()
                    .render_area(physical_render_pass.scissor)
                    .layer_count(1u32)
                    .color_attachments(&physical_render_pass.attachments)
                    .depth_attachment(depth_attachment.unwrap())
            } else {
                vk::RenderingInfo::builder()
                    .render_area(physical_render_pass.scissor)
                    .layer_count(1u32)
                    .color_attachments(&physical_render_pass.attachments)
            }
        };

        unsafe {
            self.device
                .vk_device
                .cmd_begin_rendering(self.device.graphics_command_buffer(), &render_info)
        };

        commands(self, self.device.graphics_command_buffer());

        unsafe {
            self.device
                .vk_device
                .cmd_end_rendering(self.device.graphics_command_buffer());
        };
    }

    fn get_physical_pass(&self, handle: VirtualRenderPassHandle) -> &PhysicalRenderPass {
        self.physical_passes.get(&handle).unwrap()
    }

    pub fn get_physical_resource(&mut self, name: &str) -> ImageHandle {
        let (handle, _) = self.resource.get_texture_resource(name);
        *self.physical_images.get(&handle).unwrap()
    }
}

/// Public API for creating render pass
#[derive(Clone, Default)]
pub struct RenderPassLayout {
    pub color_attachments: Vec<(String, AttachmentInfo)>,
    pub depth_attachment: Option<(String, AttachmentInfo)>,
    pub texture_inputs: Vec<String>,
    clear_colour: [f32; 4],
    depth_clear: f32,
    stencil_clear: u32,
}

impl RenderPassLayout {
    pub fn add_color_attachment(mut self, name: &str, info: &AttachmentInfo) -> Self {
        self.color_attachments
            .push((name.to_string(), info.clone()));
        self
    }

    pub fn set_depth_stencil_attachment(mut self, name: &str, info: &AttachmentInfo) -> Self {
        self.depth_attachment = Some((name.to_string(), info.clone()));
        self
    }

    pub fn add_texture_input(mut self, name: &str) -> Self {
        self.texture_inputs.push(name.to_string());
        self
    }

    pub fn set_clear_colour(mut self, colour: [f32; 4]) -> Self {
        self.clear_colour = colour;
        self
    }

    pub fn set_depth_stencil_clear(mut self, depth: f32, stencil: u32) -> Self {
        self.depth_clear = depth;
        self.stencil_clear = stencil;
        self
    }
}

#[derive(Default)]
struct PhysicalRenderPass {
    attachments: Vec<vk::RenderingAttachmentInfo>,
    depth_attachment: Option<vk::RenderingAttachmentInfo>,
    viewport: vk::Viewport,
    scissor: vk::Rect2D,
    barriers: Vec<ImageBarrier>,
    clear_color: vk::ClearValue,
    depth_stencil_clear: vk::ClearValue,
}

/*NOTES:

Builds up VirtualRenderPasses which consist of VirtualTextureResources.
Once all render passes have been added, will generate images for all of the virtual texture resources.
Once all physical images have been created, will create physical renderpasses & barriers.
Barriers will be stored with the renderpass where they are needed.
Then, when starting the specified renderpass will also use those barriers.

EXPECTED API:
let mut list = RenderList::new(device);

let forward = AttachmentInfo...
let bright = AttachmentInfo...

let forward_pass = list.add(RenderPass::new()
                 .add_colour_attachment("forward", forward)
                 .add_colour_attachment("bright", bright)

list.bake();

list.run_pass(forward_pass, |cmd| {});
*/

fn get_viewport_info(size: (u32, u32), flipped: bool) -> vk::Viewport {
    if flipped {
        vk::Viewport::builder()
            .x(0.0f32)
            .y(size.1 as f32)
            .width(size.0 as f32)
            .height(-(size.1 as f32))
            .min_depth(0.0f32)
            .max_depth(1.0f32)
            .build()
    } else {
        vk::Viewport::builder()
            .x(0.0f32)
            .y(0.0f32)
            .width(size.0 as f32)
            .height(size.1 as f32)
            .min_depth(0.0f32)
            .max_depth(1.0f32)
            .build()
    }
}

enum LastUsage {
    Write,
    Read,
    None,
}

impl Display for LastUsage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let display = match self {
            LastUsage::Write => "WRITE",
            LastUsage::Read => "READ",
            LastUsage::None => "NONE",
        };
        write!(f, "{}", display)
    }
}
