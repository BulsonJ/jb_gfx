use anyhow::Result;
use ash::vk;
use ash::vk::{AccessFlags2, ImageAspectFlags, ImageLayout, PipelineStageFlags2};

use crate::device::GraphicsDevice;
use crate::resource::ImageHandle;

pub struct ImageBarrier {
    pub image: ImageHandleType,
    pub src_stage_mask: PipelineStageFlags2,
    pub src_access_mask: AccessFlags2,
    pub dst_stage_mask: PipelineStageFlags2,
    pub dst_access_mask: AccessFlags2,
    pub old_layout: ImageLayout,
    pub new_layout: ImageLayout,
    pub base_mip_level: u32,
    pub level_count: u32,
}

pub enum ImageHandleType {
    Image(ImageHandle),
    SwapchainImage(),
}

impl Default for ImageBarrier {
    fn default() -> Self {
        Self {
            image: ImageHandleType::Image(ImageHandle::default()),
            src_stage_mask: PipelineStageFlags2::NONE,
            src_access_mask: AccessFlags2::NONE,
            dst_stage_mask: PipelineStageFlags2::NONE,
            dst_access_mask: AccessFlags2::NONE,
            old_layout: ImageLayout::UNDEFINED,
            new_layout: ImageLayout::UNDEFINED,
            base_mip_level: 0,
            level_count: 1,
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
                ImageHandleType::Image(image) => {
                    Some(device.resource_manager.get_image(image).unwrap())
                }
                _ => None,
            };

            let image_handle = match image_barrier.image {
                ImageHandleType::SwapchainImage() => device.get_present_image(),
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
                    layer_count: 1,
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
