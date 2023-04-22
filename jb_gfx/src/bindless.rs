use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use ash::vk;
use ash::vk::ImageLayout;

use crate::device::FRAMES_IN_FLIGHT;
use crate::resource::{ImageHandle, ResourceManager};
use crate::targets::{RenderTargetHandle, RenderTargets};

pub struct BindlessManager {
    device: Arc<ash::Device>,
    resource_manager: Arc<ResourceManager>,
    bindless_textures: Vec<ImageHandle>,
    bindless_indexes: HashMap<ImageHandle, usize>,
    pub descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
}

impl BindlessManager {
    pub fn new(
        device: Arc<ash::Device>,
        resource_manager: Arc<ResourceManager>,
        descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    ) -> Self {
        Self {
            device,
            resource_manager,
            descriptor_set,
            bindless_textures: Vec::default(),
            bindless_indexes: HashMap::default(),
        }
    }

    pub fn get_bindless_index(&self, image: &ImageHandle) -> Option<usize> {
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

    pub fn add_image_to_bindless(&mut self, image: &ImageHandle) {
        self.bindless_textures.push(*image);
        let bindless_index = self.bindless_textures.len();
        self.bindless_indexes.insert(*image, bindless_index);

        let image_view = self
            .resource_manager
            .get_image(*image)
            .unwrap()
            .image_view();

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
            self.device
                .update_descriptor_sets(&[*desc_write, *desc_write_two], &[]);
        }
    }
}
