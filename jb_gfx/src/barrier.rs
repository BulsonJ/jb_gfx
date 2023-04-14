use crate::device::GraphicsDevice;
use crate::resource::{ImageHandle, ResourceManager};
use crate::targets::{RenderTarget, RenderTargetHandle, RenderTargets};
use anyhow::Result;
use ash::vk;
use ash::vk::{AccessFlags2, ImageAspectFlags, ImageLayout, ImageUsageFlags, PipelineStageFlags2};

pub struct ImageBarrier {
    image: ImageHandleType,
    src_stage_mask: PipelineStageFlags2,
    src_access_mask: AccessFlags2,
    dst_stage_mask: PipelineStageFlags2,
    dst_access_mask: AccessFlags2,
    old_layout: ImageLayout,
    new_layout: ImageLayout,
}

pub enum ImageHandleType {
    Image(ImageHandle),
    RenderTarget(RenderTargetHandle),
}

impl ImageBarrier {
    pub fn new(
        image: ImageHandleType,
        src_stage_mask: PipelineStageFlags2,
        src_access_mask: AccessFlags2,
        dst_stage_mask: PipelineStageFlags2,
        dst_access_mask: AccessFlags2,
        old_layout: ImageLayout,
        new_layout: ImageLayout,
    ) -> Self {
        Self {
            image,
            src_stage_mask,
            src_access_mask,
            dst_stage_mask,
            dst_access_mask,
            old_layout,
            new_layout,
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

    pub fn build(
        self,
        vk_device: &ash::Device,
        command_buffer: &vk::CommandBuffer,
        resource_manager: &ResourceManager,
        render_targets: &RenderTargets,
    ) -> Result<()> {
        let mut image_memory_barriers = Vec::new();
        for image_barrier in self.barriers.iter() {
            let image = match image_barrier.image {
                ImageHandleType::Image(image) => resource_manager.get_image(image).unwrap(),
                ImageHandleType::RenderTarget(image) => resource_manager
                    .get_image(render_targets.get_render_target(image).unwrap().image())
                    .unwrap(),
            };

            let aspect_mask = {
                if image
                    .usage()
                    .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                {
                    ImageAspectFlags::COLOR
                } else {
                    ImageAspectFlags::DEPTH
                }
            };

            let barrier = vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(image_barrier.src_stage_mask)
                .src_access_mask(image_barrier.src_access_mask)
                .dst_stage_mask(image_barrier.dst_stage_mask)
                .dst_access_mask(image_barrier.dst_access_mask)
                .old_layout(image_barrier.old_layout)
                .new_layout(image_barrier.new_layout)
                .image(image.image())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            image_memory_barriers.push(*barrier);
        }

        let graphics_barrier_dependency_info =
            vk::DependencyInfo::builder().image_memory_barriers(&image_memory_barriers);

        unsafe {
            vk_device.cmd_pipeline_barrier2(*command_buffer, &graphics_barrier_dependency_info)
        };

        Ok(())
    }
}