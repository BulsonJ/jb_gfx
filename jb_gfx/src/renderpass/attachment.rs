use crate::renderpass::resource::ImageUsageTracker;
use crate::{GraphicsDevice, ImageHandle};
use ash::vk;

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
            clear_value: vk::ClearValue::default(),
        }
    }
}
