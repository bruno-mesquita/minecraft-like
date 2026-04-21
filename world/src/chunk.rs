use serde::{Deserialize, Serialize};
use voxel_core::ChunkCoord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub coord: ChunkCoord,
    pub dirty: bool,
}

impl Chunk {
    pub fn new(coord: ChunkCoord) -> Self {
        Self {
            coord,
            dirty: false,
        }
    }
}
