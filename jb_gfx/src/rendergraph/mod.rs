use ash::vk;
use std::collections::HashMap;
use std::sync::Arc;
use log::info;

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
}

impl RenderList {
    pub fn new(device: Arc<GraphicsDevice>) -> Self {
        Self {
            device,
            passes: RenderPassTracker::default(),
            resource: RenderResourceTracker::default(),
            order_of_passes: Vec::default(),
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
    }

    pub fn run_pass<F>(&self, render_pass: VirtualRenderPassHandle, commands: F)
    where
        F: Fn(vk::CommandBuffer),
    {
        // DO IMAGE BARRIERS NEEDED
        // START RENDERPASS
        commands(self.device.graphics_command_buffer());
        // END RENDERPASS
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
