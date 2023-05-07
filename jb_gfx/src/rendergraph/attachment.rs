use ash::vk;

#[derive(Clone, Default)]
pub struct AttachmentInfo {
    pub size: SizeClass,
    pub format: vk::Format,
}

#[derive(Copy, Clone)]
pub enum SizeClass {
    SwapchainRelative,
    Custom(u32, u32),
}

impl Default for SizeClass {
    fn default() -> Self {
        Self::SwapchainRelative
    }
}
