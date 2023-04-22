use crate::device::GraphicsDevice;
use ash::prelude::VkResult;
use ash::vk;
use ash::vk::DescriptorPoolCreateFlags;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::BitOr;
use std::ptr::hash;
use std::sync::Arc;
use vk_mem_alloc::create_pool;

pub struct DescriptorAllocator {
    device: Arc<ash::Device>,
    descriptor_sizes: PoolSizes,
    used_pools: Vec<vk::DescriptorPool>,
    free_pools: Vec<vk::DescriptorPool>,
    current_pool: Option<vk::DescriptorPool>,
}

impl DescriptorAllocator {
    pub fn new(device: Arc<ash::Device>) -> Self {
        Self {
            device,
            descriptor_sizes: Default::default(),
            used_pools: vec![],
            free_pools: vec![],
            current_pool: None,
        }
    }

    pub fn cleanup(&self) {
        for pool in self.free_pools.iter() {
            unsafe {
                self.device.destroy_descriptor_pool(*pool, None);
            }
        }
        for pool in self.used_pools.iter() {
            unsafe {
                self.device.destroy_descriptor_pool(*pool, None);
            }
        }
    }

    fn create_pool(
        device: &ash::Device,
        pool_sizes: &PoolSizes,
        count: i32,
        flags: vk::DescriptorPoolCreateFlags,
    ) -> anyhow::Result<vk::DescriptorPool> {
        let sizes: Vec<vk::DescriptorPoolSize> = pool_sizes
            .sizes
            .iter()
            .map(|&pair| {
                vk::DescriptorPoolSize::builder()
                    .ty(pair.0)
                    .descriptor_count((pair.1 * count as f32) as u32)
                    .build()
            })
            .collect();

        let pool_create_info = vk::DescriptorPoolCreateInfo::builder()
            .flags(flags)
            .max_sets(count as u32)
            .pool_sizes(&sizes);

        let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_create_info, None) }?;

        Ok(descriptor_pool)
    }

    pub fn grab_pool(&mut self) -> anyhow::Result<vk::DescriptorPool> {
        if !self.free_pools.is_empty() {
            let pool = self.free_pools.remove(self.free_pools.len());
            Ok(pool)
        } else {
            let pool = DescriptorAllocator::create_pool(
                &self.device,
                &self.descriptor_sizes,
                1000,
                vk::DescriptorPoolCreateFlags::empty(),
            )?;
            Ok(pool)
        }
    }

    pub fn allocate(
        &mut self,
        layout: vk::DescriptorSetLayout,
    ) -> anyhow::Result<vk::DescriptorSet> {
        if self.current_pool.is_none() {
            self.current_pool = Some(self.grab_pool()?);
            self.used_pools.push(self.current_pool.unwrap())
        }

        let set_layouts = [layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::builder()
            .set_layouts(&set_layouts)
            .descriptor_pool(self.current_pool.unwrap());

        let result = unsafe { self.device.allocate_descriptor_sets(&alloc_info) };
        match result {
            Ok(sets) => {
                let first = *sets.get(0).unwrap();
                return Ok(first);
            }
            Err(error) => {
                if error == vk::Result::ERROR_OUT_OF_POOL_MEMORY {
                    self.current_pool = Some(self.grab_pool()?);
                    self.used_pools.push(self.current_pool.unwrap());
                    let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                        .set_layouts(&set_layouts)
                        .descriptor_pool(self.current_pool.unwrap());

                    let result = unsafe { self.device.allocate_descriptor_sets(&alloc_info) };
                    if result.is_err() {
                        anyhow::bail!("Not working")
                    }
                    let first = *result.unwrap().get(0).unwrap();
                    return Ok(first);
                }
                anyhow::bail!("Not working")
            }
        }
    }

    pub fn reset_pools(&mut self) -> anyhow::Result<()> {
        for pool in self.used_pools.iter() {
            unsafe {
                self.device
                    .reset_descriptor_pool(*pool, vk::DescriptorPoolResetFlags::empty())
            }?;
            self.free_pools.push(*pool);
        }

        self.used_pools.clear();
        self.current_pool = None;
        Ok(())
    }
}

struct PoolSizes {
    sizes: Vec<(vk::DescriptorType, f32)>,
}

impl Default for PoolSizes {
    fn default() -> Self {
        Self {
            sizes: vec![
                (vk::DescriptorType::SAMPLER, 0.5),
                (vk::DescriptorType::COMBINED_IMAGE_SAMPLER, 4.0),
                (vk::DescriptorType::SAMPLED_IMAGE, 4.0),
                (vk::DescriptorType::STORAGE_IMAGE, 1.0),
                (vk::DescriptorType::UNIFORM_TEXEL_BUFFER, 1.0),
                (vk::DescriptorType::STORAGE_TEXEL_BUFFER, 1.0),
                (vk::DescriptorType::UNIFORM_BUFFER, 2.0),
                (vk::DescriptorType::STORAGE_BUFFER, 2.0),
                (vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC, 1.0),
                (vk::DescriptorType::STORAGE_BUFFER_DYNAMIC, 1.0),
                (vk::DescriptorType::INPUT_ATTACHMENT, 0.5),
            ],
        }
    }
}

