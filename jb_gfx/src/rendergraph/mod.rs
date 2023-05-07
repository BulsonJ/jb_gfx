use ash::vk;
use std::sync::Arc;

use crate::rendergraph::attachment::{AttachmentInfo};
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
}

impl RenderList {
    pub fn new(device: Arc<GraphicsDevice>) -> Self {
        Self {
            device,
            passes: RenderPassTracker::default(),
            resource: RenderResourceTracker::default(),
        }
    }

    pub fn add_pass(&mut self, name: &str, pass_layout: RenderPassLayout) -> VirtualRenderPassHandle {
        let (pass_handle, render_pass) = self.passes.get_render_pass(name);
        render_pass.name = name.to_string();
        for attach in pass_layout.color_attachments {
            let (resource_handle, resource) = self.resource.get_texture_resource(&attach.0);
            resource.set_image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);
            resource.write_in_pass(pass_handle);
            render_pass.color_attachments.push(resource_handle);
        }
        if let Some(attach) = pass_layout.depth_attachment {
            let (resource_handle, resource) = self.resource.get_texture_resource(&attach.0);
            resource.set_image_usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);
            resource.write_in_pass(pass_handle);
            render_pass.depth_attachment = Some(resource_handle);
        }
        pass_handle
    }

    pub fn bake(&mut self) {

    }

    pub fn run_pass<F>(&self, render_pass: VirtualRenderPassHandle, commands: F) where F : Fn(vk::CommandBuffer) {
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
