use ash::vk;

pub mod barrier;
pub mod builder;

pub struct RenderPass<'a> {
    device: &'a ash::Device,
    command_buffer: &'a vk::CommandBuffer,
}

impl<'a> RenderPass<'a> {
    /// Updates the Scissor of the RenderPass, starting from
    /// position: offset and size: extent.
    ///
    /// # Examples
    ///
    /// ```
    /// render_pass.set_scissor([0.0,0.0], [1920.0, 1080.0])
    /// ```
    pub fn set_scissor(&self, offset: [f32; 2], extent: [f32; 2]) {
        let scissor = vk::Rect2D::builder()
            .offset(vk::Offset2D {
                x: offset[0] as i32,
                y: offset[1] as i32,
            })
            .extent(vk::Extent2D {
                width: extent[0] as u32,
                height: extent[1] as u32,
            });

        unsafe {
            self.device
                .cmd_set_scissor(*self.command_buffer, 0u32, &[*scissor])
        };
    }
}
