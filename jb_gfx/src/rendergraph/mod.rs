use crate::rendergraph::virtual_resource::{VirtualRenderPassHandle, VirtualResource, VirtualTextureResource, VirtualTextureResourceHandle};
use ash::vk;
use slotmap::SlotMap;
use std::collections::HashMap;

pub mod attachment;
pub mod physical_resource;
pub mod virtual_resource;

#[derive(Default)]
struct RenderPassTracker {
    pass_to_handle: HashMap<String, VirtualRenderPassHandle>,
    passes: SlotMap<VirtualRenderPassHandle, RenderPassInternal>,
}
impl RenderPassTracker {
    fn get_render_pass(
        &mut self,
        name: &str,
    ) -> (VirtualRenderPassHandle, &mut RenderPassInternal) {
        // If already existing resource
        if let Some(&handle) = self.pass_to_handle.get(name) {
            let pass = self.passes.get_mut(handle).unwrap();
            (handle, pass)
        } else {
            // If new resource
            let handle = self.passes.insert(RenderPassInternal::default());
            self.pass_to_handle.insert(name.to_string(), handle);
            let pass = self.passes.get_mut(handle).unwrap();
            //pass.set_name(name);
            (handle, pass)
        }
    }
}

/// Internal RenderPass used for tracking resources
#[derive(Clone, Default)]
struct RenderPassInternal {
    pub name: String,
    pub color_attachments: Vec<VirtualTextureResourceHandle>,
    pub depth_attachment: Option<VirtualTextureResourceHandle>,
}

struct RenderResourceTracker {
    resource_to_handle: HashMap<String, VirtualTextureResourceHandle>,
    resources: SlotMap<VirtualTextureResourceHandle, VirtualTextureResource>,
}
impl RenderResourceTracker {
    fn get_texture_resource(
        &mut self,
        name: &str,
    ) -> (VirtualTextureResourceHandle, &mut VirtualTextureResource) {
        // If already existing resource
        if let Some(&handle) = self.resource_to_handle.get(name) {
            let resource = self.resources.get_mut(handle).unwrap();
            (handle, resource)
        } else {
            // If new resource
            let handle = self.resources.insert(VirtualTextureResource::default());
            self.resource_to_handle.insert(name.to_string(), handle);
            let resource = self.resources.get_mut(handle).unwrap();
            resource.set_name(name);
            (handle, resource)
        }
    }
}
