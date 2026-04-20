use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkPhase {
    Generate,
    Mesh,
    Upload,
    Save,
    Load,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorldCounters {
    pub requested: usize,
    pub generating: usize,
    pub meshing: usize,
    pub resident: usize,
    pub evicting: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMetrics {
    pub generation_time: Duration,
    pub mesh_time: Duration,
    pub upload_time: Duration,
    pub save_time: Duration,
    pub load_time: Duration,
    pub counters: WorldCounters,
}

impl Default for FrameMetrics {
    fn default() -> Self {
        Self {
            generation_time: Duration::ZERO,
            mesh_time: Duration::ZERO,
            upload_time: Duration::ZERO,
            save_time: Duration::ZERO,
            load_time: Duration::ZERO,
            counters: WorldCounters::default(),
        }
    }
}

impl FrameMetrics {
    pub fn record_phase(&mut self, phase: WorkPhase, elapsed: Duration) {
        match phase {
            WorkPhase::Generate => self.generation_time += elapsed,
            WorkPhase::Mesh => self.mesh_time += elapsed,
            WorkPhase::Upload => self.upload_time += elapsed,
            WorkPhase::Save => self.save_time += elapsed,
            WorkPhase::Load => self.load_time += elapsed,
        }
    }
}
