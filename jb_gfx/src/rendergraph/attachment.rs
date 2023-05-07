use ash::vk;

#[derive(Clone)]
pub struct AttachmentInfo {
    pub size: SizeClass,
    pub format: vk::Format,
}

#[derive(Copy, Clone)]
pub enum SizeClass {
    SwapchainRelative,
    Custom(u32, u32),
}
