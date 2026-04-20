use crate::{
    chunk::Chunk,
    voxel::{AIR, DIRT, GRASS, STONE},
};
use voxel_core::{ChunkCoord, CHUNK_HEIGHT, CHUNK_SIZE_X, CHUNK_SIZE_Z};

#[derive(Debug, Clone)]
pub struct TerrainGenerator {
    seed: u64,
    sea_level: i32,
}

impl TerrainGenerator {
    pub const fn new(seed: u64, sea_level: i32) -> Self {
        Self { seed, sea_level }
    }

    pub fn generate_chunk(&self, coord: ChunkCoord) -> Chunk {
        let mut chunk = Chunk::new(coord);
        let origin = coord.world_origin();

        for z in 0..CHUNK_SIZE_Z {
            for x in 0..CHUNK_SIZE_X {
                let world_x = origin.x + x;
                let world_z = origin.z + z;
                let height = self.height_at(world_x, world_z);

                for y in 0..CHUNK_HEIGHT {
                    let id = match y.cmp(&height) {
                        std::cmp::Ordering::Greater => AIR,
                        std::cmp::Ordering::Equal => GRASS,
                        std::cmp::Ordering::Less if y >= height - 3 => DIRT,
                        std::cmp::Ordering::Less => STONE,
                    };

                    chunk.storage.set(x, y, z, id);
                }
            }
        }

        chunk
    }

    pub fn height_at(&self, world_x: i32, world_z: i32) -> i32 {
        let base = self.sea_level;
        let large = signed_hash(self.seed, world_x / 32, world_z / 32) % 22;
        let detail = signed_hash(self.seed ^ 0x9E3779B97F4A7C15, world_x / 8, world_z / 8) % 7;
        (base + large + detail).clamp(8, CHUNK_HEIGHT - 2)
    }
}

fn signed_hash(seed: u64, x: i32, z: i32) -> i32 {
    let mut value = seed
        ^ (x as u64).wrapping_mul(0x9E3779B185EBCA87)
        ^ (z as u64).wrapping_mul(0xC2B2AE3D27D4EB4F);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58476D1CE4E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D049BB133111EB);
    value ^= value >> 31;

    (value % 37) as i32 - 18
}
