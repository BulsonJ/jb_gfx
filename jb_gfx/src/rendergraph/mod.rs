use std::sync::Arc;

use crate::GraphicsDevice;
use crate::rendergraph::resource_tracker::{RenderPassTracker, RenderResourceTracker};
use crate::rendergraph::virtual_resource::VirtualResource;

pub mod attachment;
pub mod physical_resource;
pub mod virtual_resource;
pub mod resource_tracker;

pub struct RenderList {
    device: Arc<GraphicsDevice>,
    passes: RenderPassTracker,
    resource: RenderResourceTracker,
}
impl RenderList {
    pub fn new(device: Arc<GraphicsDevice>) -> Self {
        Self {
            device,
            passes: RenderPassTracker::default(),
            resource: RenderResourceTracker::default(),
        }
    }
}

/*NOTES:

Builds up VirtualRenderPasses which consist of VirtualTextureResources.
Once all render passes have been added, will generate images for all of the virtual texture resources.
Once all physical images have been created, will create physical renderpasses & barriers.
Barriers will be stored with the renderpass where they are needed.
Then, when starting the specified renderpass will also use those barriers.

EXPECTED API:
let mut list = RenderList::new(device);

let forward = AttachmentInfo...
let bright = AttachmentInfo...

let forward_pass = list.add(RenderPass::new()
                 .add_colour_attachment("forward", forward)
                 .add_colour_attachment("bright", bright)

list.bake();

list.run_pass(forward_pass, |cmd| {});
*/
