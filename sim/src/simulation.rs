use crate::player::{PlayerController, PlayerInput};
use hecs::World as EntityWorld;
use voxel_core::{CameraTransform, SimulationConfig};
use voxel_world::World;

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