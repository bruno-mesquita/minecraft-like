use glam::Vec3;
use serde::{Deserialize, Serialize};
use voxel_core::{CameraTransform, Item, ItemKind, SimulationConfig};
use voxel_world::World;

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerInput {
    pub move_forward: f32,
    pub move_right: f32,
    pub look_delta: glam::Vec2,
    pub jump_pressed: bool,
    pub sprint_held: bool,
    pub action_primary: bool,
    pub action_secondary: bool,
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
pub struct PlayerAttributes {
    pub health: f32,
    pub stamina: f32,
    pub hunger: f32,
    pub experience: u32,
    pub level: u32,
}

impl PlayerAttributes {
    pub fn new(config: &voxel_core::SimulationConfig) -> Self {
        Self {
            health: config.max_health,
            stamina: config.max_stamina,
            hunger: config.max_hunger,
            experience: 0,
            level: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlayerController {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    pub collider: Aabb,
    pub equipped_item: Option<Item>,
    pub attributes: PlayerAttributes,
}

impl Default for PlayerController {
    fn default() -> Self {
        let config = voxel_core::SimulationConfig::default();
        Self {
            position: Vec3::new(0.0, 90.0, 0.0),
            velocity: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            grounded: false,
            collider: Aabb {
                half_extents: Vec3::new(0.4, 0.9, 0.4),
            },
            equipped_item: Some(Item::new(ItemKind::Sword)),
            attributes: PlayerAttributes::new(&config),
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
        let is_moving = movement.length_squared() > 0.0;
        let is_sprinting = input.sprint_held && is_moving && self.attributes.stamina > 0.0;

        let speed = if is_sprinting {
            config.walk_speed * config.sprint_multiplier
        } else {
            config.walk_speed
        };

        // Update attributes
        if is_sprinting {
            self.attributes.stamina = (self.attributes.stamina - 20.0 * dt_seconds).max(0.0);
        } else {
            self.attributes.stamina = (self.attributes.stamina + config.stamina_regen_rate * dt_seconds).min(config.max_stamina);
        }

        self.attributes.hunger = (self.attributes.hunger - config.hunger_decay_rate * dt_seconds).max(0.0);

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

    pub fn attack_damage(&self) -> u8 {
        match &self.equipped_item {
            Some(item) => item.kind.damage(),
            None => 1,
        }
    }

    pub fn mining_speed(&self, block_id: u8) -> u8 {
        match &self.equipped_item {
            Some(item) => item.kind.mining_speed(block_id),
            None => 1,
        }
    }
}