use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemKind {
    Sword,
    Pickaxe,
    Axe,
    Hoe,
    Shovel,
}

impl ItemKind {
    pub fn damage(&self) -> u8 {
        match self {
            ItemKind::Sword => 4,
            ItemKind::Pickaxe => 3,
            ItemKind::Axe => 5,
            ItemKind::Hoe => 1,
            ItemKind::Shovel => 2,
        }
    }

    pub fn mining_speed(&self, block_id: u8) -> u8 {
        let base = match self {
            ItemKind::Sword => 1,
            ItemKind::Pickaxe => 10,
            ItemKind::Axe => 8,
            ItemKind::Hoe => 2,
            ItemKind::Shovel => 6,
        };
        let multiplier = match block_id {
            1 | 2 => 2,
            3 => 1,
            _ => 1,
        };
        base * multiplier
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub kind: ItemKind,
    pub durability: u16,
}

impl Item {
    pub const fn new(kind: ItemKind) -> Self {
        Self {
            kind,
            durability: 100,
        }
    }

    pub const fn with_durability(kind: ItemKind, durability: u16) -> Self {
        Self { kind, durability }
    }
}