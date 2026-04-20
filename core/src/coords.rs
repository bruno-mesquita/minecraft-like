use glam::IVec3;
use serde::{Deserialize, Serialize};

pub const CHUNK_SIZE_X: i32 = 32;
pub const CHUNK_SIZE_Z: i32 = 32;
pub const CHUNK_HEIGHT: i32 = 256;
pub const REGION_SIZE: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn as_ivec3(self) -> IVec3 {
        IVec3::new(self.x, self.y, self.z)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

impl ChunkCoord {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub fn from_block(block: BlockCoord) -> Self {
        Self {
            x: block.x.div_euclid(CHUNK_SIZE_X),
            z: block.z.div_euclid(CHUNK_SIZE_Z),
        }
    }

    pub fn to_region(self) -> RegionCoord {
        RegionCoord {
            x: self.x.div_euclid(REGION_SIZE),
            z: self.z.div_euclid(REGION_SIZE),
        }
    }

    pub fn world_origin(self) -> BlockCoord {
        BlockCoord::new(self.x * CHUNK_SIZE_X, 0, self.z * CHUNK_SIZE_Z)
    }

    pub fn distance_squared(self, other: Self) -> i32 {
        let dx = self.x - other.x;
        let dz = self.z - other.z;
        dx * dx + dz * dz
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegionCoord {
    pub x: i32,
    pub z: i32,
}

impl RegionCoord {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}
