use crate::player::Aabb;
use glam::{IVec3, Vec3};
use voxel_core::BlockCoord;
use voxel_world::World;

#[derive(Debug, Clone, Copy)]
pub struct RaycastHit {
    pub coord: BlockCoord,
    pub normal: IVec3,
}

pub fn raycast(world: &World, origin: Vec3, direction: Vec3, max_dist: f32) -> Option<RaycastHit> {
    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    let step_x = if direction.x >= 0.0 { 1 } else { -1 };
    let step_y = if direction.y >= 0.0 { 1 } else { -1 };
    let step_z = if direction.z >= 0.0 { 1 } else { -1 };

    let t_delta_x = if direction.x != 0.0 { direction.x.abs().recip() } else { f32::INFINITY };
    let t_delta_y = if direction.y != 0.0 { direction.y.abs().recip() } else { f32::INFINITY };
    let t_delta_z = if direction.z != 0.0 { direction.z.abs().recip() } else { f32::INFINITY };

    let mut t_max_x = if direction.x > 0.0 {
        (x as f32 + 1.0 - origin.x) * t_delta_x
    } else if direction.x < 0.0 {
        (origin.x - x as f32) * t_delta_x
    } else {
        f32::INFINITY
    };

    let mut t_max_y = if direction.y > 0.0 {
        (y as f32 + 1.0 - origin.y) * t_delta_y
    } else if direction.y < 0.0 {
        (origin.y - y as f32) * t_delta_y
    } else {
        f32::INFINITY
    };

    let mut t_max_z = if direction.z > 0.0 {
        (z as f32 + 1.0 - origin.z) * t_delta_z
    } else if direction.z < 0.0 {
        (origin.z - z as f32) * t_delta_z
    } else {
        f32::INFINITY
    };

    let normal;

    // Check start block
    if world.sample_block(BlockCoord::new(x, y, z)).is_solid() {
        return Some(RaycastHit {
            coord: BlockCoord::new(x, y, z),
            normal: IVec3::ZERO,
        });
    }

    let mut nx = 0;
    let mut ny = 0;
    let mut nz = 0;

    loop {
        if t_max_x < t_max_y {
            if t_max_x < t_max_z {
                if t_max_x > max_dist { break; }
                x += step_x;
                t_max_x += t_delta_x;
                nx = -step_x; ny = 0; nz = 0;
            } else {
                if t_max_z > max_dist { break; }
                z += step_z;
                t_max_z += t_delta_z;
                nx = 0; ny = 0; nz = -step_z;
            }
        } else {
            if t_max_y < t_max_z {
                if t_max_y > max_dist { break; }
                y += step_y;
                t_max_y += t_delta_y;
                nx = 0; ny = -step_y; nz = 0;
            } else {
                if t_max_z > max_dist { break; }
                z += step_z;
                t_max_z += t_delta_z;
                nx = 0; ny = 0; nz = -step_z;
            }
        }

        let coord = BlockCoord::new(x, y, z);
        if world.sample_block(coord).is_solid() {
            normal = IVec3::new(nx, ny, nz);
            return Some(RaycastHit { coord, normal });
        }
    }

    None
}

pub fn move_axis(
    world: &World,
    current: Vec3,
    delta: Vec3,
    collider: Aabb,
    velocity: &mut Vec3,
    grounded: &mut bool,
) -> Vec3 {
    if delta == Vec3::ZERO {
        return current;
    }

    let candidate = current + delta;
    if intersects_solid(world, candidate, collider) {
        if delta.y < 0.0 {
            *grounded = true;
        }

        if delta.y != 0.0 {
            velocity.y = 0.0;
        } else if delta.x != 0.0 {
            velocity.x = 0.0;
        } else if delta.z != 0.0 {
            velocity.z = 0.0;
        }

        current
    } else {
        candidate
    }
}

pub fn intersects_solid(world: &World, position: Vec3, collider: Aabb) -> bool {
    let min = position - collider.half_extents;
    let max = position + collider.half_extents;

    let min_x = min.x.floor() as i32;
    let max_x = max.x.floor() as i32;
    let min_y = min.y.floor() as i32;
    let max_y = max.y.floor() as i32;
    let min_z = min.z.floor() as i32;
    let max_z = max.z.floor() as i32;

    for z in min_z..=max_z {
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if world.sample_block(BlockCoord::new(x, y, z)).is_solid() {
                    return true;
                }
            }
        }
    }

    false
}