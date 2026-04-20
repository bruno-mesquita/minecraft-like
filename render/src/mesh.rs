use crate::gpu::GpuVertex;
use crate::types::{ChunkMesh, Face, FaceVertex};
use voxel_core::ChunkCoord;
use voxel_world::{BlockId, Chunk, AIR};
use wgpu::util::DeviceExt;

pub fn push_face_if_visible(
    chunk: &Chunk,
    vertices: &mut Vec<FaceVertex>,
    world_x: i32,
    world_y: i32,
    world_z: i32,
    neighbor_x: i32,
    neighbor_y: i32,
    neighbor_z: i32,
    block_id: BlockId,
    face: Face,
) {
    if chunk.storage.get(neighbor_x, neighbor_y, neighbor_z) != AIR {
        return;
    }

    vertices.push(FaceVertex {
        position: [world_x, world_y, world_z],
        block_id,
        face,
    });
}

#[derive(Debug)]
pub struct GpuChunkMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl GpuChunkMesh {
    pub fn from_chunk_mesh(device: &wgpu::Device, mesh: &ChunkMesh) -> Option<Self> {
        if mesh.vertices.is_empty() {
            return None;
        }

        let mut vertices = Vec::with_capacity(mesh.vertices.len() * 4);
        let mut indices = Vec::with_capacity(mesh.vertices.len() * 6);

        for (face_index, face) in mesh.vertices.iter().enumerate() {
            let base_index = (face_index * 4) as u32;
            let face_vertices = face_quad_vertices(*face);
            vertices.extend_from_slice(&face_vertices);
            indices.extend_from_slice(&[
                base_index,
                base_index + 1,
                base_index + 2,
                base_index,
                base_index + 2,
                base_index + 3,
            ]);
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("chunk-vertex-buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("chunk-index-buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Some(Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        })
    }
}

pub fn face_quad_vertices(face: FaceVertex) -> [GpuVertex; 4] {
    let [x, y, z] = face.position;
    let x = x as f32;
    let y = y as f32;
    let z = z as f32;
    let color = face_color(face.block_id, face.face);

    match face.face {
        Face::Left => [
            GpuVertex { position: [x, y, z], color },
            GpuVertex { position: [x, y, z + 1.0], color },
            GpuVertex { position: [x, y + 1.0, z + 1.0], color },
            GpuVertex { position: [x, y + 1.0, z], color },
        ],
        Face::Right => [
            GpuVertex { position: [x + 1.0, y, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y, z], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z + 1.0], color },
        ],
        Face::Bottom => [
            GpuVertex { position: [x, y, z + 1.0], color },
            GpuVertex { position: [x, y, z], color },
            GpuVertex { position: [x + 1.0, y, z], color },
            GpuVertex { position: [x + 1.0, y, z + 1.0], color },
        ],
        Face::Top => [
            GpuVertex { position: [x, y + 1.0, z], color },
            GpuVertex { position: [x, y + 1.0, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z], color },
        ],
        Face::Back => [
            GpuVertex { position: [x + 1.0, y, z], color },
            GpuVertex { position: [x, y, z], color },
            GpuVertex { position: [x, y + 1.0, z], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z], color },
        ],
        Face::Front => [
            GpuVertex { position: [x, y, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z + 1.0], color },
            GpuVertex { position: [x, y + 1.0, z + 1.0], color },
        ],
    }
}

fn face_color(block_id: BlockId, face: Face) -> [f32; 3] {
    use voxel_world::{DIRT, GRASS, STONE};

    let base = match block_id {
        GRASS => [0.29, 0.66, 0.18],
        DIRT => [0.51, 0.34, 0.18],
        STONE => [0.52, 0.52, 0.56],
        _ => [0.85, 0.25, 0.85],
    };

    let shade = match face {
        Face::Top => 1.1,
        Face::Bottom => 0.55,
        Face::Left | Face::Right => 0.85,
        Face::Back | Face::Front => 0.95,
    };

    [base[0] * shade, base[1] * shade, base[2] * shade]
}

pub fn is_chunk_in_view(camera: &crate::camera::Camera, chunk: ChunkCoord) -> bool {
    use glam::Vec3;
    use voxel_core::{CHUNK_SIZE_X, CHUNK_HEIGHT, CHUNK_SIZE_Z};

    let center = chunk.world_origin();
    let offset = Vec3::new(
        center.x as f32 + (CHUNK_SIZE_X / 2) as f32,
        (CHUNK_HEIGHT / 2) as f32,
        center.z as f32 + (CHUNK_SIZE_Z / 2) as f32,
    ) - camera.transform.position;

    offset.x * offset.x + offset.z * offset.z <= (camera.view_distance * camera.view_distance) as f32
}

pub fn sample_chunk_surface(world: &voxel_world::World, coord: ChunkCoord) -> Option<voxel_core::BlockCoord> {
    use voxel_core::{BlockCoord, CHUNK_HEIGHT, CHUNK_SIZE_X, CHUNK_SIZE_Z};
    use voxel_world::AIR;

    let chunk = world.loaded_chunk(coord)?;
    let origin = coord.world_origin();

    for y in (0..CHUNK_HEIGHT).rev() {
        for z in 0..CHUNK_SIZE_Z {
            for x in 0..CHUNK_SIZE_X {
                if chunk.storage.get(x, y, z) != AIR {
                    return Some(BlockCoord::new(origin.x + x, y, origin.z + z));
                }
            }
        }
    }

    None
}