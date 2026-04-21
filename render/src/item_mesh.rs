use crate::gpu::GpuVertex;
use crate::item_model::ItemModel;
use wgpu::{Buffer, Device};

pub struct GpuItemMesh {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
}

impl GpuItemMesh {
    pub fn from_item_model(device: &Device, model: &ItemModel) -> Option<Self> {
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("item-vertex-buffer"),
            size: (model.vertices.len() * std::mem::size_of::<GpuVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("item-index-buffer"),
            size: (model.indices.len() * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Some(Self {
            vertex_buffer,
            index_buffer,
            index_count: model.indices.len() as u32,
        })
    }

    pub fn upload(&self, queue: &wgpu::Queue, model: &ItemModel) {
        queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&model.vertices),
        );
        queue.write_buffer(
            &self.index_buffer,
            0,
            bytemuck::cast_slice(&model.indices),
        );
    }
}