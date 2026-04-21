use crate::gpu::GpuVertex;
use crate::types::{ChunkMesh, Face, FaceVertex};
use voxel_core::ChunkCoord;
use voxel_world::{BlockId, AIR};
use wgpu::util::DeviceExt;

pub fn push_face_if_visible(
    world: &voxel_world::World,
    vertices: &mut Vec<FaceVertex>,
    coord: voxel_core::BlockCoord,
    neighbor: voxel_core::BlockCoord,
    block_id: BlockId,
    face: Face,
) {
    if world.sample_block(neighbor).id != AIR {
        return;
    }

    vertices.push(FaceVertex {
        position: [coord.x, coord.y, coord.z],
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
            
            let [x, y, z] = face.position;
            let x = x as f32;
            let y = y as f32;
            let z = z as f32;
            let color = face_color(face.block_id, face.face);

            let face_vertices = match face.face {
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
            };

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
    use voxel_core::{CHUNK_SIZE_X, CHUNK_SIZE_Z, CHUNK_HEIGHT};

    let center = chunk.world_origin();
    // Use the actual center of the chunk's 3D volume
    let chunk_center = Vec3::new(
        center.x as f32 + (CHUNK_SIZE_X / 2) as f32,
        (CHUNK_HEIGHT / 2) as f32,
        center.z as f32 + (CHUNK_SIZE_Z / 2) as f32,
    );
    
    let offset = chunk_center - camera.transform.position;
    
    // 1. Distance check (XZ only, as chunks are vertical columns)
    let offset_xz = Vec3::new(offset.x, 0.0, offset.z);
    let dist_sq_xz = offset_xz.length_squared();
    let view_dist = camera.view_distance as f32;
    // Radius for XZ footprint (diagonal of 16x16) is ~22.6
    let footprint_radius = 23.0;
    
    if dist_sq_xz > (view_dist + footprint_radius).powi(2) {
        return false;
    }
    
    // 2. Frustum-ish check using a bounding sphere for the whole chunk column.
    // Chunk diagonal radius: sqrt(16^2 + 128^2 + 16^2) ≈ 130
    let chunk_radius = 130.0;
    let dist = offset.length();

    // If we are inside or very close to the chunk's bounding sphere, it's visible.
    if dist < chunk_radius {
        return true;
    }

    // Dot product check with the chunk's bounding sphere.
    // If the angle between forward and vector-to-center is too large, 
    // we check if the sphere still intersects the vision cone.
    let dot = camera.transform.forward.dot(offset / dist);
    
    // We use a very permissive threshold. 
    // For a sphere of radius R at distance D, the angular radius is arcsin(R/D).
    // The cos of the max angle is roughly sqrt(1 - (R/D)^2).
    // But for simplicity and safety, we just allow a wider FOV.
    // cos(80 degrees) is ~0.17. We use -0.2 to be extra safe.
    let min_dot = -0.2; 
    
    if dot < min_dot {
        // Even if the center is "behind", a large chunk might still be visible
        // if its radius is large enough to cross into the front.
        if dot * dist + chunk_radius < 0.0 {
            return false;
        }
    }

    true
}

pub fn sample_chunk_surface(world: &voxel_world::World, coord: ChunkCoord) -> Option<voxel_core::BlockCoord> {
    use voxel_core::{BlockCoord, CHUNK_HEIGHT, CHUNK_SIZE_X, CHUNK_SIZE_Z};
    use voxel_world::AIR;

    let origin = coord.world_origin();

    for y in (0..CHUNK_HEIGHT).rev() {
        for z in 0..CHUNK_SIZE_Z {
            for x in 0..CHUNK_SIZE_X {
                let block_coord = BlockCoord::new(origin.x + x, y, origin.z + z);
                if world.sample_block(block_coord).id != AIR {
                    return Some(block_coord);
                }
            }
        }
    }

    None
}