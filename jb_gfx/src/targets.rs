use crate::resource::{ImageHandle, ResourceManager};
use anyhow::Result;
use ash::vk;
use log::info;
use slotmap::{new_key_type, SlotMap};

pub struct RenderTargets {
    targets: SlotMap<RenderTargetHandle, RenderTarget>,
    fullscreen_size: (u32, u32),
}

impl RenderTargets {
    pub fn new(size: (u32, u32)) -> Self {
        Self {
            targets: SlotMap::default(),
            fullscreen_size: size,
        }
    }

    pub fn create_render_target(
        &mut self,
        resource_manager: &mut ResourceManager,
        format: vk::Format,
        size: RenderTargetSize,
        image_type: RenderImageType,
    ) -> Result<RenderTargetHandle> {
        let actual_size = match size {
            RenderTargetSize::Static(width, height) => (width, height),
            RenderTargetSize::Fullscreen => self.fullscreen_size,
        };

        let render_image =
            create_render_target_image(resource_manager, format, actual_size, image_type)?;
        let render_target = RenderTarget {
            image: render_image,
            size,
            format,
            image_type,
        };
        Ok(self.targets.insert(render_target))
    }

    pub fn get_render_target(&self, render_target: RenderTargetHandle) -> Option<&RenderTarget> {
        self.targets.get(render_target)
    }

    pub fn recreate_render_targets(
        &mut self,
        resource_manager: &mut ResourceManager,
        new_size: (u32, u32),
    ) -> Result<()> {
        self.fullscreen_size = new_size;

        for (_, render_target) in self.targets.iter_mut() {
            if render_target.size != RenderTargetSize::Fullscreen {
                continue;
            }

            let size = {
                match render_target.size {
                    RenderTargetSize::Fullscreen => self.fullscreen_size,
                    _ => (0, 0),
                }
            };

            resource_manager.destroy_image(render_target.image);
            render_target.image = create_render_target_image(
                resource_manager,
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

    pub fn size(&self) -> RenderTargetSize {
        self.size
    }

    pub fn format(&self) -> vk::Format {
        self.format
    }
}

fn create_render_target_image(
    resource_manager: &mut ResourceManager,
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
