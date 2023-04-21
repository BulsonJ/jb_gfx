use std::cell::RefCell;
use std::sync::Arc;
use anyhow::{anyhow, ensure, Result};
use ash::vk;
use ash::vk::Format;
use log::trace;
use slotmap::{self, new_key_type, SlotMap};

/// Used to create Buffers and Images.
///
/// Currently, not great code as gets around lifetimes by cloning ash structs into it.
/// In the future, refactor to use Rust lifetimes to ensure that a ResourceManager does not outlive the
/// ash structs that it takes in.
pub struct ResourceManager {
    device: Arc<ash::Device>,
    allocator: vk_mem_alloc::Allocator,
    buffers: RefCell<SlotMap<BufferHandle, Buffer>>,
    images: RefCell<SlotMap<ImageHandle, Image>>,
}

#[derive(Copy, Clone)]
pub enum BufferStorageType {
    Device,
    HostLocal,
}

#[derive(Copy, Clone)]
pub struct BufferCreateInfo {
    pub size: usize,
    pub usage: vk::BufferUsageFlags,
    pub storage_type: BufferStorageType,
}

impl From<BufferCreateInfo> for vk::BufferCreateInfo {
    fn from(value: BufferCreateInfo) -> Self {
        Self {
            size: value.size as vk::DeviceSize,
            usage: value.usage,
            ..Default::default()
        }
    }
}

impl From<BufferCreateInfo> for vk_mem_alloc::AllocationCreateInfo {
    fn from(value: BufferCreateInfo) -> Self {
        let flags = match value.storage_type {
            BufferStorageType::Device => vk_mem_alloc::AllocationCreateFlags::NONE,
            BufferStorageType::HostLocal => {
                vk_mem_alloc::AllocationCreateFlags::MAPPED
                    | vk_mem_alloc::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
            }
        };
        Self {
            flags,
            usage: vk_mem_alloc::MemoryUsage::AUTO,
            ..Default::default()
        }
    }
}

