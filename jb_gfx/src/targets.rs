use crate::device::GraphicsDevice;
use anyhow::Result;
use ash::vk;
use log::{info, trace};
use slotmap::{new_key_type, SlotMap};
use std::sync::Arc;

use crate::resource::{ImageHandle, ResourceManager};

pub struct RenderTargets {
    device: Arc<GraphicsDevice>,
    targets: SlotMap<RenderTargetHandle, RenderTarget>,
}

impl RenderTargets {
    pub fn new(device: Arc<GraphicsDevice>) -> Self {
        Self {
            device,
            targets: SlotMap::default(),
        }
    }

    pub fn create_render_target(
        &mut self,
        format: vk::Format,
        size: RenderTargetSize,
        image_type: RenderImageType,
    ) -> Result<RenderTargetHandle> {
        profiling::scope!("Create Render Target");

        let actual_size = match size {
            RenderTargetSize::Static(width, height) => (width, height),
            RenderTargetSize::Fullscreen => (self.device.size().width, self.device.size().height),
        };

        let render_image = create_render_target_image(
            &self.device.resource_manager,
            format,
            actual_size,
            image_type,
        )?;
        let render_target = RenderTarget {
            image: render_image,
            size,
            format,
            image_type,
        };
        trace!(
            "Render Target Created: {} | Size: [{},{}]",
            "Test",
            actual_size.0,
            actual_size.1,
        );
        Ok(self.targets.insert(render_target))
    }

    pub fn get(&self, render_target: RenderTargetHandle) -> Option<ImageHandle> {
        self.targets.get(render_target).map(|render| render.image)
    }

    pub fn recreate_render_targets(&mut self) -> Result<()> {
        profiling::scope!("Recreate Render Targets");

        for (_, render_target) in self.targets.iter_mut() {
            if render_target.size != RenderTargetSize::Fullscreen {
                continue;
            }

            let size = {
                match render_target.size {
                    RenderTargetSize::Fullscreen => {
                        (self.device.size().width, self.device.size().height)
                    }
                    _ => (0, 0),
                }
            };

            info!(
                "Recreating Render Target: {} | Size: [{},{}] |",
                "Test", size.0, size.1,
            );

            self.device
                .resource_manager
                .destroy_image(render_target.image);
            render_target.image = create_render_target_image(
                &self.device.resource_manager,
                render_target.format,
                size,
                render_target.image_type,
            )?;
        }

        info!("Render Targets recreated successfully.");
        Ok(())
    }
}

new_key_type! {pub struct RenderTargetHandle;}

#[derive(Copy, Clone, PartialEq)]
pub enum RenderTargetSize {
    Static(u32, u32),
    Fullscreen,
}

#[derive(Copy, Clone)]
pub enum RenderImageType {
    Colour,
    Depth,
}

pub struct RenderTarget {
    image: ImageHandle,
    size: RenderTargetSize,
    format: vk::Format,
    image_type: RenderImageType,
}

impl RenderTarget {
    pub fn image(&self) -> ImageHandle {
        self.image
    }
}

fn create_render_target_image(
    resource_manager: &ResourceManager,
    format: vk::Format,
    size: (u32, u32),
    image_type: RenderImageType,
) -> Result<ImageHandle> {
    let extent = vk::Extent3D {
        width: size.0,
        height: size.1,
        depth: 1,
    };

    let usage = match image_type {
        RenderImageType::Colour => {
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
        }
        RenderImageType::Depth => {
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
        }
    };

    let render_image = {
        let render_image_create_info = vk::ImageCreateInfo::builder()
            .format(format)
            .usage(usage)
            .extent(extent)
            .image_type(vk::ImageType::TYPE_2D)
            .array_layers(1u32)
            .mip_levels(1u32)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL);

        resource_manager.create_image(&render_image_create_info)
    };

    Ok(render_image)
}