pub struct DescriptorLayoutCache {
    device: Arc<ash::Device>,
    layout_cache: HashMap<DescriptorLayoutInfo, vk::DescriptorSetLayout>,
}

impl DescriptorLayoutCache {
    pub fn new(device: Arc<ash::Device>) -> Self {
        Self {
            device,
            layout_cache: HashMap::default(),
        }
    }

    pub fn cleanup(&self) {
        for (_, set_layout) in self.layout_cache.iter() {
            unsafe { self.device.destroy_descriptor_set_layout(*set_layout, None) }
        }
    }

    pub fn create_descriptor_layout(
        &mut self,
        create_info: vk::DescriptorSetLayoutCreateInfo,
    ) -> vk::DescriptorSetLayout {
        let mut layout_info = DescriptorLayoutInfo { bindings: vec![] };
        layout_info
            .bindings
            .reserve(create_info.binding_count as usize);

        for i in 0..create_info.binding_count {
            layout_info
                .bindings
                .push(unsafe { create_info.p_bindings.offset(i as isize).read() });

            //TODO check bindings in order
        }

        return if let Some(layout) = self.layout_cache.get(&layout_info) {
            *layout
        } else {
            let layout =
                unsafe { self.device.create_descriptor_set_layout(&create_info, None) }.unwrap();
            self.layout_cache.insert(layout_info, layout);
            layout
        };
    }
}

struct DescriptorLayoutInfo {
    bindings: Vec<vk::DescriptorSetLayoutBinding>,
}

impl PartialEq<Self> for DescriptorLayoutInfo {
    fn eq(&self, other: &Self) -> bool {
        if self.bindings.len() != other.bindings.len() {
            return false;
        }

        for (i, binding) in self.bindings.iter().enumerate() {
            let other_bindings = other.bindings.get(i).unwrap();

            if other_bindings.binding != binding.binding {
                return false;
            }
            if other_bindings.descriptor_type != binding.descriptor_type {
                return false;
            }
            if other_bindings.descriptor_count != binding.descriptor_count {
                return false;
            }
            if other_bindings.stage_flags != binding.stage_flags {
                return false;
            }
        }
        true
    }
}

impl Eq for DescriptorLayoutInfo {}

impl Hash for DescriptorLayoutInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bindings.len().hash(state);

        for binding in self.bindings.iter() {
            let binding: i64 = (binding.binding as i64).bitor(
                ((binding.descriptor_type.as_raw() as i64) << 8i64).bitor(
                    ((binding.descriptor_count << 16i64) as i64)
                        .bitor((binding.stage_flags.as_raw() as i64) << 24),
                ),
            );
            binding.hash(state);
        }
    }
}

pub struct DescriptorBuilder<'a> {
    writes: Vec<vk::WriteDescriptorSet>,
    bindings: Vec<vk::DescriptorSetLayoutBinding>,

    cache: &'a mut DescriptorLayoutCache,
    alloc: &'a mut DescriptorAllocator,
}

impl<'a> DescriptorBuilder<'a> {
    pub fn new(cache: &'a mut DescriptorLayoutCache, alloc: &'a mut DescriptorAllocator) -> Self {
        Self {
            cache,
            alloc,
            writes: Vec::default(),
            bindings: Vec::default(),
        }
    }

    pub fn bind_buffer(
        mut self,
        binding: u32,
        buffer_info: &[vk::DescriptorBufferInfo],
        desc_type: vk::DescriptorType,
        stage_flags: vk::ShaderStageFlags,
    ) -> Self {
        let new_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(binding)
            .descriptor_type(desc_type)
            .descriptor_count(1u32)
            .stage_flags(stage_flags);

        self.bindings.push(*new_binding);

        let new_write = *vk::WriteDescriptorSet::builder()
            .descriptor_type(desc_type)
            .dst_binding(binding)
            .buffer_info(&buffer_info);

        self.writes.push(new_write);

        self
    }

    pub fn build(mut self) -> anyhow::Result<(vk::DescriptorSet, vk::DescriptorSetLayout)> {
        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&self.bindings);

        let layout = self.cache.create_descriptor_layout(*layout_info);

        let set = self.alloc.allocate(layout)?;
        for write in self.writes.iter_mut() {
            write.dst_set = set;
        }

        unsafe { self.alloc.device.update_descriptor_sets(&self.writes, &[]) };

        Ok((set, layout))
    }
}
