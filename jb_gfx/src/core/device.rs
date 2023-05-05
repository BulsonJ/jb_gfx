use std::cell::RefCell;
use std::ffi::CString;
use std::sync::Arc;
use std::{borrow::Cow, ffi::CStr};

use anyhow::{ensure, Result};
use ash::extensions::khr::Synchronization2;
use ash::extensions::{ext::DebugUtils, khr::DynamicRendering};
use ash::vk::{
    self, DebugUtilsObjectNameInfoEXT, DeviceSize, Handle, ImageLayout, ObjectType,
    SurfaceTransformFlagsKHR,
};
use log::{error, info};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::renderpass::barrier::{ImageBarrier, ImageBarrierBuilder, ImageHandleType};
use crate::resource::{
    BufferCreateInfo, BufferHandle, BufferStorageType, ImageHandle, ResourceManager,
};
use crate::util::bindless::BindlessManager;

pub const FRAMES_IN_FLIGHT: usize = 2usize;
pub const SHADOWMAP_SIZE: u32 = 4096u32;
pub const QUERY_COUNT: u32 = 10u32;

pub struct GraphicsDevice {
    instance: ash::Instance,
    size: RefCell<PhysicalSize<u32>>,
    swapchain: RefCell<Swapchain>,
    surface: RefCell<Surface>,
    present_index: RefCell<usize>,
    frame_number: RefCell<usize>,
    pub vk_device: Arc<ash::Device>,
    pdevice: vk::PhysicalDevice,
    query_pool: vk::QueryPool,
    timestamp_period: f32,
    timestamp_frame_count: RefCell<usize>,
    pub resource_manager: Arc<ResourceManager>,
    debug_utils_loader: DebugUtils,
    debug_call_back: vk::DebugUtilsMessengerEXT,
    graphics_queue: vk::Queue,
    graphics_command_pool: [vk::CommandPool; FRAMES_IN_FLIGHT],
    graphics_command_buffer: [vk::CommandBuffer; FRAMES_IN_FLIGHT],
    draw_commands_reuse_fence: [vk::Fence; FRAMES_IN_FLIGHT],
    rendering_complete_semaphore: [vk::Semaphore; FRAMES_IN_FLIGHT],
    present_complete_semaphore: [vk::Semaphore; FRAMES_IN_FLIGHT],
    upload_context: UploadContext,
    images_to_upload: RefCell<Vec<ImageToUpload>>,
    buffers_to_delete: RefCell<Vec<(BufferHandle, usize)>>,
    bindless_descriptor_set_layout: vk::DescriptorSetLayout,
    bindless_descriptor_set: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    bindless_manager: RefCell<BindlessManager>,
    bindless_descriptor_pool: vk::DescriptorPool,
    default_sampler: vk::Sampler,
    shadow_sampler: vk::Sampler,
    ui_sampler: vk::Sampler,
    timestamps: RefCell<Vec<u64>>,
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

        let mut sync_2_feature =
            vk::PhysicalDeviceSynchronization2Features::builder().synchronization2(true);

        let mut dynamic_rendering_feature =
            vk::PhysicalDeviceDynamicRenderingFeatures::builder().dynamic_rendering(true);

        let surface_loader = ash::extensions::khr::Surface::new(&entry, &instance);
        let pdevices =
            unsafe { instance.enumerate_physical_devices() }.expect("Physical device error");
        let mut timestamp_period = 0.0;
        let mut max_sampler_anisotropy = 0.0;
        let (pdevice, queue_family_index) = pdevices
            .iter()
            .find_map(|pdevice| {
                let limits = unsafe { instance.get_physical_device_properties(*pdevice).limits };
                if limits.timestamp_period == 0.0 {
                    None
                } else {
                    timestamp_period = limits.timestamp_period;
                    max_sampler_anisotropy = limits.max_sampler_anisotropy;
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
                }
            })
            .expect("Couldn't find suitable device.");
        let queue_family_index = queue_family_index as u32;
        let device_extension_names_raw = [
            ash::extensions::khr::Swapchain::name().as_ptr(),
            DynamicRendering::name().as_ptr(),
            Synchronization2::name().as_ptr(),
        ];
        let features = vk::PhysicalDeviceFeatures {
            shader_clip_distance: 1,
            sampler_anisotropy: vk::TRUE,
            ..Default::default()
        };
        let mut descriptor_indexing_features =
            vk::PhysicalDeviceDescriptorIndexingFeatures::builder()
                .shader_sampled_image_array_non_uniform_indexing(true)
                .descriptor_binding_partially_bound(true)
                .descriptor_binding_variable_descriptor_count(true)
                .runtime_descriptor_array(true);
        let mut query_features =
            vk::PhysicalDeviceHostQueryResetFeatures::builder().host_query_reset(true);

