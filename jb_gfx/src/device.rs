use std::ffi::CString;
use std::mem::size_of;
use std::{borrow::Cow, ffi::CStr};

use ash::extensions::khr::Synchronization2;
use ash::extensions::{
    ext::DebugUtils,
    khr::{DynamicRendering, Swapchain},
};
use ash::vk::{
    self, DebugUtilsObjectNameInfoEXT, DeviceSize, Handle, ObjectType, PipelineStageFlags2,
    SwapchainKHR,
};
use image::EncodableLayout;
use log::info;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use winit::window::Window;

use crate::resource;
use crate::resource::{ImageHandle, ResourceManager};

pub const FRAMES_IN_FLIGHT: usize = 2usize;

pub struct GraphicsDevice {
    instance: ash::Instance,
    pub size: winit::dpi::PhysicalSize<u32>,
    surface: vk::SurfaceKHR,
    surface_loader: ash::extensions::khr::Surface,
    surface_format: vk::SurfaceFormatKHR,
    surface_resolution: vk::Extent2D,
    pub vk_device: ash::Device,
    pdevice: vk::PhysicalDevice,
    pub resource_manager: ResourceManager,
    pub debug_utils_loader: DebugUtils,
    debug_call_back: vk::DebugUtilsMessengerEXT,
    swapchain: SwapchainKHR,
    swapchain_loader: Swapchain,
    pub present_images: Vec<vk::Image>,
    pub present_image_views: Vec<vk::ImageView>,
    pub render_image: ImageHandle,
    pub render_image_format: vk::Format,
    pub depth_image: ImageHandle,
    pub depth_image_format: vk::Format,
    pub graphics_queue: vk::Queue,
    pub graphics_command_pool: [vk::CommandPool; FRAMES_IN_FLIGHT],
    pub graphics_command_buffer: [vk::CommandBuffer; FRAMES_IN_FLIGHT],
    pub draw_commands_reuse_fence: [vk::Fence; FRAMES_IN_FLIGHT],
    pub rendering_complete_semaphore: [vk::Semaphore; FRAMES_IN_FLIGHT],
    pub present_complete_semaphore: [vk::Semaphore; FRAMES_IN_FLIGHT],
    pub upload_context: UploadContext,
    pub default_sampler: vk::Sampler,
    frame_number: usize,
}

