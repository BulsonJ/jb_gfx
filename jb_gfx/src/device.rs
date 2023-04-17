use std::ffi::CString;
use std::{borrow::Cow, ffi::CStr};

use anyhow::{ensure, Result};
use ash::extensions::khr::Synchronization2;
use ash::extensions::{
    ext::DebugUtils,
    khr::{DynamicRendering, Swapchain},
};
use ash::vk::{
    self, DebugUtilsObjectNameInfoEXT, DeviceSize, Handle, ImageLayout, ObjectType, SwapchainKHR,
};
use log::info;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use winit::window::Window;

use crate::barrier::{ImageBarrier, ImageBarrierBuilder, ImageHandleType};
use crate::bindless::{BindlessImage, BindlessManager};
use crate::resource::{
    BufferCreateInfo, BufferHandle, BufferStorageType, ImageHandle, ResourceManager,
};
use crate::targets::{RenderImageType, RenderTargetHandle, RenderTargetSize, RenderTargets};

pub const FRAMES_IN_FLIGHT: usize = 2usize;
pub const SHADOWMAP_SIZE: u32 = 4096u32;

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
    pub render_image: RenderTargetHandle,
    pub depth_image: RenderTargetHandle,
    pub graphics_queue: vk::Queue,
    pub graphics_command_pool: [vk::CommandPool; FRAMES_IN_FLIGHT],
    pub graphics_command_buffer: [vk::CommandBuffer; FRAMES_IN_FLIGHT],
    pub draw_commands_reuse_fence: [vk::Fence; FRAMES_IN_FLIGHT],
    pub rendering_complete_semaphore: [vk::Semaphore; FRAMES_IN_FLIGHT],
    pub present_complete_semaphore: [vk::Semaphore; FRAMES_IN_FLIGHT],
    pub upload_context: UploadContext,
    pub default_sampler: vk::Sampler,
    frame_number: usize,
    images_to_upload: Vec<ImageToUpload>,
    buffers_to_delete: Vec<(BufferHandle, usize)>,
    render_targets: RenderTargets,
    bindless_descriptor_set_layout: vk::DescriptorSetLayout,
    bindless_descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    bindless_manager: BindlessManager,
    bindless_descriptor_pool: vk::DescriptorPool,
    pub directional_light_shadow_image: RenderTargetHandle,
    pub shadow_sampler: vk::Sampler,
}

