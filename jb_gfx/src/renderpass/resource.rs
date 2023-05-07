use crate::AttachmentHandle;
use ash::vk;
use std::collections::HashMap;
use std::mem::replace;

#[derive(Default)]
pub struct ImageUsageTracker {
    usages: HashMap<AttachmentHandle, vk::ImageUsageFlags>,
}

impl ImageUsageTracker {
    pub fn get_last_usage(&self, handle: AttachmentHandle) -> Option<vk::ImageUsageFlags> {
        self.usages.get(&handle).cloned()
    }

    pub fn set_last_usage(&mut self, handle: AttachmentHandle, usage: vk::ImageUsageFlags) {
        if let Some(old) = self.usages.get_mut(&handle) {
            let _ = replace(old, usage);
        } else {
            self.usages.insert(handle, usage);
        }
    }
}