impl GraphicsDevice {
    pub fn new(window: &Window) -> Self {
        let size = window.inner_size();

        let entry = ash::Entry::linked();
        let app_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"Rust Renderer\0") };
        let app_info = vk::ApplicationInfo::builder()
            .application_name(app_name)
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(app_name)
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            .api_version(vk::make_api_version(0, 1, 3, 0));

        let mut instance_extensions =
            ash_window::enumerate_required_extensions(window.raw_display_handle())
                .unwrap()
                .to_vec();

        instance_extensions.push(DebugUtils::name().as_ptr());

        let instance_create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&instance_extensions);

        let instance = unsafe {
            entry
                .create_instance(&instance_create_info, None)
                .expect("Instance Creation Error")
        };

        let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING, //        | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(vulkan_debug_callback));

        let debug_utils_loader = DebugUtils::new(&entry, &instance);
        let debug_call_back =
            unsafe { debug_utils_loader.create_debug_utils_messenger(&debug_info, None) }.unwrap();

        let surface = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                window.raw_display_handle(),
                window.raw_window_handle(),
                None,
            )
        }
        .unwrap();

        let surface_loader = ash::extensions::khr::Surface::new(&entry, &instance);

        let mut sync_2_feature =
            vk::PhysicalDeviceSynchronization2Features::builder().synchronization2(true);

        let mut dynamic_rendering_feature =
            vk::PhysicalDeviceDynamicRenderingFeatures::builder().dynamic_rendering(true);

        let pdevices =
            unsafe { instance.enumerate_physical_devices() }.expect("Physical device error");
        let (pdevice, queue_family_index) = pdevices
            .iter()
            .find_map(|pdevice| {
                unsafe { instance.get_physical_device_queue_family_properties(*pdevice) }
                    .iter()
                    .enumerate()
                    .find_map(|(index, info)| {
                        let supports_graphic_and_surface =
                            info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                                && unsafe {
                                    surface_loader.get_physical_device_surface_support(
                                        *pdevice,
                                        index as u32,
                                        surface,
                                    )
                                }
                                .unwrap();
                        if supports_graphic_and_surface {
                            Some((*pdevice, index))
                        } else {
                            None
                        }
                    })
            })
            .expect("Couldn't find suitable device.");

        let queue_family_index = queue_family_index as u32;
        let device_extension_names_raw = [
            Swapchain::name().as_ptr(),
            DynamicRendering::name().as_ptr(),
            Synchronization2::name().as_ptr(),
        ];
        let features = vk::PhysicalDeviceFeatures {
            shader_clip_distance: 1,
            ..Default::default()
        };
        let mut descriptor_indexing_features =
            vk::PhysicalDeviceDescriptorIndexingFeatures::builder()
                .shader_sampled_image_array_non_uniform_indexing(true)
                .descriptor_binding_partially_bound(true)
                .descriptor_binding_variable_descriptor_count(true)
                .runtime_descriptor_array(true);

        let priorities = [1.0];

        let queue_info = vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities);

        let device_create_info = vk::DeviceCreateInfo::builder()
            .push_next(&mut descriptor_indexing_features)
            .push_next(&mut sync_2_feature)
            .push_next(&mut dynamic_rendering_feature)
            .queue_create_infos(std::slice::from_ref(&queue_info))
            .enabled_extension_names(&device_extension_names_raw)
            .enabled_features(&features);

        let device = unsafe { instance.create_device(pdevice, &device_create_info, None) }.unwrap();

        let mut resource_manager = ResourceManager::new(&instance, &pdevice, device.clone());

        let graphics_queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let surface_format =
            unsafe { surface_loader.get_physical_device_surface_formats(pdevice, surface) }
                .unwrap()
                .into_iter()
                .find(|&x| {
                    x.format == vk::Format::B8G8R8A8_SRGB || x.format == vk::Format::R8G8B8A8_SRGB
                })
                .unwrap();

        let surface_capabilities =
            unsafe { surface_loader.get_physical_device_surface_capabilities(pdevice, surface) }
                .unwrap();
        let mut desired_image_count = surface_capabilities.min_image_count + 1;
        if surface_capabilities.max_image_count > 0
            && desired_image_count > surface_capabilities.max_image_count
        {
            desired_image_count = surface_capabilities.max_image_count;
        }
        let surface_resolution = match surface_capabilities.current_extent.width {
            u32::MAX => vk::Extent2D {
                width: size.width,
                height: size.height,
            },
            _ => surface_capabilities.current_extent,
        };
        let pre_transform = if surface_capabilities
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface_capabilities.current_transform
        };
        let present_modes =
            unsafe { surface_loader.get_physical_device_surface_present_modes(pdevice, surface) }
                .unwrap();
        let present_mode = present_modes
            .iter()
            .cloned()
            .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);
        let swapchain_loader = Swapchain::new(&instance, &device);

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface)
            .min_image_count(desired_image_count)
            .image_color_space(surface_format.color_space)
            .image_format(surface_format.format)
            .image_extent(surface_resolution)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_array_layers(1);

        let swapchain =
            unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None) }.unwrap();

        let present_images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }.unwrap();
        let present_image_views: Vec<vk::ImageView> = present_images
            .iter()
            .map(|&image| {
                let create_view_info = vk::ImageViewCreateInfo::builder()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(surface_format.format)
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image(image);
                unsafe { device.create_image_view(&create_view_info, None) }.unwrap()
            })
            .collect();

        let pool_create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);

        let graphics_command_pool = [
            unsafe { device.create_command_pool(&pool_create_info, None) }.unwrap(),
            unsafe { device.create_command_pool(&pool_create_info, None) }.unwrap(),
        ];

        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(graphics_command_pool[0])
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers =
            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }.unwrap();

        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(graphics_command_pool[1])
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers_two =
            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }.unwrap();

        let graphics_command_buffer = [command_buffers[0], command_buffers_two[0]];

        let upload_command_pool = {
            let pool_create_info = vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);

            unsafe { device.create_command_pool(&pool_create_info, None) }.unwrap()
        };

        let upload_command_buffer = {
            let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(upload_command_pool)
                .level(vk::CommandBufferLevel::PRIMARY);

            let command_buffers =
                unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }.unwrap();

            command_buffers[0]
        };

        let fence_create_info =
            vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);

        let draw_commands_reuse_fence = [
            unsafe { device.create_fence(&fence_create_info, None) }.expect("Create fence failed."),
            unsafe { device.create_fence(&fence_create_info, None) }.expect("Create fence failed."),
        ];

        let upload_fence = {
            let fence_create_info = vk::FenceCreateInfo::builder();

            unsafe { device.create_fence(&fence_create_info, None) }.expect("Create fence failed.")
        };

        let semaphore_create_info = vk::SemaphoreCreateInfo::default();

        let present_complete_semaphore = [
            unsafe { device.create_semaphore(&semaphore_create_info, None) }.unwrap(),
            unsafe { device.create_semaphore(&semaphore_create_info, None) }.unwrap(),
        ];
        let rendering_complete_semaphore = [
            unsafe { device.create_semaphore(&semaphore_create_info, None) }.unwrap(),
            unsafe { device.create_semaphore(&semaphore_create_info, None) }.unwrap(),
        ];

        let render_image_format = vk::Format::R8G8B8A8_SRGB;
        let render_image = {
            let render_image_create_info = vk::ImageCreateInfo::builder()
                .format(render_image_format)
                .usage(
                    vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                )
                .extent(vk::Extent3D {
                    width: surface_resolution.width,
                    height: surface_resolution.height,
                    depth: 1,
                })
                .image_type(vk::ImageType::TYPE_2D)
                .array_layers(1u32)
                .mip_levels(1u32)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL);

            resource_manager
                .create_image(&render_image_create_info, resource::ImageAspectType::Color)
        };

        {
            let object_name = CString::new("Render Image").unwrap();
            let pipeline_debug_info = DebugUtilsObjectNameInfoEXT::builder()
                .object_type(ObjectType::IMAGE)
                .object_handle(
                    resource_manager
                        .get_image(render_image)
                        .unwrap()
                        .image
                        .as_raw(),
                )
                .object_name(object_name.as_ref());

            unsafe {
                debug_utils_loader
                    .set_debug_utils_object_name(device.handle(), &pipeline_debug_info)
                    .expect("Named object");
            }
        }

        let depth_image_format = vk::Format::D32_SFLOAT;
        let depth_image = {
            let depth_image_create_info = vk::ImageCreateInfo::builder()
                .format(depth_image_format)
                .usage(
                    vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                )
                .extent(vk::Extent3D {
                    width: surface_resolution.width,
                    height: surface_resolution.height,
                    depth: 1,
                })
                .image_type(vk::ImageType::TYPE_2D)
                .array_layers(1u32)
                .mip_levels(1u32)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL);

            resource_manager
                .create_image(&depth_image_create_info, resource::ImageAspectType::Depth)
        };

        {
            let object_name = CString::new("Depth Image").unwrap();
            let pipeline_debug_info = DebugUtilsObjectNameInfoEXT::builder()
                .object_type(ObjectType::IMAGE)
                .object_handle(
                    resource_manager
                        .get_image(depth_image)
                        .unwrap()
                        .image
                        .as_raw(),
                )
                .object_name(object_name.as_ref());

            unsafe {
                debug_utils_loader
                    .set_debug_utils_object_name(device.handle(), &pipeline_debug_info)
                    .expect("Named object");
            }
        }

        let default_sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT);

            unsafe { device.create_sampler(&sampler_info, None).unwrap() }
        };

        let upload_context = UploadContext {
            command_pool: upload_command_pool,
            command_buffer: upload_command_buffer,
            fence: upload_fence,
            queue: graphics_queue,
        };

        info!("Device created");

        Self {
            instance,
            size,
            surface,
            surface_loader,
            surface_format,
            surface_resolution,
            vk_device: device,
            pdevice,
            resource_manager,
            debug_utils_loader,
            debug_call_back,
            swapchain,
            swapchain_loader,
            present_images,
            present_image_views,
            render_image,
            render_image_format,
            depth_image,
            depth_image_format,
            graphics_queue,
            graphics_command_pool,
            graphics_command_buffer,
            draw_commands_reuse_fence,
            rendering_complete_semaphore,
            present_complete_semaphore,
            upload_context,
            default_sampler,
            frame_number: 0usize,
        }
    }

    pub fn frame_number(&self) -> usize {
        self.frame_number
    }

    pub fn buffered_resource_number(&self) -> usize {
        self.frame_number % 2
    }

    pub fn start_frame(&mut self) -> u32 {
        unsafe {
            self.vk_device.wait_for_fences(
                &[self.draw_commands_reuse_fence[self.buffered_resource_number()]],
                true,
                u64::MAX,
            )
        }
        .expect("Wait for fence failed.");

        unsafe {
            self.vk_device
                .reset_fences(&[self.draw_commands_reuse_fence[self.buffered_resource_number()]])
        }
        .expect("Reset fences failed.");

        unsafe {
            self.vk_device.reset_command_buffer(
                self.graphics_command_buffer[self.buffered_resource_number()],
                vk::CommandBufferResetFlags::RELEASE_RESOURCES,
            )
        }
        .expect("Reset command buffer failed.");

        let (present_index, _) = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.present_complete_semaphore[self.buffered_resource_number()],
                vk::Fence::null(),
            )
        }
        .unwrap();

        present_index
    }

    pub fn end_frame(&mut self, present_index: u32) {
        let wait_semaphores = [self.rendering_complete_semaphore[self.buffered_resource_number()]];
        let swapchains = [self.swapchain];
        let image_indices = [present_index];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        unsafe {
            self.swapchain_loader
                .queue_present(self.graphics_queue, &present_info)
        }
        .unwrap();

        self.frame_number += 1usize;
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0u32 || new_size.height == 0u32 || new_size == self.size {
            return;
        }

        unsafe { self.vk_device.device_wait_idle() }.unwrap();
        self.size = new_size;

        // Destroy old swapchain

        unsafe {
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);

            for &image_view in self.present_image_views.iter() {
                self.vk_device.destroy_image_view(image_view, None);
            }
        }

        // Create swapchain
        // TODO : Possibly better way to wrap this up

        let surface_capabilities = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(self.pdevice, self.surface)
        }
        .unwrap();
        let mut desired_image_count = surface_capabilities.min_image_count + 1;
        if surface_capabilities.max_image_count > 0
            && desired_image_count > surface_capabilities.max_image_count
        {
            desired_image_count = surface_capabilities.max_image_count;
        }
        self.surface_resolution = match surface_capabilities.current_extent.width {
            u32::MAX => vk::Extent2D {
                width: self.size.width,
                height: self.size.height,
            },
            _ => surface_capabilities.current_extent,
        };
        let pre_transform = if surface_capabilities
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            surface_capabilities.current_transform
        };
        let present_modes = unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(self.pdevice, self.surface)
        }
        .unwrap();
        let present_mode = present_modes
            .iter()
            .cloned()
            .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(self.surface)
            .min_image_count(desired_image_count)
            .image_color_space(self.surface_format.color_space)
            .image_format(self.surface_format.format)
            .image_extent(self.surface_resolution)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_array_layers(1);

        self.swapchain = unsafe {
            self.swapchain_loader
                .create_swapchain(&swapchain_create_info, None)
        }
        .unwrap();

        self.present_images =
            unsafe { self.swapchain_loader.get_swapchain_images(self.swapchain) }.unwrap();
        self.present_image_views = self
            .present_images
            .iter()
            .map(|&image| {
                let create_view_info = vk::ImageViewCreateInfo::builder()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(self.surface_format.format)
                    .components(vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    })
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image(image);
                unsafe { self.vk_device.create_image_view(&create_view_info, None) }.unwrap()
            })
            .collect();

        // Recreate render image

        let render_image_create_info = vk::ImageCreateInfo::builder()
            .format(vk::Format::R8G8B8A8_SRGB)
            .usage(
                vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )
            .extent(vk::Extent3D {
                width: self.surface_resolution.width,
                height: self.surface_resolution.height,
                depth: 1,
            })
            .image_type(vk::ImageType::TYPE_2D)
            .array_layers(1u32)
            .mip_levels(1u32)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL);

        self.resource_manager.destroy_image(self.render_image);

        self.render_image = self
            .resource_manager
            .create_image(&render_image_create_info, resource::ImageAspectType::Color);

        info!("Recreating swapchain.");
    }

    pub(crate) fn load_image(
        &mut self,
        img_bytes: &[u8],
        img_width: u32,
        img_height: u32,
    ) -> Result<ImageHandle, String> {
        let img_size = (img_width * img_height * 4u32) as DeviceSize;
        //(size_of::<u8>() * img_bytes.len()) as u64;

        let staging_buffer_create_info = vk::BufferCreateInfo {
            size: img_size,
            usage: vk::BufferUsageFlags::TRANSFER_SRC,
            ..Default::default()
        };

        let staging_buffer_allocation_create_info = vk_mem_alloc::AllocationCreateInfo {
            flags: vk_mem_alloc::AllocationCreateFlags::MAPPED
                | vk_mem_alloc::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            usage: vk_mem_alloc::MemoryUsage::AUTO,
            ..Default::default()
        };

        let staging_buffer = self.resource_manager.create_buffer(
            &staging_buffer_create_info,
            &staging_buffer_allocation_create_info,
        );

        unsafe {
            self.resource_manager
                .get_buffer(staging_buffer)
                .unwrap()
                .allocation_info
                .mapped_data
                .copy_from_nonoverlapping(img_bytes.as_ptr().cast(), img_bytes.len());
        };

        let image_create_info = vk::ImageCreateInfo::builder()
            .format(vk::Format::R8G8B8A8_SRGB)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
            .extent(vk::Extent3D {
                width: img_width,
                height: img_height,
                depth: 1,
            })
            .image_type(vk::ImageType::TYPE_2D)
            .array_layers(1u32)
            .mip_levels(1u32)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL);

        let image = self
            .resource_manager
            .create_image(&image_create_info, resource::ImageAspectType::Color);

        self.upload_context.immediate_submit(
            &mut self.vk_device,
            &mut self.resource_manager,
            |device, resource_manager, cmd| {
                let range = vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0u32)
                    .level_count(1u32)
                    .base_array_layer(0u32)
                    .layer_count(1u32);

                {
                    let image_barrier_transfer_to = vk::ImageMemoryBarrier2::builder()
                        .src_stage_mask(PipelineStageFlags2::NONE)
                        .src_access_mask(vk::AccessFlags2::NONE)
                        .dst_stage_mask(PipelineStageFlags2::TRANSFER)
                        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .image(resource_manager.get_image(image).unwrap().image)
                        .subresource_range(*range);

                    let image_memory_barriers = [*image_barrier_transfer_to];
                    let image_dependency_info =
                        vk::DependencyInfo::builder().image_memory_barriers(&image_memory_barriers);

                    unsafe {
                        device.cmd_pipeline_barrier2(*cmd, &image_dependency_info);
                    }
                }

                let copy_region = vk::BufferImageCopy::builder()
                    .buffer_offset(0u64)
                    .buffer_row_length(0u32)
                    .buffer_image_height(0u32)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0u32,
                        base_array_layer: 0u32,
                        layer_count: 1u32,
                    })
                    .image_extent(vk::Extent3D {
                        width: img_width,
                        height: img_height,
                        depth: 1,
                    });

                unsafe {
                    device.cmd_copy_buffer_to_image(
                        *cmd,
                        resource_manager.get_buffer(staging_buffer).unwrap().buffer,
                        resource_manager.get_image(image).unwrap().image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[*copy_region],
                    );
                }

                {
                    let image_barrier_transfer = vk::ImageMemoryBarrier2::builder()
                        .src_stage_mask(PipelineStageFlags2::TRANSFER)
                        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                        .dst_stage_mask(PipelineStageFlags2::FRAGMENT_SHADER)
                        .dst_access_mask(vk::AccessFlags2::SHADER_READ)
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .image(resource_manager.get_image(image).unwrap().image)
                        .subresource_range(*range);

                    let image_memory_barriers = [*image_barrier_transfer];
                    let image_dependency_info =
                        vk::DependencyInfo::builder().image_memory_barriers(&image_memory_barriers);

                    unsafe {
                        device.cmd_pipeline_barrier2(*cmd, &image_dependency_info);
                    }
                }
            },
        );

        info!("Copied to Image.");

        Ok(image)
    }
}

