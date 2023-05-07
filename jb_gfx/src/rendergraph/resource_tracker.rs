use std::collections::HashMap;

use slotmap::SlotMap;

use crate::rendergraph::virtual_resource::{VirtualRenderPassHandle, VirtualResource, VirtualTextureResource, VirtualTextureResourceHandle};

#[derive(Default)]
pub struct RenderPassTracker {
    pass_to_handle: HashMap<String, VirtualRenderPassHandle>,
    passes: SlotMap<VirtualRenderPassHandle, VirtualRenderPass>,
}

impl RenderPassTracker {
    fn get_render_pass(
        &mut self,
        name: &str,
    ) -> (VirtualRenderPassHandle, &mut VirtualRenderPass) {
        // If already existing resource
        if let Some(&handle) = self.pass_to_handle.get(name) {
            let pass = self.passes.get_mut(handle).unwrap();
            (handle, pass)
        } else {
            // If new resource
            let handle = self.passes.insert(VirtualRenderPass::default());
            self.pass_to_handle.insert(name.to_string(), handle);
            let pass = self.passes.get_mut(handle).unwrap();
            //pass.set_name(name);
            (handle, pass)
        }
    }
}

/// Internal RenderPass used for tracking resources
#[derive(Clone, Default)]
struct VirtualRenderPass {
    pub name: String,
    pub color_attachments: Vec<VirtualTextureResourceHandle>,
    pub depth_attachment: Option<VirtualTextureResourceHandle>,
}

#[derive(Default)]
pub struct RenderResourceTracker {
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