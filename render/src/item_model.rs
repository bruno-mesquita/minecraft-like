use crate::gpu::GpuVertex;
use glam::{Vec2, Vec3};
use voxel_core::ItemKind;

pub struct ItemModel {
    pub vertices: Vec<GpuVertex>,
    pub indices: Vec<u32>,
}

impl ItemModel {
    pub fn from_kind(kind: ItemKind) -> Self {
        match kind {
            ItemKind::Sword => Self::sword(),
            ItemKind::Pickaxe => Self::pickaxe(),
            ItemKind::Axe => Self::axe(),
            ItemKind::Hoe => Self::hoe(),
            ItemKind::Shovel => Self::shovel(),
        }
    }

    fn sword() -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let blade = BoxBuilder::new(Vec3::new(0.3, 0.0, 0.0), Vec3::new(0.15, 0.6, 0.03))
            .with_color([0.8, 0.8, 0.9])
            .build(&mut vertices, &mut indices);

        let handle = BoxBuilder::new(Vec3::new(0.3, -0.45, 0.0), Vec3::new(0.04, 0.4, 0.04]))
            .with_color([0.55, 0.35, 0.2])
            .build(&mut vertices, &mut indices);

        let pommel = BoxBuilder::new(Vec3::new(0.3, -0.65, 0.0), Vec3::new(0.06, 0.1, 0.06]))
            .with_color([0.7, 0.7, 0.3])
            .build(&mut vertices, &mut indices);

        Self { vertices, indices }
    }

    fn pickaxe() -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let head = BoxBuilder::new(Vec3::new(0.3, 0.25, 0.0), Vec3::new(0.35, 0.08, 0.04]))
            .with_color([0.7, 0.7, 0.7])
            .build(&mut vertices, &mut indices);

        let handle = BoxBuilder::new(Vec3::new(0.3, -0.2, 0.0), Vec3::new(0.04, 0.5, 0.04]))
            .with_color([0.55, 0.35, 0.2])
            .build(&mut vertices, &mut indices);

        Self { vertices, indices }
    }

    fn axe() -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let head = BoxBuilder::new(Vec3::new(0.3, 0.15, 0.0), Vec3::new(0.25, 0.1, 0.04]))
            .with_color([0.7, 0.7, 0.7])
            .build(&mut vertices, &mut indices);

        let handle = BoxBuilder::new(Vec3::new(0.3, -0.2, 0.0), Vec3::new(0.04, 0.45, 0.04]))
            .with_color([0.55, 0.35, 0.2])
            .build(&mut vertices, &mut indices);

        Self { vertices, indices }
    }

    fn hoe() -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let head = BoxBuilder::new(Vec3::new(0.3, 0.2, 0.0), Vec3::new(0.25, 0.06, 0.04]))
            .with_color([0.7, 0.7, 0.7])
            .build(&mut vertices, &mut indices);

        let handle = BoxBuilder::new(Vec3::new(0.3, -0.2, 0.0), Vec3::new(0.04, 0.45, 0.04]))
            .with_color([0.55, 0.35, 0.2])
            .build(&mut vertices, &mut indices);

        Self { vertices, indices }
    }

    fn shovel() -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let head = BoxBuilder::new(Vec3::new(0.3, 0.25, 0.0), Vec3::new(0.15, 0.2, 0.03]))
            .with_color([0.75, 0.75, 0.75])
            .build(&mut vertices, &mut indices);

        let handle = BoxBuilder::new(Vec3::new(0.3, -0.2, 0.0), Vec3::new(0.035, 0.45, 0.035]))
            .with_color([0.55, 0.35, 0.2])
            .build(&mut vertices, &mut indices);

        Self { vertices, indices }
    }
}

struct BoxBuilder {
    center: Vec3,
    half_extents: Vec3,
    color: [f32; 3],
}

impl BoxBuilder {
    fn new(center: Vec3, half_extents: Vec3) -> Self {
        Self {
            center,
            half_extents,
            color: [0.5, 0.5, 0.5],
        }
    }

    fn with_color(mut self, color: [f32; 3]) -> Self {
        self.color = color;
        self
    }

    fn build(self, vertices: &mut Vec<GpuVertex>, indices: &mut Vec<u32>) -> u32 {
        let base = vertices.len() as u32;
        let he = self.half_extents;
        let c = self.center;

        let mut add_quad = |p1, p2, p3, p4: Vec3, n: Vec3| {
            let v1 = GpuVertex {
                position: p1,
                normal: n,
                color: self.color,
            };
            let v2 = GpuVertex {
                position: p2,
                normal: n,
                color: self.color,
            };
            let v3 = GpuVertex {
                position: p3,
                normal: n,
                color: self.color,
            };
            let v4 = GpuVertex {
                position: p4,
                normal: n,
                color: self.color,
            };
            let idx = vertices.len() as u32;
            vertices.extend([v1, v2, v3, v4]);
            indices.extend([idx, idx + 1, idx + 2, idx, idx + 2, idx + 3]);
        };

        let px = Vec3::X * he.x;
        let py = Vec3::Y * he.y;
        let pz = Vec3::Z * he.z;

        add_quad(c - px - py - pz, c - px + py - pz, c + px + py - pz, c + px - py - pz, -Vec3::Z);
        add_quad(c + px - py + pz, c + px + py + pz, c - px + py + pz, c - px - py + pz, Vec3::Z);
        add_quad(c - px - py + pz, c - px + py + pz, c - px + py - pz, c - px - py - pz, -Vec3::X);
        add_quad(c + px - py - pz, c + px + py - pz, c + px + py + pz, c + px - py + pz, Vec3::X);
        add_quad(c - px + py - pz, c - px + py + pz, c + px + py + pz, c + px + py - pz, Vec3::Y);
        add_quad(c - px - py + pz, c - px - py - pz, c + px - py - pz, c + px - py + pz, -Vec3::Y);

        6
    }
}