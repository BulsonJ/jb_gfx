use anyhow::{anyhow, ensure, Result};
use ash::vk;
use log::info;
use slotmap::{self, new_key_type, SlotMap};

/// Used to create Buffers and Images.
///
/// Currently, not great code as gets around lifetimes by cloning ash structs into it.
/// In the future, refactor to use Rust lifetimes to ensure that a ResourceManager does not outlive the
/// ash structs that it takes in.
pub struct ResourceManager {
    device: ash::Device,
    allocator: vk_mem_alloc::Allocator,
    buffers: SlotMap<BufferHandle, Buffer>,
    images: SlotMap<ImageHandle, Image>,
}

impl ResourceManager {
    pub fn new(
        instance: &ash::Instance,
        pdevice: &vk::PhysicalDevice,
        device: ash::Device,
    ) -> Self {
        let allocator =
            unsafe { vk_mem_alloc::create_allocator(instance, *pdevice, &device, None) }.unwrap();

        Self {
            device,
            allocator,
            buffers: SlotMap::default(),
            images: SlotMap::default(),
        }
    }

    /// Creates a buffer on the GPU and returns a handle([`BufferHandle`] to it.
    ///
    /// # Arguments
    ///
    /// * `buffer_create_info`: The buffer creation information.
    /// * `alloc_create_info`: The allocation creation information.
    ///
    /// returns: BufferHandle
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn create_buffer(
        &mut self,
        buffer_create_info: &vk::BufferCreateInfo,
        alloc_create_info: &vk_mem_alloc::AllocationCreateInfo,
    ) -> BufferHandle {
        // Create the buffer
        let (vk_buffer, allocation, allocation_info) = unsafe {
            vk_mem_alloc::create_buffer(self.allocator, buffer_create_info, alloc_create_info)
        }
        .unwrap();
        let buffer = Buffer {
            buffer: vk_buffer,
            size: buffer_create_info.size,
            allocation,
            allocation_info,
        };

        info!("Buffer created. [Size: {} bytes]", buffer_create_info.size);

        self.buffers.insert(buffer)
    }

    /// Gets a GPU [`Buffer`] using a [`BufferHandle`]. If buffer does not exist, returns [`None`]
    ///
    /// # Arguments
    ///
    /// * `handle`: The handle to the buffer.
    ///
    /// returns: Option<&Buffer>
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn get_buffer(&self, handle: BufferHandle) -> Option<&Buffer> {
        self.buffers.get(handle)
    }

    pub fn get_buffer_mut(&mut self, handle: BufferHandle) -> Option<&mut Buffer> {
        self.buffers.get_mut(handle)
    }

    /// Creates an [`Image`] on the GPU.
    ///
    /// # Arguments
    ///
    /// * `image_create_info`:
    /// * `usage_type`:
    ///
    /// returns: ImageHandle
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn create_image(
        &mut self,
        image_create_info: &vk::ImageCreateInfo,
        usage_type: ImageAspectType,
    ) -> ImageHandle {
        let alloc_create_info = vk_mem_alloc::AllocationCreateInfo {
            usage: vk_mem_alloc::MemoryUsage::AUTO,
            ..Default::default()
        };

        // Create the image
        let (vk_image, allocation, allocation_info) = unsafe {
            vk_mem_alloc::create_image(self.allocator, image_create_info, &alloc_create_info)
        }
        .unwrap();

        let default_image_view_create_info = vk::ImageViewCreateInfo::builder()
            .format(image_create_info.format)
            .image(vk_image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: usage_type.into(),
                level_count: 1u32,
                layer_count: 1u32,
                ..Default::default()
            })
            .build();

        let default_view = {
            unsafe {
                self.device
                    .create_image_view(&default_image_view_create_info, None)
            }
            .unwrap()
        };

        let image = Image {
            image_view: default_view,
            image: vk_image,
            allocation,
            allocation_info,
        };

        info!(
            "Image created. [Dim: {},{}]",
            image_create_info.extent.width, image_create_info.extent.height
        );

        self.images.insert(image)
    }

    pub fn get_image(&mut self, handle: ImageHandle) -> Option<&Image> {
        self.images.get(handle)
    }

    pub fn destroy_image(&mut self, handle: ImageHandle) {
        let image = self.images.remove(handle).unwrap();
        unsafe {
            self.device.destroy_image_view(image.image_view, None);
            vk_mem_alloc::destroy_image(self.allocator, image.image, image.allocation)
        };
    }
    pub fn destroy_resources(&mut self) {
        unsafe {
            for buffer in self.buffers.iter_mut() {
                vk_mem_alloc::destroy_buffer(self.allocator, buffer.1.buffer, buffer.1.allocation);
            }
            for image in self.images.iter_mut() {
                self.device.destroy_image_view(image.1.image_view, None);
                vk_mem_alloc::destroy_image(self.allocator, image.1.image, image.1.allocation);
            }

            vk_mem_alloc::destroy_allocator(self.allocator)
        };
    }
}

pub enum ImageAspectType {
    Color,
    Depth,
}

impl From<ImageAspectType> for vk::ImageAspectFlags {
    fn from(value: ImageAspectType) -> Self {
        match value {
            ImageAspectType::Color => vk::ImageAspectFlags::COLOR,
            ImageAspectType::Depth => vk::ImageAspectFlags::DEPTH,
        }
    }
}

/// A buffer and it's memory allocation.
pub struct Buffer {
    buffer: vk::Buffer,
    size: vk::DeviceSize,
    allocation: vk_mem_alloc::Allocation,
    allocation_info: vk_mem_alloc::AllocationInfo,
}

impl Buffer {
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer
    }

    pub fn size(&self) -> vk::DeviceSize {
        self.size
    }

    pub fn is_mapped(&self) -> bool {
        !self.allocation_info.mapped_data.is_null()
    }

    pub fn view(&self) -> BufferView {
        BufferView {
            buffer: self,
            offset: 0,
            size: self.size,
        }
    }
}

pub struct BufferView<'a> {
    buffer: &'a Buffer,
    offset: vk::DeviceSize,
    size: vk::DeviceSize,
}

impl<'a> BufferView<'a> {
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    pub fn size(&self) -> vk::DeviceSize {
        self.buffer.size
    }

    /// Obtain a slice to the mapped memory of this buffer.
    /// # Errors
    /// Fails if this buffer is not mappable (not `HOST_VISIBLE`).
    pub fn mapped_slice<T>(&mut self) -> Result<&mut [T]> {
        ensure!(self.buffer.is_mapped(), anyhow!("Not mapped!"));

        let pointer = self.buffer.allocation_info.mapped_data;
        let offset_pointer = unsafe { pointer.offset(self.offset as isize) };
        Ok(unsafe {
            std::slice::from_raw_parts_mut(
                offset_pointer.cast::<T>(),
                self.size as usize / std::mem::size_of::<T>(),
            )
        })
    }
}

/// A image and it's memory allocation.
pub struct Image {
    image: vk::Image,
    image_view: vk::ImageView,
    allocation: vk_mem_alloc::Allocation,
    allocation_info: vk_mem_alloc::AllocationInfo,
}

impl Image {
    pub fn image(&self) -> vk::Image {
        self.image
    }

    pub fn image_view(&self) -> vk::ImageView {
        self.image_view
    }
}

new_key_type! {
    /// Used to access buffers in a ResourceManager.
    pub struct BufferHandle;
    /// Used to access images in a ResourceManager.
    pub struct ImageHandle;
}
