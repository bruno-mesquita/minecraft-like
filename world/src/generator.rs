use crate::{
    chunk::Chunk,
    voxel::{AIR, DIRT, GRASS, BlockId},
};
use voxel_core::{BlockCoord, ChunkCoord};

#[derive(Debug, Clone)]
pub struct TerrainGenerator {
    pub sea_level: i32,
}

impl TerrainGenerator {
    pub const fn new(_seed: u64, sea_level: i32) -> Self {
        Self { sea_level }
    }

    pub fn generate_chunk(&self, coord: ChunkCoord) -> Chunk {
        // Now generation is purely procedural, so we just return a chunk metadata shell
        Chunk::new(coord)
    }

    pub fn block_at(&self, coord: BlockCoord) -> BlockId {
        if coord.y == self.sea_level {
            GRASS
        } else if coord.y < self.sea_level && coord.y > self.sea_level - 4 {
            DIRT
        } else if coord.y <= self.sea_level - 4 {
            crate::voxel::STONE
        } else {
            AIR
        }
    }
}
