use ash::vk;
use log::info;
use std::collections::HashMap;
use std::sync::Arc;

use crate::rendergraph::attachment::{AttachmentInfo, SizeClass};
use crate::rendergraph::resource_tracker::{RenderPassTracker, RenderResourceTracker};
use crate::rendergraph::virtual_resource::{VirtualRenderPassHandle, VirtualResource};
use crate::GraphicsDevice;

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
}

impl RenderList {
    pub fn new(device: Arc<GraphicsDevice>) -> Self {
        Self {
            device,
            passes: RenderPassTracker::default(),
            resource: RenderResourceTracker::default(),
            order_of_passes: Vec::default(),
            physical_passes: HashMap::default(),
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
            resource.set_image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);
            resource.read_in_pass(pass_handle);
            render_pass.texture_inputs.push(resource_handle);
        }

        self.order_of_passes.push(pass_handle);

        pass_handle
    }

    pub fn bake(&mut self) {
        // Create physical images
        let mut images = HashMap::new();
        for (handle, resource) in self.resource.get_resources() {
            let size = {
                match resource.get_attachment_info().size {
                    SizeClass::SwapchainRelative => {
                        (self.device.size().width, self.device.size().height)
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

            images.insert(handle, image);
            info!("Image Created: {}", resource.name());
        }

        for &pass in self.order_of_passes.iter() {
            let mut physical_render_pass = PhysicalRenderPass::default();

            let renderpass = self.passes.retrieve_render_pass(pass);

            for &color in renderpass.color_attachments.iter() {
                let physical_image = images.get(&color).unwrap();
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
                    clear_value: Default::default(),
                    ..Default::default()
                };

                let resource = self.resource.retrieve_resource(color);
                let size = match resource.get_attachment_info().size {
                    SizeClass::SwapchainRelative => (1920, 1080),
                    SizeClass::Custom(width, height) => (width, height),
                };
                let viewport = get_viewport_info(size, true);
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
                let physical_image = images.get(&depth).unwrap();
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
                    clear_value: Default::default(),
                    ..Default::default()
                };

                let resource = self.resource.retrieve_resource(depth);
                let size = match resource.get_attachment_info().size {
                    SizeClass::SwapchainRelative => (1920, 1080),
                    SizeClass::Custom(width, height) => (width, height),
                };
                let viewport = get_viewport_info(size, true);
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
    }

    pub fn run_pass<F>(&self, render_pass: VirtualRenderPassHandle, commands: F)
    where
        F: Fn(vk::CommandBuffer),
    {
        // DO IMAGE BARRIERS NEEDED
        // START RENDERPASS

        let physical_render_pass = self.get_physical_pass(render_pass);

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

        commands(self.device.graphics_command_buffer());

        unsafe {
            self.device
                .vk_device
                .cmd_end_rendering(self.device.graphics_command_buffer());
        };
    }

    fn get_physical_pass(&self, handle: VirtualRenderPassHandle) -> &PhysicalRenderPass {
        self.physical_passes.get(&handle).unwrap()
    }
}

/// Public API for creating render pass
#[derive(Clone, Default)]
pub struct RenderPassLayout {
    pub color_attachments: Vec<(String, AttachmentInfo)>,
    pub depth_attachment: Option<(String, AttachmentInfo)>,
    pub texture_inputs: Vec<String>,
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
}

#[derive(Default)]
struct PhysicalRenderPass {
    attachments: Vec<vk::RenderingAttachmentInfo>,
    depth_attachment: Option<vk::RenderingAttachmentInfo>,
    viewport: vk::Viewport,
    scissor: vk::Rect2D,
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
