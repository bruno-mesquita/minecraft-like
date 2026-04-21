use bytemuck::{Pod, Zeroable};
use voxel_world::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Face {
    Left,
    Right,
    Bottom,
    Top,
    Back,
    Front,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaceVertex {
    pub position: [i32; 3],
    pub block_id: BlockId,
    pub face: Face,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkMesh {
    pub chunk: voxel_core::ChunkCoord,
    pub vertices: Vec<FaceVertex>,
    pub visible_faces: u32,
}

impl ChunkMesh {
    pub fn build(world: &voxel_world::World, coord: voxel_core::ChunkCoord) -> Self {
        use voxel_core::{CHUNK_HEIGHT, CHUNK_SIZE_X, CHUNK_SIZE_Z, BlockCoord};
        use voxel_world::AIR;

        let mut vertices = Vec::new();
        let origin = coord.world_origin();

        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE_Z {
                for x in 0..CHUNK_SIZE_X {
                    let wx = origin.x + x;
                    let wz = origin.z + z;
                    let block_coord = BlockCoord::new(wx, y, wz);
                    
                    let block_id = world.sample_block(block_coord).id;
                    if block_id == AIR {
                        continue;
                    }

                    super::mesh::push_face_if_visible(world, &mut vertices, block_coord, BlockCoord::new(wx - 1, y, wz), block_id, Face::Left);
                    super::mesh::push_face_if_visible(world, &mut vertices, block_coord, BlockCoord::new(wx + 1, y, wz), block_id, Face::Right);
                    super::mesh::push_face_if_visible(world, &mut vertices, block_coord, BlockCoord::new(wx, y - 1, wz), block_id, Face::Bottom);
                    super::mesh::push_face_if_visible(world, &mut vertices, block_coord, BlockCoord::new(wx, y + 1, wz), block_id, Face::Top);
                    super::mesh::push_face_if_visible(world, &mut vertices, block_coord, BlockCoord::new(wx, y, wz - 1), block_id, Face::Back);
                    super::mesh::push_face_if_visible(world, &mut vertices, block_coord, BlockCoord::new(wx, y, wz + 1), block_id, Face::Front);
                }
            }
        }

        Self {
            chunk: coord,
            visible_faces: vertices.len() as u32,
            vertices,
        }
    }
}
