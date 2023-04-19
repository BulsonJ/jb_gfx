use anyhow::Result;
use ash::vk;

use crate::device::GraphicsDevice;
use crate::targets::RenderTargetHandle;

#[derive(Default)]
pub struct RenderPassBuilder {
    colour_attachments: Vec<AttachmentInfo>,
    depth_attachment: Option<AttachmentInfo>,
    viewport_size: (u32, u32),
}

impl RenderPassBuilder {
    pub fn new(viewport_size: (u32, u32)) -> Self {
        Self {
            viewport_size,
            colour_attachments: Vec::default(),
            ..Default::default()
        }
    }

    pub fn add_colour_attachment(mut self, attachment: AttachmentInfo) -> Self {
        self.colour_attachments.push(attachment);
        self
    }

    pub fn set_depth_attachment(mut self, attachment: AttachmentInfo) -> Self {
        self.depth_attachment = Some(attachment);
        self
    }

    /// Be very careful, no lifetimes so make sure to drop the RenderPass when you are done with it.
    pub fn start(
        mut self,
        device: &GraphicsDevice,
        command_buffer: &vk::CommandBuffer,
    ) -> Result<RenderPass> {
        let viewport = vk::Viewport::builder()
            .x(0.0f32)
            .y(self.viewport_size.1 as f32)
            .width(self.viewport_size.0 as f32)
            .height(-(self.viewport_size.1 as f32))
            .min_depth(0.0f32)
            .max_depth(1.0f32);

        let scissor = vk::Rect2D::builder()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(vk::Extent2D {
                width: self.viewport_size.0,
                height: self.viewport_size.1,
            });

        unsafe {
            device
                .vk_device
                .cmd_set_viewport(*command_buffer, 0u32, &[*viewport])
        };
        unsafe {
            device
                .vk_device
                .cmd_set_scissor(*command_buffer, 0u32, &[*scissor])
        };

        let mut colour_attachments = Vec::new();
        for attachment in self.colour_attachments.iter() {
            colour_attachments.push(convert_attach_info(device, attachment));
        }

        if let Some(attachment) = &self.depth_attachment {
            let depth_attach_info = convert_attach_info(device, attachment);
            let render_info = vk::RenderingInfo::builder()
                .render_area(*scissor)
                .layer_count(1u32)
                .color_attachments(&colour_attachments)
                .depth_attachment(&depth_attach_info);

            unsafe {
                device
                    .vk_device
                    .cmd_begin_rendering(*command_buffer, &render_info)
            };
        } else {
            let render_info = vk::RenderingInfo::builder()
                .render_area(*scissor)
                .layer_count(1u32)
                .color_attachments(&colour_attachments);

            unsafe {
                device
                    .vk_device
                    .cmd_begin_rendering(*command_buffer, &render_info)
            };
        }

        Ok(RenderPass {
            device: device.vk_device.clone(),
            command_buffer: command_buffer.clone(),
        })
    }
}

pub struct RenderPass {
    device: ash::Device,
    command_buffer: vk::CommandBuffer,
}

impl RenderPass {
    pub fn set_scissor(&self, offset: [f32; 2], extent: [f32; 2]) {
        let scissor = vk::Rect2D::builder()
            .offset(vk::Offset2D {
                x: offset[0] as i32,
                y: offset[1] as i32,
            })
            .extent(vk::Extent2D {
                width: extent[0] as u32,
                height: extent[1] as u32,
            });

        unsafe {
            self.device
                .cmd_set_scissor(self.command_buffer, 0u32, &[*scissor])
        };
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe { self.device.cmd_end_rendering(self.command_buffer) };
    }
}

#[derive(Copy, Clone)]
pub struct AttachmentInfo {
    pub target: AttachmentHandleType,
    pub image_layout: vk::ImageLayout,
    pub load_op: vk::AttachmentLoadOp,
    pub store_op: vk::AttachmentStoreOp,
    pub clear_value: vk::ClearValue,
}

#[derive(Copy, Clone)]
pub enum AttachmentHandleType {
    RenderTarget(RenderTargetHandle),
    SwapchainImage(usize),
}

impl Default for AttachmentInfo {
    fn default() -> Self {
        Self {
            target: AttachmentHandleType::RenderTarget(RenderTargetHandle::default()),
            image_layout: vk::ImageLayout::ATTACHMENT_OPTIMAL,
            load_op: vk::AttachmentLoadOp::CLEAR,
            store_op: vk::AttachmentStoreOp::STORE,
            clear_value: vk::ClearValue::default(),
        }
    }
}

fn convert_attach_info(
    device: &GraphicsDevice,
    attachment: &AttachmentInfo,
) -> vk::RenderingAttachmentInfo {
    let image_view = {
        match attachment.target {
            AttachmentHandleType::RenderTarget(render_target) => device
                .resource_manager
                .get_image(
                    device
                        .render_targets()
                        .get_render_target(render_target)
                        .unwrap()
                        .image(),
                )
                .unwrap()
                .image_view(),
            AttachmentHandleType::SwapchainImage(index) => device.present_image_views[index],
        }
    };

    let attach_info = vk::RenderingAttachmentInfo::builder()
        .image_view(image_view)
        .image_layout(attachment.image_layout)
        .load_op(attachment.load_op)
        .store_op(attachment.store_op)
        .clear_value(attachment.clear_value);

    *attach_info
}
