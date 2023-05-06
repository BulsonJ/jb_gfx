use std::mem::size_of;
use std::sync::Arc;

use anyhow::Result;
use ash::vk;
use ash::vk::{DeviceSize, IndexType};
use cgmath::Zero;
use log::trace;
use slotmap::{new_key_type, SlotMap};

use crate::core::device::cmd_copy_buffer;
use crate::mesh::Index;
use crate::resource::{BufferCreateInfo, BufferStorageType};
use crate::{BufferHandle, GraphicsDevice, MeshData, Vertex};

const LARGE_BUFFER_SIZE: u32 = 16000000; // 128mb

pub struct MeshPool {
    device: Arc<GraphicsDevice>,
    vertex_buffer: BufferHandle,
    index_buffer: BufferHandle,
    meshes: SlotMap<MeshHandle, PooledMesh>,
}

pub struct PooledMesh {
    pub vertex_offset: usize,
    pub vertex_count: usize,
    pub index_offset: usize,
    pub index_count: usize,
}

impl MeshPool {
    pub fn new(device: Arc<GraphicsDevice>) -> Self {
        let vertex_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: LARGE_BUFFER_SIZE as usize,
                usage: vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER,
                storage_type: BufferStorageType::Device,
            };

            device.resource_manager.create_buffer(&buffer_create_info)
        };

        let index_buffer = {
            let buffer_create_info = BufferCreateInfo {
                size: LARGE_BUFFER_SIZE as usize,
                usage: vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::INDEX_BUFFER,
                storage_type: BufferStorageType::Device,
            };

            device.resource_manager.create_buffer(&buffer_create_info)
        };

        MeshPool {
            device,
            vertex_buffer,
            index_buffer,
            meshes: SlotMap::default(),
        }
    }

    pub fn vertex_buffer(&self) -> vk::Buffer {
        self.device
            .resource_manager
            .get_buffer(self.vertex_buffer)
            .unwrap()
            .buffer()
    }

    pub fn index_buffer(&self) -> vk::Buffer {
        self.device
            .resource_manager
            .get_buffer(self.index_buffer)
            .unwrap()
            .buffer()
    }

    pub fn get(&self, handle: MeshHandle) -> Option<&PooledMesh> {
        self.meshes.get(handle)
    }

    pub fn add_mesh(&mut self, mesh: &MeshData) -> Result<MeshHandle> {
        profiling::scope!("Load Mesh");

        let vertex_buffer_offset = {
            let staging_buffer_create_info = BufferCreateInfo {
                size: (size_of::<Vertex>() * mesh.vertices.len()),
                usage: vk::BufferUsageFlags::TRANSFER_SRC,
                storage_type: BufferStorageType::HostLocal,
            };

            let staging_buffer = self
                .device
                .resource_manager
                .create_buffer(&staging_buffer_create_info);

            self.device
                .resource_manager
                .get_buffer(staging_buffer)
                .unwrap()
                .view()
                .mapped_slice()?
                .copy_from_slice(mesh.vertices.as_slice());

            let offset = self.meshes.values().map(|mesh| mesh.vertex_count).sum();
            let buffer_offset = size_of::<Vertex>() * offset;

            assert!(
                size_of::<Vertex>() * (offset + mesh.vertices.len()) <= LARGE_BUFFER_SIZE as usize
            );

            self.device.immediate_submit(|device, cmd| {
                cmd_copy_buffer(
                    device,
                    cmd,
                    staging_buffer,
                    self.vertex_buffer,
                    buffer_offset,
                )?;
                Ok(())
            })?;

            offset
        };
        match &mesh.indices {
            None => {
                let render_mesh = PooledMesh {
                    vertex_offset: vertex_buffer_offset,
                    vertex_count: mesh.vertices.len(),
                    index_offset: 0,
                    index_count: 0,
                };
                trace!(
                    "Mesh Loaded. Vertex Count:{}|Faces:{}",
                    mesh.vertices.len(),
                    mesh.faces.len()
                );
                Ok(self.meshes.insert(render_mesh))
            }
            Some(indices) => {
                let index_buffer_offset = {
                    let buffer_size = size_of::<Index>() * indices.len();
                    let staging_buffer_create_info = BufferCreateInfo {
                        size: buffer_size,
                        usage: vk::BufferUsageFlags::TRANSFER_SRC,
                        storage_type: BufferStorageType::HostLocal,
                    };

                    let staging_buffer = self
                        .device
                        .resource_manager
                        .create_buffer(&staging_buffer_create_info);

                    self.device
                        .resource_manager
                        .get_buffer(staging_buffer)
                        .unwrap()
                        .view()
                        .mapped_slice()?
                        .copy_from_slice(indices.as_slice());

                    let offset = self.meshes.values().map(|mesh| mesh.index_count).sum();
                    let buffer_offset = size_of::<Index>() * offset;

                    assert!(
                        size_of::<Index>() * (offset + indices.len()) <= LARGE_BUFFER_SIZE as usize
                    );

                    self.device.immediate_submit(|device, cmd| {
                        cmd_copy_buffer(
                            device,
                            cmd,
                            staging_buffer,
                            self.index_buffer,
                            buffer_offset,
                        )?;
                        Ok(())
                    })?;

                    offset
                };
                let render_mesh = PooledMesh {
                    vertex_offset: vertex_buffer_offset,
                    vertex_count: mesh.vertices.len(),
                    index_offset: index_buffer_offset,
                    index_count: indices.len(),
                };
                trace!(
                    "Mesh Loaded. Vertex Count:{}|Index Count:{}|Faces:{}",
                    mesh.vertices.len(),
                    mesh.indices.as_ref().unwrap().len(),
                    mesh.faces.len()
                );
                Ok(self.meshes.insert(render_mesh))
            }
        }
    }

    pub fn bind(&self, cmd: vk::CommandBuffer) {
        let vertex_buffer = self.vertex_buffer();
        let index_buffer = self.index_buffer();
        unsafe {
            self.device
                .vk_device
                .cmd_bind_vertex_buffers(cmd, 0u32, &[vertex_buffer], &[0u64]);
            self.device.vk_device.cmd_bind_index_buffer(
                cmd,
                index_buffer,
                DeviceSize::zero(),
                IndexType::UINT32,
            );
        }
    }
}

new_key_type! {pub struct MeshHandle;}
