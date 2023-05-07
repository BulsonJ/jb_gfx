use ash::vk;

struct ResourceDimensions {
    format: vk::Format,
    width: u32,
    height: u32,
    usage: vk::ImageUsageFlags,
}

impl PartialEq for ResourceDimensions {
    fn eq(&self, other: &Self) -> bool {
        self.format == other.format && self.width == other.width && self.height == other.height
    }
}
