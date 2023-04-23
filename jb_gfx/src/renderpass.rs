use anyhow::Result;
use ash::vk;

use crate::device::GraphicsDevice;
use crate::resource::ImageHandle;

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
///
/// RenderPassBuilder::new((1920.0,1080.0))
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
    viewport_size: (u32, u32),
}

impl RenderPassBuilder {
    /// Start constructing a new RenderPass.
    ///
    /// # Examples
    ///
    /// ```
    /// RenderPassBuilder::new((1920.0,1080.0))
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

pub struct RenderPass<'a> {
    device: &'a ash::Device,
    command_buffer: &'a vk::CommandBuffer,
}

impl<'a> RenderPass<'a> {
    /// Updates the Scissor of the RenderPass, starting from
    /// position: offset and size: extent.
    ///
    /// # Examples
    ///
    /// ```
    /// render_pass.set_scissor([0.0,0.0], [1920.0, 1080.0])
    /// ```
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
                .cmd_set_scissor(*self.command_buffer, 0u32, &[*scissor])
        };
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
///
/// RenderPassBuilder::new((1920.0,1080.0))
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
#[derive(Copy, Clone)]
pub enum AttachmentHandle {
    Image(ImageHandle),
    SwapchainImage,
}

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
