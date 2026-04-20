use crate::{chunk::Chunk, generator::TerrainGenerator, region::RegionStore, AIR};
use rustc_hash::FxHashMap;
use std::{collections::VecDeque, io, time::Instant};
use tracing::debug;
use voxel_core::{
    BlockCoord, ChunkCoord, FrameBudget, FrameMetrics, StreamingConfig, WorkPhase, WorldConfig,
    WorldCounters,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkState {
    Requested,
    Generating,
    Meshing,
    Resident,
    Evicting,
}

#[derive(Debug)]
pub struct ChunkManager {
    states: FxHashMap<ChunkCoord, ChunkState>,
    requested: VecDeque<ChunkCoord>,
    pending_mesh: VecDeque<ChunkCoord>,
}

impl ChunkManager {
    pub fn new() -> Self {
        Self {
            states: FxHashMap::default(),
            requested: VecDeque::new(),
            pending_mesh: VecDeque::new(),
        }
    }

    pub fn request(&mut self, coord: ChunkCoord) {
        if self.states.contains_key(&coord) {
            return;
        }

        self.states.insert(coord, ChunkState::Requested);
        self.requested.push_back(coord);
    }

    pub fn mark_generating(&mut self, coord: ChunkCoord) {
        self.states.insert(coord, ChunkState::Generating);
    }

    pub fn mark_meshing(&mut self, coord: ChunkCoord) {
        self.states.insert(coord, ChunkState::Meshing);
        self.pending_mesh.push_back(coord);
    }

    pub fn mark_resident(&mut self, coord: ChunkCoord) {
        self.states.insert(coord, ChunkState::Resident);
    }

    pub fn mark_evicting(&mut self, coord: ChunkCoord) {
        self.states.insert(coord, ChunkState::Evicting);
    }

    pub fn pop_requested(&mut self) -> Option<ChunkCoord> {
        self.requested.pop_front()
    }

    pub fn counters(&self) -> WorldCounters {
        let mut counters = WorldCounters::default();

        for state in self.states.values() {
            match state {
                ChunkState::Requested => counters.requested += 1,
                ChunkState::Generating => counters.generating += 1,
                ChunkState::Meshing => counters.meshing += 1,
                ChunkState::Resident => counters.resident += 1,
                ChunkState::Evicting => counters.evicting += 1,
            }
        }

        counters
    }

    pub fn has_state(&self, coord: ChunkCoord, expected: ChunkState) -> bool {
        self.states.get(&coord).copied() == Some(expected)
    }

    pub fn state(&self, coord: ChunkCoord) -> Option<ChunkState> {
        self.states.get(&coord).copied()
    }
}

#[derive(Debug)]
pub struct World {
    pub config: WorldConfig,
    pub streaming: StreamingConfig,
    generator: TerrainGenerator,
    pub store: RegionStore,
    pub manager: ChunkManager,
    loaded_chunks: FxHashMap<ChunkCoord, Chunk>,
}

impl World {
    pub fn new(
        config: WorldConfig,
        streaming: StreamingConfig,
        seed: u64,
        save_root: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            generator: TerrainGenerator::new(seed, config.sea_level),
            store: RegionStore::new(save_root),
            manager: ChunkManager::new(),
            loaded_chunks: FxHashMap::default(),
            config,
            streaming,
        }
    }

    pub fn request_visible_chunks(&mut self, center: ChunkCoord) {
        for coord in nearest_visible_coords(center, self.streaming.load_radius, self.streaming.max_active_chunks) {
            self.manager.request(coord);
        }
    }

    pub fn pump_generation(&mut self, metrics: &mut FrameMetrics) {
        let mut budget = FrameBudget::new(self.streaming.max_generation_jobs_per_frame);

        while budget.try_take(1) {
            let Some(coord) = self.manager.pop_requested() else {
                break;
            };

            self.manager.mark_generating(coord);
            let started = Instant::now();
            let chunk = match self.store.load_chunk(coord) {
                Ok(Some(chunk)) => {
                    metrics.record_phase(WorkPhase::Load, started.elapsed());
                    chunk
                }
                Ok(None) => {
                    let generated = self.generator.generate_chunk(coord);
                    metrics.record_phase(WorkPhase::Generate, started.elapsed());
                    generated
                }
                Err(error) => {
                    debug!(?coord, %error, "failed to load chunk from disk, regenerating");
                    let generated = self.generator.generate_chunk(coord);
                    metrics.record_phase(WorkPhase::Generate, started.elapsed());
                    generated
                }
            };

            self.loaded_chunks.insert(coord, chunk);
            self.manager.mark_meshing(coord);
        }

        metrics.counters = self.manager.counters();
    }

    pub fn mark_chunk_resident(&mut self, coord: ChunkCoord) {
        self.manager.mark_resident(coord);
    }

    pub fn evict_far_chunks(&mut self, center: ChunkCoord) {
        let keep_distance_sq = self.streaming.keep_radius * self.streaming.keep_radius;
        let mut nearest_loaded: Vec<_> = self
            .loaded_chunks
            .keys()
            .copied()
            .map(|coord| (center.distance_squared(coord), coord))
            .collect();
        nearest_loaded.sort_unstable_by_key(|&(distance_sq, coord)| (distance_sq, coord.x, coord.z));

        let keep_by_count: FxHashMap<_, _> = nearest_loaded
            .iter()
            .take(self.streaming.max_active_chunks)
            .map(|&(_, coord)| (coord, ()))
            .collect();

        let to_evict: Vec<_> = self
            .loaded_chunks
            .keys()
            .copied()
            .filter(|coord| center.distance_squared(*coord) > keep_distance_sq)
            .chain(
                self.loaded_chunks
                    .keys()
                    .copied()
                    .filter(|coord| !keep_by_count.contains_key(coord)),
            )
            .collect();

        for coord in to_evict {
            self.manager.mark_evicting(coord);
            self.loaded_chunks.remove(&coord);
        }
    }

    pub fn sample_block(&self, block: BlockCoord) -> crate::voxel::Voxel {
        if !(0..self.config.vertical_height).contains(&block.y) {
            return crate::voxel::Voxel::new(AIR);
        }

        let chunk_coord = ChunkCoord::from_block(block);
        let Some(chunk) = self.loaded_chunks.get(&chunk_coord) else {
            return crate::voxel::Voxel::new(AIR);
        };

        chunk.voxel_at_world(block)
    }

    pub fn loaded_chunk(&self, coord: ChunkCoord) -> Option<&Chunk> {
        self.loaded_chunks.get(&coord)
    }

    pub fn loaded_chunk_mut(&mut self, coord: ChunkCoord) -> Option<&mut Chunk> {
        self.loaded_chunks.get_mut(&coord)
    }

    pub fn loaded_chunks_iter(&self) -> impl Iterator<Item = (&ChunkCoord, &Chunk)> {
        self.loaded_chunks.iter()
    }

    pub fn save_dirty_chunks(&mut self, metrics: &mut FrameMetrics) -> io::Result<usize> {
        let started = Instant::now();
        let mut saved = 0;

        for chunk in self.loaded_chunks.values_mut() {
            if !chunk.dirty {
                continue;
            }

            self.store.save_chunk(chunk)?;
            chunk.dirty = false;
            saved += 1;
        }

        metrics.record_phase(WorkPhase::Save, started.elapsed());
        Ok(saved)
    }
}

