use crate::voxel::{AIR, BlockId, Voxel};
use serde::{Deserialize, Serialize};
use voxel_core::{BlockCoord, ChunkCoord, CHUNK_HEIGHT, CHUNK_SIZE_X, CHUNK_SIZE_Z};

const CHUNK_VOLUME: usize = (CHUNK_SIZE_X as usize) * (CHUNK_HEIGHT as usize) * (CHUNK_SIZE_Z as usize);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkStorage {
    blocks: Vec<BlockId>,
}

impl Default for ChunkStorage {
    fn default() -> Self {
        Self {
            blocks: vec![AIR; CHUNK_VOLUME],
        }
    }
}

impl ChunkStorage {
    pub fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        if !(0..CHUNK_SIZE_X).contains(&x) || !(0..CHUNK_HEIGHT).contains(&y) || !(0..CHUNK_SIZE_Z).contains(&z) {
            return AIR;
        }

        self.blocks[index(x, y, z)]
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, id: BlockId) {
        if !(0..CHUNK_SIZE_X).contains(&x) || !(0..CHUNK_HEIGHT).contains(&y) || !(0..CHUNK_SIZE_Z).contains(&z) {
            return;
        }

        let idx = index(x, y, z);
        self.blocks[idx] = id;
    }

    pub fn as_slice(&self) -> &[BlockId] {
        &self.blocks
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub coord: ChunkCoord,
    pub storage: ChunkStorage,
    pub dirty: bool,
}

impl Chunk {
    pub fn new(coord: ChunkCoord) -> Self {
        Self {
            coord,
            storage: ChunkStorage::default(),
            dirty: false,
        }
    }

    pub fn voxel_at_world(&self, coord: BlockCoord) -> Voxel {
        let origin = self.coord.world_origin();
        let local_x = coord.x - origin.x;
        let local_y = coord.y;
        let local_z = coord.z - origin.z;
        Voxel::new(self.storage.get(local_x, local_y, local_z))
    }
}

fn index(x: i32, y: i32, z: i32) -> usize {
    ((y as usize * CHUNK_SIZE_Z as usize + z as usize) * CHUNK_SIZE_X as usize) + x as usize
}
