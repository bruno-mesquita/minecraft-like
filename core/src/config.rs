use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub seed: u64,
    pub world: WorldConfig,
    pub streaming: StreamingConfig,
    pub render: RenderConfig,
    pub simulation: SimulationConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            seed: 0xC0FFEE,
            world: WorldConfig::default(),
            streaming: StreamingConfig::default(),
            render: RenderConfig::default(),
            simulation: SimulationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldConfig {
    pub vertical_height: i32,
    pub sea_level: i32,
    pub region_size: i32,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            vertical_height: 256,
            sea_level: 64,
            region_size: 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    pub load_radius: i32,
    pub keep_radius: i32,
    pub max_active_chunks: usize,
    pub max_generation_jobs_per_frame: usize,
    pub max_mesh_jobs_per_frame: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            load_radius: 5,
            keep_radius: 7,
            max_active_chunks: 128,
            max_generation_jobs_per_frame: 4,
            max_mesh_jobs_per_frame: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    pub window_width: u32,
    pub window_height: u32,
    pub fov_degrees: f32,
    pub near_plane: f32,
    pub far_plane: f32,
    pub mouse_sensitivity: f32,
    pub max_frames_in_flight: usize,
    pub occlusion_culling: bool,
    pub vsync: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            window_width: 1280,
            window_height: 720,
            fov_degrees: 75.0,
            near_plane: 0.1,
            far_plane: 512.0,
            mouse_sensitivity: 0.0025,
            max_frames_in_flight: 2,
            occlusion_culling: false,
            vsync: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub walk_speed: f32,
    pub sprint_multiplier: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
    pub eye_height: f32,
    pub fixed_dt_seconds: f32,
    pub max_health: f32,
    pub max_stamina: f32,
    pub max_hunger: f32,
    pub stamina_regen_rate: f32,
    pub hunger_decay_rate: f32,
    pub health_regen_rate: f32,
    pub starvation_damage_rate: f32,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            walk_speed: 6.0,
            sprint_multiplier: 1.6,
            jump_velocity: 8.5,
            gravity: 24.0,
            eye_height: 0.6,
            fixed_dt_seconds: 1.0 / 120.0,
            max_health: 20.0,
            max_stamina: 100.0,
            max_hunger: 20.0,
            stamina_regen_rate: 10.0,
            hunger_decay_rate: 0.05,
            health_regen_rate: 1.0,
            starvation_damage_rate: 1.0,
        }
    }
}