impl Drop for GraphicsDevice {
    fn drop(&mut self) {
        unsafe {
            self.vk_device.device_wait_idle().unwrap();
            self.resource_manager.destroy_resources();
            self.vk_device.destroy_sampler(self.default_sampler, None);
            for semaphore in self.present_complete_semaphore.into_iter() {
                self.vk_device.destroy_semaphore(semaphore, None);
            }
            for semaphore in self.rendering_complete_semaphore.into_iter() {
                self.vk_device.destroy_semaphore(semaphore, None);
            }
            self.vk_device
                .destroy_fence(self.upload_context.fence, None);
            for fence in self.draw_commands_reuse_fence.into_iter() {
                self.vk_device.destroy_fence(fence, None);
            }
            for &image_view in self.present_image_views.iter() {
                self.vk_device.destroy_image_view(image_view, None);
            }
            self.vk_device
                .destroy_command_pool(self.upload_context.command_pool, None);
            for pool in self.graphics_command_pool.into_iter() {
                self.vk_device.destroy_command_pool(pool, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            self.vk_device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.debug_utils_loader
                .destroy_debug_utils_messenger(self.debug_call_back, None);
            self.instance.destroy_instance(None);
        }
    }
}

pub struct UploadContext {
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
    queue: vk::Queue,
}

impl UploadContext {
    // TODO: Could remove passing of ResourceManager and force users to get their buffer references etc beforehand
    pub fn immediate_submit<
        F: Fn(&mut ash::Device, &mut ResourceManager, &mut vk::CommandBuffer),
    >(
        &mut self,
        vk_device: &mut ash::Device,
        resource_manager: &mut ResourceManager,
        function: F,
    ) {
        let mut cmd = self.command_buffer;

        let cmd_begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe { vk_device.begin_command_buffer(cmd, &cmd_begin_info) }
            .expect("Upload start command buffer failed.");

        function(vk_device, resource_manager, &mut cmd);

        unsafe { vk_device.end_command_buffer(cmd) }.expect("Upload end command buffer failed.");

        let command_buffers = [cmd];
        let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);

        let submits = [*submit_info];
        unsafe { vk_device.queue_submit(self.queue, &submits, self.fence) }
            .expect("Upload submission failed.");

        unsafe { vk_device.wait_for_fences(&[self.fence], true, u64::MAX) }
            .expect("Wait for fence failed.");

        unsafe { vk_device.reset_fences(&[self.fence]) }.expect("Reset upload fence failed.");

        unsafe {
            vk_device.reset_command_pool(
                self.command_pool,
                vk::CommandPoolResetFlags::RELEASE_RESOURCES,
            )
        }
        .expect("Reset upload command pool failed");
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    println!(
        "{:?}:\n{:?} [{} ({})] : {}\n",
        message_severity,
        message_type,
        message_id_name,
        &message_id_number.to_string(),
        message,
    );

    vk::FALSE
}
