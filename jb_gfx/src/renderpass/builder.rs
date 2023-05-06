use anyhow::Result;
use ash::vk;
use ash::vk::{AccessFlags2, ImageLayout, PipelineStageFlags2};
use log::info;
use std::collections::HashMap;
use std::mem::replace;

use crate::renderpass::barrier::{ImageBarrier, ImageBarrierBuilder};
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
            colour_attachments.push(convert_attach_info(device, attachment));

            let &mut last_usage = usage_tracker
                .get_last_usage(attachment.target)
                .get_or_insert(vk::ImageUsageFlags::empty());
            if last_usage != vk::ImageUsageFlags::COLOR_ATTACHMENT {
                usage_tracker
                    .set_last_usage(attachment.target, vk::ImageUsageFlags::COLOR_ATTACHMENT);
                let barrier = ImageBarrier {
                    image: attachment.target,
                    src_stage_mask: get_stage_flag_from_usage(last_usage),
                    src_access_mask: get_access_flag_from_usage(last_usage),
                    old_layout: get_image_layout_from_usage(last_usage),
                    dst_stage_mask: PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    ..Default::default()
                };
                image_barriers.push(barrier);
            }
        }

        if let Some(attachment) = self.depth_attachment {
            let &mut last_usage = usage_tracker
                .get_last_usage(attachment.target)
                .get_or_insert(vk::ImageUsageFlags::empty());
            if last_usage != vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT {
                usage_tracker.set_last_usage(
                    attachment.target,
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                );
                let barrier = ImageBarrier {
                    image: attachment.target,
                    src_stage_mask: get_stage_flag_from_usage(last_usage),
                    src_access_mask: get_access_flag_from_usage(last_usage),
                    old_layout: get_image_layout_from_usage(last_usage),
                    dst_stage_mask: PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                    dst_access_mask: AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    new_layout: ImageLayout::ATTACHMENT_OPTIMAL,
                    ..Default::default()
                };
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
                let barrier = ImageBarrier {
                    image: AttachmentHandle::Image(handle),
                    src_stage_mask: get_stage_flag_from_usage(last_usage),
                    src_access_mask: get_access_flag_from_usage(last_usage),
                    old_layout: get_image_layout_from_usage(last_usage),
                    dst_stage_mask: PipelineStageFlags2::FRAGMENT_SHADER,
                    dst_access_mask: AccessFlags2::SHADER_READ,
                    new_layout: ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ..Default::default()
                };
                image_barriers.push(barrier);
            }
        }

        let mut barrier_builder = ImageBarrierBuilder::default();
        for barrier in image_barriers.into_iter() {
            barrier_builder = barrier_builder.add_image_barrier(barrier);
        }
        barrier_builder.build(device, command_buffer)?;

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

/// A RenderPass Attachment
///
/// This represents an attachment to a [RenderPass]. It contains
/// an AttachmentHandle which is the handle to the image of the attachment.
/// It also determines the [vk::ImageLayout] the image must be in, the
/// load [vk::AttachmentLoadOp] and store [vk::AttachmentStoreOp] operations that take place.
/// Finally, it contains the [vk::ClearValue] of the attachment.
///
/// # Usage
///
/// This is fed into a [RenderPassBuilder] to create a [RenderPass]
///
/// ```
/// use ash::vk::{ClearColorValue, ClearValue};
/// use jb_gfx::{AttachmentHandle, AttachmentInfo, RenderPassBuilder};
///
/// RenderPassBuilder::new((1920,1080))
/// .add_colour_attachment(AttachmentInfo {
///     target: AttachmentHandle::Image(device.render_image),
///            clear_value: ClearValue {
///                color: ClearColorValue {
///                    float32: clear_colour.extend(0.0).into(),
///                },
///            },
///            ..Default::default()
///     });
///
/// ```
#[derive(Copy, Clone)]
pub struct AttachmentInfo {
    pub target: AttachmentHandle,
    pub image_layout: vk::ImageLayout,
    pub load_op: vk::AttachmentLoadOp,
    pub store_op: vk::AttachmentStoreOp,
    pub clear_value: vk::ClearValue,
}

/// A RenderPass Attachment
///
/// A handle to either a [RenderTargetHandle] or a SwapchainImage(index)
#[derive(Copy, Clone, PartialEq, Hash)]
pub enum AttachmentHandle {
    Image(ImageHandle),
    SwapchainImage,
}

impl Eq for AttachmentHandle {}

impl Default for AttachmentInfo {
    fn default() -> Self {
        Self {
            target: AttachmentHandle::Image(ImageHandle::default()),
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
            AttachmentHandle::Image(image) => device
                .resource_manager
                .get_image(image)
                .unwrap()
                .image_view(),
            AttachmentHandle::SwapchainImage => device.get_present_image_view(),
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

fn get_stage_flag_from_usage(flags: vk::ImageUsageFlags) -> vk::PipelineStageFlags2 {
    if flags == vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT {
        vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS
    } else if flags == vk::ImageUsageFlags::SAMPLED {
        vk::PipelineStageFlags2::FRAGMENT_SHADER
    } else {
        vk::PipelineStageFlags2::empty()
    }
}

fn get_access_flag_from_usage(flags: vk::ImageUsageFlags) -> vk::AccessFlags2 {
    if flags == vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT {
        vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
    } else if flags == vk::ImageUsageFlags::SAMPLED {
        vk::AccessFlags2::SHADER_READ
    } else {
        vk::AccessFlags2::empty()
    }
}

fn get_image_layout_from_usage(flags: vk::ImageUsageFlags) -> vk::ImageLayout {
    if flags == vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT {
        vk::ImageLayout::ATTACHMENT_OPTIMAL
    } else if flags == vk::ImageUsageFlags::SAMPLED {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    } else {
        vk::ImageLayout::UNDEFINED
    }
}

#[derive(Default)]
pub struct ImageUsageTracker {
    usages: HashMap<AttachmentHandle, vk::ImageUsageFlags>,
}

impl ImageUsageTracker {
    pub fn get_last_usage(&self, handle: AttachmentHandle) -> Option<vk::ImageUsageFlags> {
        self.usages.get(&handle).cloned()
    }

    pub fn set_last_usage(&mut self, handle: AttachmentHandle, usage: vk::ImageUsageFlags) {
        if let Some(old) = self.usages.get_mut(&handle) {
            let _ = replace(old, usage);
        } else {
            self.usages.insert(handle, usage);
        }
    }
}