impl GraphicsDevice {
    pub fn new(window: &Window) -> Result<Self> {
        profiling::scope!("GraphicsDevice::new");

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
            ash_window::enumerate_required_extensions(window.raw_display_handle())?.to_vec();

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
            unsafe { debug_utils_loader.create_debug_utils_messenger(&debug_info, None) }?;

        let surface = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                window.raw_display_handle(),
                window.raw_window_handle(),
                None,
            )
        }?;

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

        let device = unsafe { instance.create_device(pdevice, &device_create_info, None) }?;

        let mut resource_manager = ResourceManager::new(&instance, &pdevice, device.clone());

        let graphics_queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let surface_format =
            unsafe { surface_loader.get_physical_device_surface_formats(pdevice, surface) }?
                .into_iter()
                .find(|&x| {
                    x.format == vk::Format::B8G8R8A8_SRGB || x.format == vk::Format::R8G8B8A8_SRGB
                })
                .unwrap();

        let surface_capabilities =
            unsafe { surface_loader.get_physical_device_surface_capabilities(pdevice, surface) }?;
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
            unsafe { surface_loader.get_physical_device_surface_present_modes(pdevice, surface) }?;
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

        let swapchain = unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None) }?;

        let present_images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }?;
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
            unsafe { device.create_command_pool(&pool_create_info, None) }?,
            unsafe { device.create_command_pool(&pool_create_info, None) }?,
        ];

        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(graphics_command_pool[0])
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers =
            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }?;

        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(graphics_command_pool[1])
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers_two =
            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }?;

        let graphics_command_buffer = [command_buffers[0], command_buffers_two[0]];

        let upload_command_pool = {
            let pool_create_info = vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);

            unsafe { device.create_command_pool(&pool_create_info, None) }?
        };

        let upload_command_buffer = {
            let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(upload_command_pool)
                .level(vk::CommandBufferLevel::PRIMARY);

            let command_buffers =
                unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }?;

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
            unsafe { device.create_semaphore(&semaphore_create_info, None) }?,
            unsafe { device.create_semaphore(&semaphore_create_info, None) }?,
        ];
        let rendering_complete_semaphore = [
            unsafe { device.create_semaphore(&semaphore_create_info, None) }?,
            unsafe { device.create_semaphore(&semaphore_create_info, None) }?,
        ];

        let default_sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .min_lod(0.0f32)
                .max_lod(vk::LOD_CLAMP_NONE);

            unsafe { device.create_sampler(&sampler_info, None)? }
        };

        let shadow_sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .min_lod(0.0f32)
                .max_lod(1.0f32);

            unsafe { device.create_sampler(&sampler_info, None)? }
        };

        let upload_context = UploadContext {
            command_pool: upload_command_pool,
            command_buffer: upload_command_buffer,
            fence: upload_fence,
            queue: graphics_queue,
        };

        // Create descriptor pool

        let pool_sizes = [
            *vk::DescriptorPoolSize::builder()
                .descriptor_count(100u32)
                .ty(vk::DescriptorType::UNIFORM_BUFFER),
            *vk::DescriptorPoolSize::builder()
                .descriptor_count(100u32)
                .ty(vk::DescriptorType::STORAGE_BUFFER),
            *vk::DescriptorPoolSize::builder()
                .descriptor_count(1000u32)
                .ty(vk::DescriptorType::SAMPLER),
            *vk::DescriptorPoolSize::builder()
                .descriptor_count(1000u32)
                .ty(vk::DescriptorType::SAMPLED_IMAGE),
        ];

        let pool_create_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(4u32)
            .pool_sizes(&pool_sizes);

        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_create_info, None) }?;

        // Create bindless set

        let bindless_binding_flags = [
            vk::DescriptorBindingFlags::empty(),
            vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT
                | vk::DescriptorBindingFlags::PARTIALLY_BOUND,
        ];

        let mut bindless_descriptor_set_binding_flags_create_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::builder()
                .binding_flags(&bindless_binding_flags);

        let bindless_descriptor_set_bindings = [
            *vk::DescriptorSetLayoutBinding::builder()
                .binding(0u32)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(2u32)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            *vk::DescriptorSetLayoutBinding::builder()
                .binding(1u32)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(100u32)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];

        let bindless_descriptor_set_layout_create_info =
            vk::DescriptorSetLayoutCreateInfo::builder()
                .push_next(&mut bindless_descriptor_set_binding_flags_create_info)
                .bindings(&bindless_descriptor_set_bindings);

        let bindless_descriptor_set_layout = unsafe {
            device.create_descriptor_set_layout(&bindless_descriptor_set_layout_create_info, None)
        }?;

        let bindless_descriptor_set = {
            let mut descriptor_set_counts =
                vk::DescriptorSetVariableDescriptorCountAllocateInfo::builder()
                    .descriptor_counts(&[100u32]);

            let set_layouts = [bindless_descriptor_set_layout];
            let create_info = vk::DescriptorSetAllocateInfo::builder()
                .push_next(&mut descriptor_set_counts)
                .descriptor_pool(descriptor_pool)
                .set_layouts(&set_layouts);

            let descriptor_sets = unsafe { device.allocate_descriptor_sets(&create_info) }?;
            let first = *descriptor_sets.get(0).unwrap();
            let descriptor_sets = unsafe { device.allocate_descriptor_sets(&create_info) }?;
            let second = *descriptor_sets.get(0).unwrap();

            [first, second]
        };

        let samplers = vec![default_sampler, shadow_sampler];
        let mut bindless_manager = BindlessManager::new(bindless_descriptor_set);
        bindless_manager.setup_samplers(&samplers, &device)?;

        let mut render_targets = RenderTargets::new((size.width, size.height));

        let render_image = render_targets.create_render_target(
            &mut resource_manager,
            vk::Format::R8G8B8A8_SRGB,
            RenderTargetSize::Fullscreen,
            RenderImageType::Colour,
        )?;
        let depth_image = render_targets.create_render_target(
            &mut resource_manager,
            vk::Format::D32_SFLOAT,
            RenderTargetSize::Fullscreen,
            RenderImageType::Depth,
        )?;
        let directional_light_shadow_image = render_targets.create_render_target(
            &mut resource_manager,
            vk::Format::D32_SFLOAT,
            RenderTargetSize::Static(SHADOWMAP_SIZE, SHADOWMAP_SIZE),
            RenderImageType::Depth,
        )?;
        bindless_manager.add_image_to_bindless(
            &device,
            &resource_manager,
            &render_targets,
            &BindlessImage::RenderTarget(directional_light_shadow_image),
        );

        let mut device = Self {
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
            depth_image,
            graphics_queue,
            graphics_command_pool,
            graphics_command_buffer,
            draw_commands_reuse_fence,
            rendering_complete_semaphore,
            present_complete_semaphore,
            upload_context,
            default_sampler,
            frame_number: 0usize,
            images_to_upload: Vec::default(),
            buffers_to_delete: Vec::default(),
            render_targets,
            bindless_descriptor_set_layout,
            bindless_descriptor_set,
            bindless_manager,
            bindless_descriptor_pool: descriptor_pool,
            directional_light_shadow_image,
            shadow_sampler,
        };

        for set in device.bindless_descriptor_set.iter() {
            device.set_vulkan_debug_name(
                set.as_raw(),
                ObjectType::DESCRIPTOR_SET,
                "Bindless Descriptor Set(0)",
            )?;
        }

        info!("Device Created");
        Ok(device)
    }

    pub fn frame_number(&self) -> usize {
        self.frame_number
    }

    pub fn buffered_resource_number(&self) -> usize {
        self.frame_number % 2
    }

    pub fn start_frame(&mut self) -> Result<u32> {
        profiling::scope!("Start Frame");

        unsafe {
            self.vk_device.wait_for_fences(
                &[self.draw_commands_reuse_fence[self.buffered_resource_number()]],
                true,
                u64::MAX,
            )
        }?;

        unsafe {
            self.vk_device
                .reset_fences(&[self.draw_commands_reuse_fence[self.buffered_resource_number()]])
        }?;

        unsafe {
            self.vk_device.reset_command_buffer(
                self.graphics_command_buffer[self.buffered_resource_number()],
                vk::CommandBufferResetFlags::RELEASE_RESOURCES,
            )
        }?;

        let (present_index, _) = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.present_complete_semaphore[self.buffered_resource_number()],
                vk::Fence::null(),
            )
        }?;

        // Begin command buffer

        let cmd_begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            self.vk_device.begin_command_buffer(
                self.graphics_command_buffer[self.buffered_resource_number()],
                &cmd_begin_info,
            )
        }?;

        // Delete old image buffers
        for buffer_to_delete in self.buffers_to_delete.iter_mut() {
            buffer_to_delete.1 -= 1;

            if buffer_to_delete.1 == 0 {
                self.resource_manager.destroy_buffer(buffer_to_delete.0);
            }
        }
        self.buffers_to_delete.clear();

        // Upload images
        // TODO: Remove buffers once upload has completed. Could use status enum so when fences are called, updates images that were submitted to being done.
        // Can then clear done images from vec.
        for image in self.images_to_upload.iter() {
            {
                ImageBarrierBuilder::default()
                    .add_image_barrier(ImageBarrier {
                        image: ImageHandleType::Image(image.image_handle),
                        dst_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                        dst_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                        new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        level_count: image.mip_levels,
                        ..Default::default()
                    })
                    .build(
                        self,
                        &self.graphics_command_buffer[self.buffered_resource_number()],
                    )?;

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
                        width: image.width,
                        height: image.height,
                        depth: 1,
                    });

                unsafe {
                    self.vk_device.cmd_copy_buffer_to_image(
                        self.graphics_command_buffer[self.buffered_resource_number()],
                        self.resource_manager
                            .get_buffer(image.buffer_handle)
                            .unwrap()
                            .buffer(),
                        self.resource_manager
                            .get_image(image.image_handle)
                            .unwrap()
                            .image(),
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[*copy_region],
                    );
                }

                self.buffers_to_delete.push((image.buffer_handle, 2));
            }

            // Generate mipmaps
            {
                let mut mip_width = image.width;
                let mut mip_height = image.height;

                for i in 1..image.mip_levels {
                    ImageBarrierBuilder::default()
                        .add_image_barrier(ImageBarrier {
                            image: ImageHandleType::Image(image.image_handle),
                            src_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                            src_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                            dst_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                            dst_access_mask: vk::AccessFlags2::TRANSFER_READ,
                            old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            new_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                            base_mip_level: i - 1,
                            level_count: 1,
                        })
                        .build(
                            self,
                            &self.graphics_command_buffer[self.buffered_resource_number()],
                        )?;

                    let image_blit = vk::ImageBlit::builder()
                        .src_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: i - 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        })
                        .src_offsets([
                            vk::Offset3D { x: 0, y: 0, z: 0 },
                            vk::Offset3D {
                                x: mip_width as i32,
                                y: mip_height as i32,
                                z: 1,
                            },
                        ])
                        .dst_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: i,
                            base_array_layer: 0,
                            layer_count: 1,
                        })
                        .dst_offsets([
                            vk::Offset3D { x: 0, y: 0, z: 0 },
                            vk::Offset3D {
                                x: if mip_width > 1 { mip_width / 2 } else { 1 } as i32,
                                y: if mip_height > 1 { mip_height / 2 } else { 1 } as i32,
                                z: 1,
                            },
                        ]);

                    let regions = [*image_blit];
                    let image_vk_handle = self
                        .resource_manager
                        .get_image(image.image_handle)
                        .unwrap()
                        .image();
                    unsafe {
                        self.vk_device.cmd_blit_image(
                            self.graphics_command_buffer[self.buffered_resource_number()],
                            image_vk_handle,
                            ImageLayout::TRANSFER_SRC_OPTIMAL,
                            image_vk_handle,
                            ImageLayout::TRANSFER_DST_OPTIMAL,
                            &regions,
                            vk::Filter::LINEAR,
                        )
                    }

                    ImageBarrierBuilder::default()
                        .add_image_barrier(ImageBarrier {
                            image: ImageHandleType::Image(image.image_handle),
                            src_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                            src_access_mask: vk::AccessFlags2::TRANSFER_READ,
                            dst_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
                            dst_access_mask: vk::AccessFlags2::SHADER_READ,
                            old_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                            new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            base_mip_level: i - 1,
                            level_count: 1,
                        })
                        .build(
                            self,
                            &self.graphics_command_buffer[self.buffered_resource_number()],
                        )?;

                    if mip_width > 1 {
                        mip_width /= 2
                    };
                    if mip_height > 1 {
                        mip_height /= 2
                    };
                }

                ImageBarrierBuilder::default()
                    .add_image_barrier(ImageBarrier {
                        image: ImageHandleType::Image(image.image_handle),
                        src_stage_mask: vk::PipelineStageFlags2::TRANSFER,
                        src_access_mask: vk::AccessFlags2::TRANSFER_WRITE,
                        dst_stage_mask: vk::PipelineStageFlags2::FRAGMENT_SHADER,
                        dst_access_mask: vk::AccessFlags2::SHADER_READ,
                        old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        base_mip_level: image.mip_levels - 1,
                        level_count: 1,
                    })
                    .build(
                        self,
                        &self.graphics_command_buffer[self.buffered_resource_number()],
                    )?;
            }
        }
        self.images_to_upload.clear();

        Ok(present_index)
    }

    pub fn end_frame(&mut self, present_index: u32) -> Result<()> {
        profiling::scope!("End Frame");

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
        }?;

        self.frame_number += 1usize;
        Ok(())
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) -> Result<()> {
        if new_size.width == 0u32 || new_size.height == 0u32 || new_size == self.size {
            return Ok(());
        }

        profiling::scope!("Resize");

        unsafe { self.vk_device.device_wait_idle() }?;
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
        }?;
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
        }?;
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
        }?;

        self.present_images =
            unsafe { self.swapchain_loader.get_swapchain_images(self.swapchain) }?;
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

        self.render_targets.recreate_render_targets(
            &mut self.resource_manager,
            (self.size.width, self.size.height),
        )?;

        info!("Recreating swapchain.");
        Ok(())
    }

    pub(crate) fn load_image(
        &mut self,
        img_bytes: &[u8],
        img_width: u32,
        img_height: u32,
        image_type: &ImageFormatType,
        mip_levels: u32,
    ) -> Result<ImageHandle> {
        profiling::scope!("Load Image");

        let img_size = (img_width * img_height * 4u32) as DeviceSize;

        let staging_buffer_create_info = BufferCreateInfo {
            size: img_size as usize,
            usage: vk::BufferUsageFlags::TRANSFER_SRC,
            storage_type: BufferStorageType::HostLocal,
        };

        let staging_buffer = self
            .resource_manager
            .create_buffer(&staging_buffer_create_info);

        self.resource_manager
            .get_buffer_mut(staging_buffer)
            .unwrap()
            .view()
            .mapped_slice()?
            .copy_from_slice(img_bytes);

        let format = {
            match image_type {
                ImageFormatType::Default => vk::Format::R8G8B8A8_SRGB,
                ImageFormatType::Normal => vk::Format::R8G8B8A8_UNORM,
            }
        };

        let image_create_info = vk::ImageCreateInfo::builder()
            .format(format)
            .usage(
                vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .extent(vk::Extent3D {
                width: img_width,
                height: img_height,
                depth: 1,
            })
            .image_type(vk::ImageType::TYPE_2D)
            .array_layers(1u32)
            .mip_levels(mip_levels)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL);

        let image = self.resource_manager.create_image(&image_create_info);

        self.images_to_upload.push(ImageToUpload {
            buffer_handle: staging_buffer,
            image_handle: image,
            width: img_width,
            height: img_height,
            mip_levels,
        });

        self.bindless_manager.add_image_to_bindless(
            &self.vk_device,
            &self.resource_manager,
            &self.render_targets,
            &BindlessImage::Image(image),
        );

        Ok(image)
    }

    pub fn immediate_submit<F: Fn(&mut GraphicsDevice, &mut vk::CommandBuffer) -> Result<()>>(
        &mut self,
        function: F,
    ) -> Result<()> {
        profiling::scope!("Immediate Submit to GPU");

        let mut cmd = self.upload_context.command_buffer;

        let cmd_begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe { self.vk_device.begin_command_buffer(cmd, &cmd_begin_info) }?;

        function(self, &mut cmd)?;

        unsafe { self.vk_device.end_command_buffer(cmd) }?;

        let command_buffers = [cmd];
        let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);

        let submits = [*submit_info];
        unsafe {
            self.vk_device.queue_submit(
                self.upload_context.queue,
                &submits,
                self.upload_context.fence,
            )
        }?;

        unsafe {
            self.vk_device
                .wait_for_fences(&[self.upload_context.fence], true, u64::MAX)
        }?;

        unsafe { self.vk_device.reset_fences(&[self.upload_context.fence]) }?;

        unsafe {
            self.vk_device.reset_command_pool(
                self.upload_context.command_pool,
                vk::CommandPoolResetFlags::RELEASE_RESOURCES,
            )
        }?;
        Ok(())
    }

    pub fn set_vulkan_debug_name(
        &self,
        object_handle: u64,
        object_type: ObjectType,
        debug_name: &str,
    ) -> Result<()> {
        let object_name = CString::new(debug_name).unwrap();
        let pipeline_debug_info = DebugUtilsObjectNameInfoEXT::builder()
            .object_type(object_type)
            .object_handle(object_handle)
            .object_name(object_name.as_ref());

        unsafe {
            self.debug_utils_loader
                .set_debug_utils_object_name(self.vk_device.handle(), &pipeline_debug_info)?;
        }
        Ok(())
    }

    pub fn render_targets(&self) -> &RenderTargets {
        &self.render_targets
    }

    pub fn bindless_descriptor_set_layout(&self) -> &vk::DescriptorSetLayout {
        &self.bindless_descriptor_set_layout
    }

    pub fn bindless_descriptor_set(&self) -> &[vk::DescriptorSet; FRAMES_IN_FLIGHT] {
        &self.bindless_descriptor_set
    }

    pub fn get_descriptor_index(&self, image: &BindlessImage) -> Option<usize> {
        self.bindless_manager.get_bindless_index(image)
    }
}

