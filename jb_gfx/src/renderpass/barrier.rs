use anyhow::Result;
use ash::vk;
use ash::vk::{AccessFlags2, ImageAspectFlags, ImageLayout, PipelineStageFlags2};

use crate::resource::ImageHandle;
use crate::{AttachmentHandle, GraphicsDevice};

pub struct ImageBarrier {
    pub image: AttachmentHandle,
    pub src_stage_mask: PipelineStageFlags2,
    pub src_access_mask: AccessFlags2,
    pub dst_stage_mask: PipelineStageFlags2,
    pub dst_access_mask: AccessFlags2,
    pub old_layout: ImageLayout,
    pub new_layout: ImageLayout,
    pub base_mip_level: u32,
    pub level_count: u32,
    pub image_layers: u32,
}

impl ImageBarrier {
    pub fn new(attachment: AttachmentHandle) -> Self {
        Self {
            image: attachment,
            ..Default::default()
        }
    }

    pub fn old_usage(mut self, usage: vk::ImageUsageFlags) -> Self {
        self.src_access_mask = get_access_flag_from_usage(usage);
        self.src_stage_mask = get_stage_flag_from_usage(usage);
        self.old_layout = get_image_layout_from_usage(usage);
        self
    }

    pub fn new_usage(mut self, usage: vk::ImageUsageFlags) -> Self {
        self.dst_access_mask = get_access_flag_from_usage(usage);
        self.dst_stage_mask = get_stage_flag_from_usage(usage);
        self.new_layout = get_image_layout_from_usage(usage);
        self
    }

    pub fn base_mip_level(mut self, base_mip_level: u32) -> Self {
        self.base_mip_level = base_mip_level;
        self
    }

    pub fn level_count(mut self, level_count: u32) -> Self {
        self.level_count = level_count;
        self
    }

    pub fn image_layers(mut self, image_layers: u32) -> Self {
        self.image_layers = image_layers;
        self
    }
}

impl Default for ImageBarrier {
    fn default() -> Self {
        Self {
            image: AttachmentHandle::Image(ImageHandle::default()),
            src_stage_mask: PipelineStageFlags2::NONE,
            src_access_mask: AccessFlags2::NONE,
            dst_stage_mask: PipelineStageFlags2::NONE,
            dst_access_mask: AccessFlags2::NONE,
            old_layout: ImageLayout::UNDEFINED,
            new_layout: ImageLayout::UNDEFINED,
            base_mip_level: 0,
            level_count: 1,
            image_layers: 1,
        }
    }
}

#[derive(Default)]
pub struct ImageBarrierBuilder {
    barriers: Vec<ImageBarrier>,
}

impl ImageBarrierBuilder {
    pub fn add_image_barrier(mut self, barrier: ImageBarrier) -> ImageBarrierBuilder {
        self.barriers.push(barrier);
        self
    }

    pub fn build(self, device: &GraphicsDevice, command_buffer: &vk::CommandBuffer) -> Result<()> {
        let mut image_memory_barriers = Vec::new();
        for image_barrier in self.barriers.iter() {
            let image = match image_barrier.image {
                AttachmentHandle::Image(image) => {
                    Some(device.resource_manager.get_image(image).unwrap())
                }
                _ => None,
            };

            let image_handle = match image_barrier.image {
                AttachmentHandle::SwapchainImage => device.get_present_image(),
                _ => image.unwrap().image(),
            };

            let aspect_mask: ImageAspectFlags = {
                if let Some(image) = image {
                    image.aspect_flags()
                } else {
                    ImageAspectFlags::COLOR
                }
            };

            let barrier = vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(image_barrier.src_stage_mask)
                .src_access_mask(image_barrier.src_access_mask)
                .dst_stage_mask(image_barrier.dst_stage_mask)
                .dst_access_mask(image_barrier.dst_access_mask)
                .old_layout(image_barrier.old_layout)
                .new_layout(image_barrier.new_layout)
                .image(image_handle)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask,
                    base_mip_level: image_barrier.base_mip_level,
                    level_count: image_barrier.level_count,
                    base_array_layer: 0,
                    layer_count: image_barrier.image_layers,
                });
            image_memory_barriers.push(*barrier);
        }

        let graphics_barrier_dependency_info =
            vk::DependencyInfo::builder().image_memory_barriers(&image_memory_barriers);

        unsafe {
            device
                .vk_device
                .cmd_pipeline_barrier2(*command_buffer, &graphics_barrier_dependency_info)
        };

        Ok(())
    }
}

fn get_stage_flag_from_usage(flags: vk::ImageUsageFlags) -> vk::PipelineStageFlags2 {
    if flags == vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT {
        vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS
    } else if flags == vk::ImageUsageFlags::SAMPLED {
        vk::PipelineStageFlags2::FRAGMENT_SHADER
    } else if flags == vk::ImageUsageFlags::COLOR_ATTACHMENT {
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
    } else {
        vk::PipelineStageFlags2::empty()
    }
}

fn get_access_flag_from_usage(flags: vk::ImageUsageFlags) -> vk::AccessFlags2 {
    if flags == vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT {
        vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
    } else if flags == vk::ImageUsageFlags::SAMPLED {
        vk::AccessFlags2::SHADER_READ
    } else if flags == vk::ImageUsageFlags::COLOR_ATTACHMENT {
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
    } else {
        vk::AccessFlags2::empty()
    }
}

fn get_image_layout_from_usage(flags: vk::ImageUsageFlags) -> vk::ImageLayout {
    if flags == vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT {
        vk::ImageLayout::ATTACHMENT_OPTIMAL
    } else if flags == vk::ImageUsageFlags::SAMPLED {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    } else if flags == vk::ImageUsageFlags::COLOR_ATTACHMENT {
        vk::ImageLayout::ATTACHMENT_OPTIMAL
    } else {
        vk::ImageLayout::UNDEFINED
    }
}
