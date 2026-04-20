use serde::{Deserialize, Serialize};

pub type BlockId = u16;

pub const AIR: BlockId = 0;
pub const GRASS: BlockId = 1;
pub const DIRT: BlockId = 2;
pub const STONE: BlockId = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voxel {
    pub id: BlockId,
}

impl Voxel {
    pub const fn new(id: BlockId) -> Self {
        Self { id }
    }

    pub const fn is_solid(self) -> bool {
        self.id != AIR
    }
}
