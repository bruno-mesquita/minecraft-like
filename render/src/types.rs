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
    pub fn build(chunk: &voxel_world::Chunk) -> Self {
        use voxel_core::{CHUNK_HEIGHT, CHUNK_SIZE_X, CHUNK_SIZE_Z};
        use voxel_world::{Chunk, AIR};

        let mut vertices = Vec::new();

        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE_Z {
                for x in 0..CHUNK_SIZE_X {
                    let block_id = chunk.storage.get(x, y, z);
                    if block_id == AIR {
                        continue;
                    }

                    let world = chunk.coord.world_origin();
                    let wx = world.x + x;
                    let wz = world.z + z;

                    super::mesh::push_face_if_visible(chunk, &mut vertices, wx, y, wz, x - 1, y, z, block_id, Face::Left);
                    super::mesh::push_face_if_visible(chunk, &mut vertices, wx, y, wz, x + 1, y, z, block_id, Face::Right);
                    super::mesh::push_face_if_visible(chunk, &mut vertices, wx, y, wz, x, y - 1, z, block_id, Face::Bottom);
                    super::mesh::push_face_if_visible(chunk, &mut vertices, wx, y, wz, x, y + 1, z, block_id, Face::Top);
                    super::mesh::push_face_if_visible(chunk, &mut vertices, wx, y, wz, x, y, z - 1, block_id, Face::Back);
                    super::mesh::push_face_if_visible(chunk, &mut vertices, wx, y, wz, x, y, z + 1, block_id, Face::Front);
                }
            }
        }

        Self {
            chunk: chunk.coord,
            visible_faces: vertices.len() as u32,
            vertices,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxel_core::ChunkCoord;
    use voxel_world::{Chunk, STONE};

    #[test]
    fn isolated_block_emits_six_faces() {
        let mut chunk = Chunk::new(ChunkCoord::new(0, 0));
        chunk.storage.set(1, 1, 1, STONE);

        let mesh = ChunkMesh::build(&chunk);
        assert_eq!(mesh.visible_faces, 6);
    }

    #[test]
    fn adjacent_blocks_cull_internal_faces() {
        let mut chunk = Chunk::new(ChunkCoord::new(0, 0));
        chunk.storage.set(1, 1, 1, STONE);
        chunk.storage.set(2, 1, 1, STONE);

        let mesh = ChunkMesh::build(&chunk);
        assert_eq!(mesh.visible_faces, 10);
    }
}