impl ResourceManager {
    pub fn new(
        instance: &ash::Instance,
        pdevice: &vk::PhysicalDevice,
        device: Arc<ash::Device>,
    ) -> Self {
        let allocator =
            unsafe { vk_mem_alloc::create_allocator(instance, *pdevice, &device, None) }.unwrap();

        Self {
            device,
            allocator,
            buffers: RefCell::new(SlotMap::default()),
            images: RefCell::new(SlotMap::default()),
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
    pub fn create_buffer(&self, buffer_create_info: &BufferCreateInfo) -> BufferHandle {
        let create_info: vk::BufferCreateInfo = (*buffer_create_info).into();
        let alloc_info: vk_mem_alloc::AllocationCreateInfo = (*buffer_create_info).into();

        // Create the buffer
        let (vk_buffer, allocation, allocation_info) =
            unsafe { vk_mem_alloc::create_buffer(self.allocator, &create_info, &alloc_info) }
                .unwrap();

        let buffer = Buffer {
            buffer: vk_buffer,
            size: buffer_create_info.size as vk::DeviceSize,
            allocation,
            allocation_info,
        };

        trace!("Buffer created. [Size: {} bytes]", buffer_create_info.size);

        self.buffers.borrow_mut().insert(buffer)
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
    pub fn get_buffer(&self, handle: BufferHandle) -> Option<Buffer> {
        self.buffers.borrow().get(handle).cloned()
    }


    pub fn destroy_buffer(&self, handle: BufferHandle) {
        let buffer = self.buffers.borrow_mut().remove(handle).unwrap();
        unsafe {
            self.device.destroy_buffer(buffer.buffer, None);
            vk_mem_alloc::destroy_buffer(self.allocator, buffer.buffer, buffer.allocation)
        };
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
    pub fn create_image(&self, image_create_info: &vk::ImageCreateInfo) -> ImageHandle {
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
                aspect_mask: get_image_aspect_flags_from_format(image_create_info.format),
                level_count: image_create_info.mip_levels,
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
            image_usage: image_create_info.usage,
            image_format: image_create_info.format,
            mip_levels: image_create_info.mip_levels,
            allocation,
            allocation_info,
        };

        trace!(
            "Image created. [Dim: {},{}]",
            image_create_info.extent.width,
            image_create_info.extent.height
        );

        self.images.borrow_mut().insert(image)
    }

    pub fn get_image(&self, handle: ImageHandle) -> Option<Image> {
        self.images.borrow().get(handle).cloned()
    }

    pub fn destroy_image(&self, handle: ImageHandle) {
        let image = self.images.borrow_mut().remove(handle).unwrap();
        unsafe {
            self.device.destroy_image_view(image.image_view, None);
            vk_mem_alloc::destroy_image(self.allocator, image.image, image.allocation)
        };
    }
    pub fn destroy_resources(&self) {
        unsafe {
            for buffer in self.buffers.borrow_mut().iter_mut() {
                vk_mem_alloc::destroy_buffer(self.allocator, buffer.1.buffer, buffer.1.allocation);
            }
            for image in self.images.borrow_mut().iter_mut() {
                self.device.destroy_image_view(image.1.image_view, None);
                vk_mem_alloc::destroy_image(self.allocator, image.1.image, image.1.allocation);
            }

            vk_mem_alloc::destroy_allocator(self.allocator)
        };
    }
}

/// A buffer and it's memory allocation.
#[derive(Copy, Clone)]
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

    pub fn view<T>(&mut self) -> BufferView<T> {
        let size = self.size;
        BufferView {
            buffer: self,
            offset: 0,
            size,
            data_type: std::marker::PhantomData::default(),
        }
    }

    pub fn view_custom<T>(&mut self, offset: usize, count: usize) -> Result<BufferView<T>> {
        let type_size = std::mem::size_of::<T>();
        let offset = offset * type_size;
        let size = count * type_size;

        ensure!(
            size <= self.size as usize,
            anyhow!(
                "Size of View[{}] exceeded size of buffer[{}]!",
                size,
                self.size
            )
        );
        ensure!(
            offset <= self.size as usize,
            anyhow!(
                "Offset of View[{}] exceeded size of buffer[{}]!",
                offset,
                self.size
            )
        );
        ensure!(
            offset + size <= self.size as usize,
            anyhow!(
                "BufferView[{} + {}] would go past end of buffer[{}]!",
                offset,
                size,
                self.size
            )
        );

        Ok(BufferView {
            buffer: self,
            offset: offset as vk::DeviceSize,
            size: size as vk::DeviceSize,
            data_type: std::marker::PhantomData::default(),
        })
    }
}

pub struct BufferView<'a, T> {
    buffer: &'a mut Buffer,
    offset: vk::DeviceSize,
    size: vk::DeviceSize,
    data_type: std::marker::PhantomData<T>,
}

impl<'a, T> BufferView<'a, T> {
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    pub fn size(&self) -> vk::DeviceSize {
        self.buffer.size
    }

    /// Obtain a slice to the mapped memory of this buffer.
    /// # Errors
    /// Fails if this buffer is not mappable (not `HOST_VISIBLE`).
    pub fn mapped_slice(&self) -> Result<&mut [T]> {
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
#[derive(Copy, Clone)]
pub struct Image {
    image: vk::Image,
    image_usage: vk::ImageUsageFlags,
    image_format: vk::Format,
    image_view: vk::ImageView,
    mip_levels: u32,
    allocation: vk_mem_alloc::Allocation,
    allocation_info: vk_mem_alloc::AllocationInfo,
}

impl Image {
    pub fn image(&self) -> vk::Image {
        self.image
    }

    pub fn format(&self) -> vk::Format {
        self.image_format
    }

    pub fn aspect_flags(&self) -> vk::ImageAspectFlags {
        get_image_aspect_flags_from_format(self.image_format)
    }

    pub fn mip_levels(&self) -> u32 {
        self.mip_levels
    }

    pub fn usage(&self) -> vk::ImageUsageFlags {
        self.image_usage
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

fn get_image_aspect_flags_from_format(format: Format) -> vk::ImageAspectFlags {
    let mut flags = vk::ImageAspectFlags::empty();

    match format {
        Format::R8G8B8A8_SRGB | Format::R8G8B8A8_UNORM => flags |= vk::ImageAspectFlags::COLOR,
        Format::D32_SFLOAT => flags |= vk::ImageAspectFlags::DEPTH,
        _ => {
            todo!()
        }
    }

    flags
}
