use ash::vk;
use slotmap::new_key_type;

use crate::rendergraph::attachment::AttachmentInfo;

new_key_type! {pub struct VirtualTextureResourceHandle; pub struct VirtualRenderPassHandle;}

pub trait VirtualResource {
    fn set_name(&mut self, name: &str);
    fn name(&self) -> &str;
    fn read_in_pass(&mut self, pass: VirtualRenderPassHandle);
    fn write_in_pass(&mut self, pass: VirtualRenderPassHandle);
    fn get_read_passes(&self) -> &[VirtualRenderPassHandle];
    fn get_write_passes(&self) -> &[VirtualRenderPassHandle];
}

#[derive(Default, Clone)]
pub struct VirtualTextureResource {
    name: String,
    attachment_info: AttachmentInfo,
    written_in_passes: Vec<VirtualRenderPassHandle>,
    read_in_passes: Vec<VirtualRenderPassHandle>,
    usage: vk::ImageUsageFlags,
}

impl VirtualTextureResource {
    pub fn set_attachment_info(&mut self, info: AttachmentInfo) {
        self.attachment_info = info
    }

    pub fn get_attachment_info(&self) -> &AttachmentInfo {
        &self.attachment_info
    }

    pub fn set_image_usage(&mut self, usage: vk::ImageUsageFlags) {
        self.usage |= usage
    }

    pub fn get_image_usage(&self) -> vk::ImageUsageFlags {
        self.usage
    }
}

impl VirtualResource for VirtualTextureResource {
    fn set_name(&mut self, name: &str) {
        self.name = name.to_string()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn read_in_pass(&mut self, pass: VirtualRenderPassHandle) {
        self.read_in_passes.push(pass)
    }

    fn write_in_pass(&mut self, pass: VirtualRenderPassHandle) {
        self.written_in_passes.push(pass)
    }

    fn get_read_passes(&self) -> &[VirtualRenderPassHandle] {
        &self.read_in_passes
    }

    fn get_write_passes(&self) -> &[VirtualRenderPassHandle] {
        &self.written_in_passes
    }
}
