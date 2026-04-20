use crate::player::Aabb;
use glam::Vec3;
use voxel_core::{BlockCoord, SimulationConfig};
use voxel_world::World;

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