impl Drop for GraphicsDevice {
    fn drop(&mut self) {
        unsafe {
            self.vk_device.device_wait_idle().unwrap();
            self.vk_device
                .destroy_descriptor_set_layout(self.bindless_descriptor_set_layout, None);
            self.vk_device
                .destroy_descriptor_pool(self.bindless_descriptor_pool, None);
            self.resource_manager.destroy_resources();
            self.vk_device.destroy_sampler(self.default_sampler, None);
            self.vk_device.destroy_sampler(self.shadow_sampler, None);
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

struct ImageToUpload {
    buffer_handle: BufferHandle,
    image_handle: ImageHandle,
    width: u32,
    height: u32,
    mip_levels: u32,
}

pub(crate) fn cmd_copy_buffer(
    graphics_device: &GraphicsDevice,
    cmd: &vk::CommandBuffer,
    src: BufferHandle,
    target: BufferHandle,
) -> Result<()> {
    let src_buffer = graphics_device.resource_manager.get_buffer(src).unwrap();
    let target_buffer = graphics_device.resource_manager.get_buffer(target).unwrap();

    ensure!(src_buffer.size() == target_buffer.size());

    let buffer_copy_info = vk::BufferCopy::builder().size(src_buffer.size());
    unsafe {
        graphics_device.vk_device.cmd_copy_buffer(
            *cmd,
            src_buffer.buffer(),
            target_buffer.buffer(),
            &[*buffer_copy_info],
        )
    }
    Ok(())
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

pub enum ImageFormatType {
    Default,
    Normal,
}
