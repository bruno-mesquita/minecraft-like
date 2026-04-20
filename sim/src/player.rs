use glam::Vec3;
use serde::{Deserialize, Serialize};
use voxel_core::{BlockCoord, CameraTransform, SimulationConfig};
use voxel_world::World;

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerInput {
    pub move_forward: f32,
    pub move_right: f32,
    pub look_delta: glam::Vec2,
    pub jump_pressed: bool,
    pub sprint_held: bool,
}

impl PlayerInput {
    pub fn move_forward_right(self) -> glam::Vec2 {
        glam::Vec2::new(self.move_forward, self.move_right)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Aabb {
    pub half_extents: Vec3,
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
        use super::physics::intersects_solid;

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

        use super::physics::move_axis;
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