fn nearest_visible_coords(center: ChunkCoord, radius: i32, max_chunks: usize) -> Vec<ChunkCoord> {
    let distance_sq_limit = radius * radius;
    let mut coords = Vec::new();

    for z in -radius..=radius {
        for x in -radius..=radius {
            let coord = ChunkCoord::new(center.x + x, center.z + z);
            let distance_sq = center.distance_squared(coord);
            if distance_sq <= distance_sq_limit {
                coords.push((distance_sq, coord));
            }
        }
    }

    coords.sort_unstable_by_key(|&(distance_sq, coord)| (distance_sq, coord.x, coord.z));
    coords
        .into_iter()
        .take(max_chunks)
        .map(|(_, coord)| coord)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_save_dir(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("minecraft-world-tests-{name}-{stamp}"))
    }

    #[test]
    fn generation_is_deterministic_for_same_seed() {
        let world_a = World::new(
            WorldConfig::default(),
            StreamingConfig::default(),
            42,
            temp_save_dir("a"),
        );
        let world_b = World::new(
            WorldConfig::default(),
            StreamingConfig::default(),
            42,
            temp_save_dir("b"),
        );

        let chunk_a = world_a.generator.generate_chunk(ChunkCoord::new(2, -4));
        let chunk_b = world_b.generator.generate_chunk(ChunkCoord::new(2, -4));
        assert_eq!(chunk_a.storage.as_slice(), chunk_b.storage.as_slice());
    }

    #[test]
    fn save_roundtrip_restores_chunk() {
        let dir = temp_save_dir("save");
        let mut world = World::new(WorldConfig::default(), StreamingConfig::default(), 7, &dir);
        let coord = ChunkCoord::new(0, 0);
        let mut metrics = FrameMetrics::default();

        world.manager.request(coord);
        world.pump_generation(&mut metrics);
        let chunk = world
            .loaded_chunks
            .get_mut(&coord)
            .expect("chunk should be generated");
        chunk.storage.set(0, 10, 0, crate::STONE);
        chunk.dirty = true;
        world.save_dirty_chunks(&mut metrics).expect("save should work");

        let reloaded = world
            .store
            .load_chunk(coord)
            .expect("load should work")
            .expect("chunk should exist on disk");
        assert_eq!(reloaded.storage.get(0, 10, 0), crate::STONE);
    }

    #[test]
    fn visible_chunk_requests_are_capped() {
        let coords = nearest_visible_coords(ChunkCoord::new(0, 0), 3, 16);

        assert_eq!(coords.len(), 16);
        assert_eq!(coords[0], ChunkCoord::new(0, 0));
    }
}
