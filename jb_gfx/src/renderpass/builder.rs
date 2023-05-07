use anyhow::Result;
use ash::vk;

use crate::renderpass::attachment::{AttachmentHandle, AttachmentInfo};
use crate::renderpass::barrier::{ImageBarrier, ImageBarrierBuilder};
use crate::renderpass::resource::ImageUsageTracker;
use crate::renderpass::RenderPass;
use crate::resource::ImageHandle;
use crate::GraphicsDevice;

/// A builder for a [RenderPass]
///
/// This is used to setup the attachments for a [RenderPass].
///
/// # Usage
///
/// Add either Colour [AttachmentInfo]'s or set the Depth [AttachmentInfo].
/// Then, call .start(GraphicsDevice, CommandBuffer, RenderPassClosure).
/// The [RenderPass] can be accessed inside the closure.
///
/// ```
/// use ash::vk::{ClearColorValue, ClearValue};
/// use jb_gfx::{AttachmentHandle, AttachmentInfo, RenderPassBuilder};
///
/// RenderPassBuilder::new((1920,1080))
/// .add_colour_attachment(AttachmentInfo {
///     target: AttachmentHandle::RenderTarget(device.render_image),
///            clear_value: ClearValue {
///                color: ClearColorValue {
///                    float32: clear_colour.extend(0.0).into(),
///                },
///            },
///            ..Default::default()
///     });
///
/// ```
#[derive(Default)]
pub struct RenderPassBuilder {
    colour_attachments: Vec<AttachmentInfo>,
    depth_attachment: Option<AttachmentInfo>,
    texture_inputs: Vec<ImageHandle>,
    viewport_size: (u32, u32),
}

impl RenderPassBuilder {
    /// Start constructing a new RenderPass.
    ///
    /// # Examples
    ///
    /// ```
    /// use jb_gfx::RenderPassBuilder;
    ///
    /// RenderPassBuilder::new((1920,1080));
    /// ```
    pub fn new(viewport_size: (u32, u32)) -> Self {
        Self {
            viewport_size,
            colour_attachments: Vec::default(),
            ..Default::default()
        }
    }

    /// Adds a Color Attachment to the RenderPass.
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn add_colour_attachment(mut self, attachment: AttachmentInfo) -> Self {
        self.colour_attachments.push(attachment);
        self
    }

    /// Sets the Depth Attachment for the RenderPass
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn set_depth_attachment(mut self, attachment: AttachmentInfo) -> Self {
        self.depth_attachment = Some(attachment);
        self
    }

    pub fn set_texture_input(mut self, handle: ImageHandle) -> Self {
        self.texture_inputs.push(handle);
        self
    }

    /// Consumes the RenderPassBuilder, constructing the 'RenderPass'
    /// which can be accessed during the closure.
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn start<F: Fn(&mut RenderPass) -> Result<()>>(
        self,
        device: &GraphicsDevice,
        usage_tracker: &mut ImageUsageTracker,
        command_buffer: &vk::CommandBuffer,
        render_pass: F,
    ) -> Result<()> {
        let viewport = {
            if let Some(attach) = self.colour_attachments.first() {
                match attach.target {
                    AttachmentHandle::SwapchainImage => get_viewport_info(self.viewport_size, true),
                    AttachmentHandle::Image(_) => get_viewport_info(self.viewport_size, false),
                }
            } else {
                get_viewport_info(self.viewport_size, false)
            }
        };

        let scissor = vk::Rect2D::builder()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(vk::Extent2D {
                width: self.viewport_size.0,
                height: self.viewport_size.1,
            });

        unsafe {
            device
                .vk_device
                .cmd_set_viewport(*command_buffer, 0u32, &[viewport])
        };
        unsafe {
            device
                .vk_device
                .cmd_set_scissor(*command_buffer, 0u32, &[*scissor])
        };

        let mut image_barriers = Vec::new();

        let mut colour_attachments = Vec::new();
        for attachment in self.colour_attachments.iter() {
            colour_attachments.push(convert_attach_info(device, usage_tracker, attachment));

            let &mut last_usage = usage_tracker
                .get_last_usage(attachment.target)
                .get_or_insert(vk::ImageUsageFlags::empty());
            if last_usage != vk::ImageUsageFlags::COLOR_ATTACHMENT {
                usage_tracker
                    .set_last_usage(attachment.target, vk::ImageUsageFlags::COLOR_ATTACHMENT);
                let barrier = ImageBarrier::new(attachment.target)
                    .old_usage(last_usage)
                    .new_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);
                image_barriers.push(barrier);
            }
        }

        if let Some(attachment) = self.depth_attachment {
            let &mut last_usage = usage_tracker
                .get_last_usage(attachment.target)
                .get_or_insert(vk::ImageUsageFlags::empty());
            if last_usage != vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT {
                let barrier = ImageBarrier::new(attachment.target)
                    .old_usage(last_usage)
                    .new_usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);
                image_barriers.push(barrier);
            }
        }

        for &handle in self.texture_inputs.iter() {
            let &mut last_usage = usage_tracker
                .get_last_usage(AttachmentHandle::Image(handle))
                .get_or_insert(vk::ImageUsageFlags::empty());
            if last_usage != vk::ImageUsageFlags::SAMPLED {
                usage_tracker.set_last_usage(
                    AttachmentHandle::Image(handle),
                    vk::ImageUsageFlags::SAMPLED,
                );
                let barrier = ImageBarrier::new(AttachmentHandle::Image(handle))
                    .old_usage(last_usage)
                    .new_usage(vk::ImageUsageFlags::SAMPLED);
                image_barriers.push(barrier);
            }
        }

        let mut barrier_builder = ImageBarrierBuilder::default();
        for barrier in image_barriers.into_iter() {
            barrier_builder = barrier_builder.add_image_barrier(barrier);
        }
        barrier_builder.build(device, command_buffer)?;

        if let Some(attachment) = &self.depth_attachment {
            let depth_attach_info = convert_attach_info(device, usage_tracker, attachment);

            // Set usage here so it doesn't mess up finding load/clear op
            usage_tracker.set_last_usage(
                attachment.target,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            );

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

        {
            let mut pass = RenderPass {
                device: &device.vk_device,
                command_buffer,
            };

            render_pass(&mut pass)?;

            unsafe {
                device.vk_device.cmd_end_rendering(*command_buffer);
            }
        }

        Ok(())
    }
}

fn convert_attach_info(
    device: &GraphicsDevice,
    usage_tracker: &ImageUsageTracker,
    attachment: &AttachmentInfo,
) -> vk::RenderingAttachmentInfo {
    let image_view = {
        match attachment.target {
            AttachmentHandle::Image(image) => device
                .resource_manager
                .get_image(image)
                .unwrap()
                .image_view(),
            AttachmentHandle::SwapchainImage => device.get_present_image_view(),
        }
    };

    let &mut last_usage = usage_tracker
        .get_last_usage(attachment.target)
        .get_or_insert(vk::ImageUsageFlags::empty());
    let load_op = {
        if last_usage == vk::ImageUsageFlags::empty() {
            vk::AttachmentLoadOp::CLEAR
        } else {
            vk::AttachmentLoadOp::LOAD
        }
    };

    let attach_info = vk::RenderingAttachmentInfo::builder()
        .image_view(image_view)
        .image_layout(vk::ImageLayout::ATTACHMENT_OPTIMAL)
        .load_op(load_op)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(attachment.clear_value);

    *attach_info
}

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