        let priorities = [1.0];

        let queue_info = vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities);

        let device_create_info = vk::DeviceCreateInfo::builder()
            .push_next(&mut descriptor_indexing_features)
            .push_next(&mut sync_2_feature)
            .push_next(&mut dynamic_rendering_feature)
            .push_next(&mut query_features)
            .queue_create_infos(std::slice::from_ref(&queue_info))
            .enabled_extension_names(&device_extension_names_raw)
            .enabled_features(&features);

        let ash_device = unsafe { instance.create_device(pdevice, &device_create_info, None) }?;
        let device = Arc::new(ash_device);

        let query_pool = {
            let create_info = vk::QueryPoolCreateInfo::builder()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count(QUERY_COUNT);

            unsafe { device.create_query_pool(&create_info, None) }
        }?;
        unsafe {
            device.reset_query_pool(query_pool, 0, QUERY_COUNT);
        }

        let resource_manager = ResourceManager::new(&instance, &pdevice, device.clone());

        let graphics_queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let (surface, swapchain) = {
            let surface_format =
                unsafe { surface_loader.get_physical_device_surface_formats(pdevice, surface) }?
                    .into_iter()
                    .find(|&x| {
                        (x.format == vk::Format::B8G8R8A8_SRGB
                            || x.format == vk::Format::R8G8B8A8_SRGB)
                            && x.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                    })
                    .unwrap();

            let surface_capabilities = unsafe {
                surface_loader.get_physical_device_surface_capabilities(pdevice, surface)
            }?;
            ensure!(surface_capabilities
                .supported_usage_flags
                .contains(vk::ImageUsageFlags::STORAGE));
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
            let swapchain_loader = ash::extensions::khr::Swapchain::new(&instance, &device);

            let surface = Surface {
                surface,
                surface_loader,
                surface_format,
                surface_resolution,
            };
            let swapchain = Swapchain::new(
                &device,
                swapchain_loader,
                pdevice,
                &surface,
                pre_transform,
                desired_image_count,
            )?;
            (surface, swapchain)
        };

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
                .max_lod(vk::LOD_CLAMP_NONE)
                .anisotropy_enable(true)
                .max_anisotropy(max_sampler_anisotropy);

            unsafe { device.create_sampler(&sampler_info, None)? }
        };

        let shadow_sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .min_lod(0.0f32)
                .max_lod(1.0f32)
                .mip_lod_bias(0.0)
                .max_anisotropy(1.0)
                .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE);

            unsafe { device.create_sampler(&sampler_info, None)? }
        };

        let ui_sampler = {
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
                .descriptor_count(3u32)
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

        let resource_manager = Arc::new(resource_manager);
        let samplers = vec![default_sampler, shadow_sampler, ui_sampler];
        let bindless_manager = RefCell::new(BindlessManager::new(
            device.clone(),
            resource_manager.clone(),
            bindless_descriptor_set,
        ));
        bindless_manager
            .borrow_mut()
            .setup_samplers(&samplers, &device)?;

        let device = Self {
            instance,
            size: RefCell::new(size),
            swapchain: RefCell::new(swapchain),
            surface: RefCell::new(surface),
            present_index: RefCell::new(0),
            vk_device: device,
            pdevice,
            query_pool,
            timestamp_period,
            timestamp_frame_count: RefCell::new(0),
            resource_manager,
            debug_utils_loader,
            debug_call_back,
            graphics_queue,
            graphics_command_pool,
            graphics_command_buffer,
            draw_commands_reuse_fence,
            rendering_complete_semaphore,
            present_complete_semaphore,
            upload_context,
            default_sampler,
            frame_number: RefCell::new(0),
            images_to_upload: RefCell::new(Vec::default()),
            buffers_to_delete: RefCell::new(Vec::default()),
            bindless_descriptor_set_layout,
            bindless_descriptor_set,
            bindless_manager,
            bindless_descriptor_pool: descriptor_pool,
            shadow_sampler,
            ui_sampler,
            timestamps: RefCell::default(),
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

    fn present_index(&self) -> usize {
        *self.present_index.borrow()
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        *self.size.borrow()
    }

    pub fn get_present_image(&self) -> vk::Image {
        self.swapchain.borrow().present_images[self.present_index()]
    }

    pub fn get_present_image_view(&self) -> vk::ImageView {
        self.swapchain.borrow().present_image_views[self.present_index()]
    }

    pub fn surface_format(&self) -> vk::SurfaceFormatKHR {
        self.surface.borrow().surface_format
    }

    pub fn frame_number(&self) -> usize {
        *self.frame_number.borrow()
    }

    pub fn buffered_resource_number(&self) -> usize {
        self.frame_number() % 2
    }

    pub fn start_frame(&self) -> Result<()> {
        profiling::scope!("Start Frame");

        unsafe {
            self.vk_device.wait_for_fences(
                &[self.draw_commands_reuse_fence[self.buffered_resource_number()]],
                true,
                u64::MAX,
            )
        }?;

        let (present_index, _) = unsafe {
            self.swapchain.borrow().swapchain_loader.acquire_next_image(
                self.swapchain.borrow().swapchain,
                u64::MAX,
                self.present_complete_semaphore[self.buffered_resource_number()],
                vk::Fence::null(),
            )
        }?;
        *self.present_index.borrow_mut() = present_index as usize;

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

        // Reset query pool
        unsafe {
            self.vk_device
                .reset_query_pool(self.query_pool, 0, QUERY_COUNT);
        }
        *self.timestamp_frame_count.borrow_mut() = 0;

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
        for buffer_to_delete in self.buffers_to_delete.borrow_mut().iter_mut() {
            buffer_to_delete.1 -= 1;

            if buffer_to_delete.1 == 0 {
                self.resource_manager.destroy_buffer(buffer_to_delete.0);
            }
        }
        self.buffers_to_delete.borrow_mut().clear();

        // Upload images
        // TODO: Remove buffers once upload has completed. Could use status enum so when fences are called, updates images that were submitted to being done.
        // Can then clear done images from vec.
        for image in self.images_to_upload.borrow().iter() {
            profiling::scope!("Deferred Upload Image to GPU");
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
                        layer_count: image.img_layers,
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

                self.buffers_to_delete
                    .borrow_mut()
                    .push((image.buffer_handle, 2));
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
                            image_layers: image.img_layers,
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
                            layer_count: image.img_layers,
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
                            layer_count: image.img_layers,
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
                            image_layers: image.img_layers,
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
                        image_layers: image.img_layers,
                    })
                    .build(
                        self,
                        &self.graphics_command_buffer[self.buffered_resource_number()],
                    )?;
            }
            self.buffers_to_delete
                .borrow_mut()
                .push((image.buffer_handle, 2));
        }
        self.images_to_upload.borrow_mut().clear();

        Ok(())
    }

    pub fn end_frame(&self) -> Result<()> {
        profiling::scope!("End Frame");

        unsafe {
            self.vk_device
                .end_command_buffer(self.graphics_command_buffer())
        }?;

        let wait_semaphores = [self.present_complete_semaphore()];
        let wait_dst_stage_mask = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = [self.graphics_command_buffer()];
        let signal_semaphores = [self.rendering_complete_semaphore()];
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_dst_stage_mask)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);

        let submits = [*submit_info];
        let result = unsafe {
            self.vk_device.queue_submit(
                self.graphics_queue(),
                &submits,
                self.draw_commands_reuse_fence(),
            )
        };
        if let Some(error) = result.err() {
            error!("{}", error);
        }

        let timestamp_result = {
            let mut query_pool_results = [0u64; QUERY_COUNT as usize];
            let result = unsafe {
                self.vk_device.get_query_pool_results(
                    self.query_pool,
                    0,
                    *self.timestamp_frame_count.borrow() as u32,
                    &mut query_pool_results,
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                )
            };
            if result.is_ok() {
                Some(Vec::from(query_pool_results))
            } else {
                //Some(Vec::from(query_pool_results))
                error!("{}", result.err().unwrap());
                None
            }
        };
        match timestamp_result {
            None => {}
            Some(timestamps) => *self.timestamps.borrow_mut() = timestamps,
        }

        let wait_semaphores = [self.rendering_complete_semaphore[self.buffered_resource_number()]];
        let swapchains = [self.swapchain.borrow().swapchain];
        let image_indices = [self.present_index() as u32];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        unsafe {
            self.swapchain
                .borrow()
                .swapchain_loader
                .queue_present(self.graphics_queue, &present_info)
        }?;

        *self.frame_number.borrow_mut() += 1usize;
        Ok(())
    }

    pub fn resize(&self, new_size: winit::dpi::PhysicalSize<u32>) -> Result<bool> {
        if new_size.width == 0u32 || new_size.height == 0u32 || new_size == self.size() {
            return Ok(false);
        }

        profiling::scope!("Resize");

        unsafe { self.vk_device.device_wait_idle() }?;
        *self.size.borrow_mut() = new_size;

        // Destroy old swapchain

        unsafe {
            self.swapchain
                .borrow()
                .swapchain_loader
                .destroy_swapchain(self.swapchain.borrow().swapchain, None);

            for &image_view in self.swapchain.borrow().present_image_views.iter() {
                self.vk_device.destroy_image_view(image_view, None);
            }
        }

        // Create swapchain
        let surface_capabilities = unsafe {
            self.surface
                .borrow()
                .surface_loader
                .get_physical_device_surface_capabilities(
                    self.pdevice,
                    self.surface.borrow().surface,
                )
        }?;
        let mut desired_image_count = surface_capabilities.min_image_count + 1;
        if surface_capabilities.max_image_count > 0
            && desired_image_count > surface_capabilities.max_image_count
        {
            desired_image_count = surface_capabilities.max_image_count;
        }
        self.surface.borrow_mut().surface_resolution =
            match surface_capabilities.current_extent.width {
                u32::MAX => vk::Extent2D {
                    width: self.size().width,
                    height: self.size().height,
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
        let loader = self.swapchain.borrow().swapchain_loader.clone();
        self.swapchain.replace(Swapchain::new(
            &self.vk_device,
            loader,
            self.pdevice,
            &self.surface.borrow(),
            pre_transform,
            desired_image_count,
        )?);

        info!("Recreating swapchain.");
        Ok(true)
    }

    pub(crate) fn load_image(
        &self,
        img_bytes: &[u8],
        img_width: u32,
        img_height: u32,
        image_type: &ImageFormatType,
        mip_levels: u32,
        img_layers: u32,
    ) -> Result<ImageHandle> {
        profiling::scope!("Load Image");

        let img_size = (img_width * img_height * 4u32 * img_layers) as DeviceSize;

        let staging_buffer_create_info = BufferCreateInfo {
            size: img_size as usize,
            usage: vk::BufferUsageFlags::TRANSFER_SRC,
            storage_type: BufferStorageType::HostLocal,
        };

        let staging_buffer = self
            .resource_manager
            .create_buffer(&staging_buffer_create_info);

        self.resource_manager
            .get_buffer(staging_buffer)
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
            .array_layers(img_layers)
            .mip_levels(mip_levels)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL);

        let image = self.resource_manager.create_image(&image_create_info);

        self.images_to_upload.borrow_mut().push(ImageToUpload {
            buffer_handle: staging_buffer,
            image_handle: image,
            width: img_width,
            height: img_height,
            mip_levels,
            img_layers,
        });

        self.bindless_manager
            .borrow_mut()
            .add_image_to_bindless(&image);

        Ok(image)
    }

    pub fn immediate_submit<F: Fn(&GraphicsDevice, &vk::CommandBuffer) -> Result<()>>(
        &self,
        function: F,
    ) -> Result<()> {
        profiling::scope!("Immediate Submit to GPU");

        let cmd = self.upload_context.command_buffer;

        let cmd_begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe { self.vk_device.begin_command_buffer(cmd, &cmd_begin_info) }?;

        function(self, &cmd)?;

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

    pub fn graphics_queue(&self) -> vk::Queue {
        self.graphics_queue
    }

    pub fn graphics_command_buffer(&self) -> vk::CommandBuffer {
        self.graphics_command_buffer[self.buffered_resource_number()]
    }

    pub fn draw_commands_reuse_fence(&self) -> vk::Fence {
        self.draw_commands_reuse_fence[self.buffered_resource_number()]
    }

    pub fn rendering_complete_semaphore(&self) -> vk::Semaphore {
        self.rendering_complete_semaphore[self.buffered_resource_number()]
    }

    pub fn present_complete_semaphore(&self) -> vk::Semaphore {
        self.present_complete_semaphore[self.buffered_resource_number()]
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

    pub fn write_timestamp(
        &self,
        cmd: vk::CommandBuffer,
        stage: vk::PipelineStageFlags2,
    ) -> TimeStampIndex {
        let mut timestamp_count = self.timestamp_frame_count.borrow_mut();
        let count = *timestamp_count as u32;
        unsafe {
            self.vk_device
                .cmd_write_timestamp2(cmd, stage, self.query_pool, count);
        }
        let timestamp_index = TimeStampIndex(*timestamp_count);
        *timestamp_count += 1;
        timestamp_index
    }

    pub fn timestamp_period(&self) -> f32 {
        self.timestamp_period
    }

    pub fn get_timestamp_result(
        &self,
        start_index: TimeStampIndex,
        end_index: TimeStampIndex,
    ) -> Option<f64> {
        let timestamps = self.timestamps.borrow();

        let start = timestamps.get(start_index.0);
        let end = timestamps.get(end_index.0);
        match (start, end) {
            (Some(&start), Some(&end)) => {
                let get_time = |start: u64, end: u64| {
                    ((end - start) as f64 * self.timestamp_period() as f64) / 1000000.0f64
                };

                let result = get_time(start, end);
                Some(result)
            }
            _ => None,
        }
    }

    pub fn bindless_descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.bindless_descriptor_set_layout
    }

    pub fn bindless_descriptor_set(&self) -> vk::DescriptorSet {
        self.bindless_descriptor_set[self.buffered_resource_number()]
    }

    pub fn get_descriptor_index(&self, image: &ImageHandle) -> Option<usize> {
        self.bindless_manager.borrow().get_bindless_index(image)
    }
}

impl GraphicsDevice {
    pub fn default_sampler(&self) -> vk::Sampler {
        self.default_sampler
    }
    pub fn shadow_sampler(&self) -> vk::Sampler {
        self.shadow_sampler
    }
    pub fn ui_sampler(&self) -> vk::Sampler {
        self.ui_sampler
    }
}

impl Drop for GraphicsDevice {
    fn drop(&mut self) {
        unsafe {
            self.vk_device.device_wait_idle().unwrap();
            self.vk_device.destroy_query_pool(self.query_pool, None);
            self.vk_device
                .destroy_descriptor_set_layout(self.bindless_descriptor_set_layout, None);
            self.vk_device
                .destroy_descriptor_pool(self.bindless_descriptor_pool, None);
            self.resource_manager.destroy_resources();
            self.vk_device.destroy_sampler(self.default_sampler, None);
            self.vk_device.destroy_sampler(self.shadow_sampler, None);
            self.vk_device.destroy_sampler(self.ui_sampler, None);
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
            for &image_view in self.swapchain.borrow().present_image_views.iter() {
                self.vk_device.destroy_image_view(image_view, None);
            }
            self.vk_device
                .destroy_command_pool(self.upload_context.command_pool, None);
            for pool in self.graphics_command_pool.into_iter() {
                self.vk_device.destroy_command_pool(pool, None);
            }
            self.swapchain
                .borrow()
                .swapchain_loader
                .destroy_swapchain(self.swapchain.borrow().swapchain, None);
            self.vk_device.destroy_device(None);
            self.surface
                .borrow()
                .surface_loader
                .destroy_surface(self.surface.borrow().surface, None);
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
    img_layers: u32,
}

pub(crate) fn cmd_copy_buffer(
    graphics_device: &GraphicsDevice,
    cmd: &vk::CommandBuffer,
    src: BufferHandle,
    target: BufferHandle,
    dst_offset: usize,
) -> Result<()> {
    let src_buffer = graphics_device.resource_manager.get_buffer(src).unwrap();
    let target_buffer = graphics_device.resource_manager.get_buffer(target).unwrap();

    let buffer_copy_info = vk::BufferCopy::builder()
        .size(src_buffer.size())
        .dst_offset(dst_offset as u64);
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

struct Swapchain {
    swapchain: vk::SwapchainKHR,
    swapchain_loader: ash::extensions::khr::Swapchain,
    present_images: Vec<vk::Image>,
    present_image_views: Vec<vk::ImageView>,
}

impl Swapchain {
    fn new(
        device: &ash::Device,
        swapchain_loader: ash::extensions::khr::Swapchain,
        pdevice: vk::PhysicalDevice,
        surface: &Surface,
        pre_transform: SurfaceTransformFlagsKHR,
        desired_image_count: u32,
    ) -> Result<Self> {
        let present_modes = unsafe {
            surface
                .surface_loader
                .get_physical_device_surface_present_modes(pdevice, surface.surface)
        }?;
        let present_mode = present_modes
            .iter()
            .cloned()
            .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface.surface)
            .min_image_count(desired_image_count)
            .image_color_space(surface.surface_format.color_space)
            .image_format(surface.surface_format.format)
            .image_extent(surface.surface_resolution)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_array_layers(1);

        let swapchain = unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None) }?;

        let present_images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }?;
        let present_image_views = present_images
            .iter()
            .map(|&image| {
                let create_view_info = vk::ImageViewCreateInfo::builder()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(surface.surface_format.format)
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

        Ok(Swapchain {
            swapchain,
            swapchain_loader,
            present_images,
            present_image_views,
        })
    }
}

struct Surface {
    surface: vk::SurfaceKHR,
    surface_loader: ash::extensions::khr::Surface,
    surface_format: vk::SurfaceFormatKHR,
    surface_resolution: vk::Extent2D,
}

#[derive(Copy, Clone)]
pub struct TimeStampIndex(usize);
