use glam::Vec3;
use hecs::World as EntityWorld;
use serde::{Deserialize, Serialize};
use voxel_core::{BlockCoord, CameraTransform, SimulationConfig};
use voxel_world::World;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Aabb {
    pub half_extents: Vec3,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerInput {
    pub move_forward: f32,
    pub move_right: f32,
    pub look_delta: glam::Vec2,
    pub jump_pressed: bool,
    pub sprint_held: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerController {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    pub collider: Aabb,
}

impl Default for PlayerController {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 90.0, 0.0),
            velocity: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            grounded: false,
            collider: Aabb {
                half_extents: Vec3::new(0.4, 0.9, 0.4),
            },
        }
    }
}

impl PlayerController {
    pub fn tick(&mut self, world: &World, input: PlayerInput, config: &SimulationConfig, dt_seconds: f32) {
        self.yaw += input.look_delta.x;
        self.pitch = (self.pitch - input.look_delta.y).clamp(-1.54, 1.54);

        self.grounded = intersects_solid(world, self.position + Vec3::new(0.0, -0.05, 0.0), self.collider);
        if self.grounded && self.velocity.y < 0.0 {
            self.velocity.y = 0.0;
        }

        let movement = input.move_forward_right();
        let speed = if input.sprint_held {
            config.walk_speed * config.sprint_multiplier
        } else {
            config.walk_speed
        };

        let forward = self.forward_vector();
        let planar_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let right = planar_forward.cross(Vec3::Y).normalize_or_zero();
        let desired = (planar_forward * movement.x + right * movement.y).normalize_or_zero() * speed;

        self.velocity.x = desired.x;
        self.velocity.z = desired.z;

        if input.jump_pressed && self.grounded {
            self.velocity.y = config.jump_velocity;
            self.grounded = false;
        } else if !self.grounded {
            self.velocity.y -= config.gravity * dt_seconds;
        }

        self.position = move_axis(world, self.position, Vec3::new(self.velocity.x * dt_seconds, 0.0, 0.0), self.collider, &mut self.velocity, &mut self.grounded);
        self.position = move_axis(world, self.position, Vec3::new(0.0, self.velocity.y * dt_seconds, 0.0), self.collider, &mut self.velocity, &mut self.grounded);
        self.position = move_axis(world, self.position, Vec3::new(0.0, 0.0, self.velocity.z * dt_seconds), self.collider, &mut self.velocity, &mut self.grounded);
    }

    pub fn camera_transform(&self, config: &SimulationConfig) -> CameraTransform {
        CameraTransform {
            position: self.position + Vec3::Y * config.eye_height,
            forward: self.forward_vector(),
            up: Vec3::Y,
        }
    }

    pub fn forward_vector(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
        .normalize_or_zero()
    }
}

impl PlayerInput {
    fn move_forward_right(self) -> glam::Vec2 {
        glam::Vec2::new(self.move_forward, self.move_right)
    }
}

#[derive(Default)]
pub struct Simulation {
    pub entities: EntityWorld,
    pub player: PlayerController,
}

impl Simulation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(&mut self, world: &World, input: PlayerInput, config: &SimulationConfig, dt_seconds: f32) {
        self.player.tick(world, input, config, dt_seconds);
    }

    pub fn camera_transform(&self, config: &SimulationConfig) -> CameraTransform {
        self.player.camera_transform(config)
    }
}

impl std::fmt::Debug for Simulation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Simulation")
            .field("entities_len", &self.entities.len())
            .field("player", &self.player)
            .finish()
    }
}

fn move_axis(
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

fn intersects_solid(world: &World, position: Vec3, collider: Aabb) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use voxel_core::{ChunkCoord, FrameMetrics, StreamingConfig, WorldConfig};
    use voxel_world::{STONE, World as VoxelWorld};

    fn temp_save_dir() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("minecraft-sim-tests-{stamp}"))
    }

    #[test]
    fn player_does_not_cross_solid_voxel() {
        let mut world = VoxelWorld::new(WorldConfig::default(), StreamingConfig::default(), 1, temp_save_dir());
        let coord = ChunkCoord::new(0, 0);
        let mut metrics = FrameMetrics::default();

        world.manager.request(coord);
        world.pump_generation(&mut metrics);
        let chunk = world
            .loaded_chunk_mut(coord)
            .expect("chunk should be generated");
        chunk.storage.set(0, 88, 0, STONE);
        chunk.dirty = true;

        let mut player = PlayerController::default();
        player.position = Vec3::new(0.0, 88.8, -1.0);
        player.tick(
            &world,
            PlayerInput {
                move_right: 0.0,
                move_forward: -1.0,
                ..PlayerInput::default()
            },
            &SimulationConfig::default(),
            0.1,
        );

        assert!(player.position.z <= -1.0);
    }

    #[test]
    fn look_input_updates_orientation() {
        let mut player = PlayerController::default();
        player.tick(
            &VoxelWorld::new(WorldConfig::default(), StreamingConfig::default(), 1, temp_save_dir()),
            PlayerInput {
                look_delta: glam::Vec2::new(0.5, -0.25),
                ..PlayerInput::default()
            },
            &SimulationConfig::default(),
            1.0 / 120.0,
        );

        assert!(player.yaw > 0.0);
        assert!(player.pitch > 0.0);
    }

    #[test]
    fn strafe_input_uses_player_right_vector() {
        let world = VoxelWorld::new(WorldConfig::default(), StreamingConfig::default(), 1, temp_save_dir());
        let config = SimulationConfig::default();
        let mut player = PlayerController::default();

        player.tick(
            &world,
            PlayerInput {
                move_right: 1.0,
                ..PlayerInput::default()
            },
            &config,
            0.1,
        );
        assert!(player.position.x > 0.0);

        player.position = Vec3::ZERO;
        player.velocity = Vec3::ZERO;
        player.tick(
            &world,
            PlayerInput {
                move_right: -1.0,
                ..PlayerInput::default()
            },
            &config,
            0.1,
        );
        assert!(player.position.x < 0.0);
    }
}
