use std::collections::HashMap;

use anyhow::Result;
use ash::vk;
use ash::vk::ImageLayout;

use crate::device::FRAMES_IN_FLIGHT;
use crate::resource::{ImageHandle, ResourceManager};
use crate::targets::{RenderTargetHandle, RenderTargets};

#[derive(Default)]
pub struct BindlessManager {
    bindless_textures: Vec<BindlessImage>,
    bindless_indexes: HashMap<BindlessImage, usize>,
    pub descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub enum BindlessImage {
    RenderTarget(RenderTargetHandle),
    Image(ImageHandle),
}

impl BindlessManager {
    pub fn new(descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT]) -> Self {
        Self {
            descriptor_set,
            ..Default::default()
        }
    }

    pub fn get_bindless_index(&self, image: &BindlessImage) -> Option<usize> {
        self.bindless_indexes.get(image).cloned()
    }

    pub fn setup_samplers(&self, samplers: &[vk::Sampler], device: &ash::Device) -> Result<()> {
        for (i, sampler) in samplers.iter().enumerate() {
            let sampler_info = vk::DescriptorImageInfo::builder().sampler(*sampler);

            let image_info = [*sampler_info];
            let desc_write = vk::WriteDescriptorSet::builder()
                .dst_set(self.descriptor_set[0])
                .dst_binding(0u32)
                .dst_array_element(i as u32)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(&image_info);
            let desc_write_two = vk::WriteDescriptorSet::builder()
                .dst_set(self.descriptor_set[1])
                .dst_binding(0u32)
                .dst_array_element(i as u32)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(&image_info);

            unsafe {
                device.update_descriptor_sets(&[*desc_write, *desc_write_two], &[]);
            }
        }

        Ok(())
    }

    pub fn add_image_to_bindless(
        &mut self,
        device: &ash::Device,
        resource_manager: &ResourceManager,
        render_target: &RenderTargets,
        image: &BindlessImage,
    ) {
        self.bindless_textures.push(*image);
        let bindless_index = self.bindless_textures.len();
        self.bindless_indexes.insert(*image, bindless_index);

        let image_view = {
            match image {
                BindlessImage::RenderTarget(handle) => resource_manager
                    .get_image(render_target.get_render_target(*handle).unwrap().image())
                    .unwrap()
                    .image_view(),
                BindlessImage::Image(handle) => {
                    resource_manager.get_image(*handle).unwrap().image_view()
                }
            }
        };

        let bindless_image_info = vk::DescriptorImageInfo::builder()
            .image_view(image_view)
            .image_layout(ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let image_info = [*bindless_image_info];
        let desc_write = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set[0])
            .dst_binding(1u32)
            .dst_array_element(bindless_index as u32 - 1u32)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(&image_info);
        let desc_write_two = vk::WriteDescriptorSet::builder()
            .dst_set(self.descriptor_set[1])
            .dst_binding(1u32)
            .dst_array_element(bindless_index as u32 - 1u32)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(&image_info);

        unsafe {
            device.update_descriptor_sets(&[*desc_write, *desc_write_two], &[]);
        }
    }
}
