use crate::chunk::Chunk;
use crate::generator::TerrainGenerator;
use crate::region::RegionStore;
use crate::state::ChunkState;
use crate::visibility::nearest_visible_coords;
use crate::voxel::AIR;
use rustc_hash::FxHashMap;
use std::{collections::VecDeque, io, time::Instant};
use tracing::debug;
use voxel_core::{
    BlockCoord, ChunkCoord, FrameBudget, FrameMetrics, StreamingConfig, WorkPhase, WorldConfig,
    WorldCounters,
};

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

    pub fn set_block(&mut self, coord: BlockCoord, block_id: crate::voxel::BlockId) {
        if !(0..self.config.vertical_height).contains(&coord.y) {
            return;
        }

        let chunk_coord = ChunkCoord::from_block(coord);
        let Some(chunk) = self.loaded_chunks.get_mut(&chunk_coord) else {
            return;
        };

        let origin = chunk_coord.world_origin();
        let lx = coord.x - origin.x;
        let ly = coord.y;
        let lz = coord.z - origin.z;

        if chunk.storage.get(lx, ly, lz) == block_id {
            return;
        }

        chunk.storage.set(lx, ly, lz, block_id);
        chunk.dirty = true;
        self.manager.mark_meshing(chunk_coord);

        // Dirty neighbors if on boundary
        if lx == 0 {
            self.dirty_chunk(ChunkCoord::new(chunk_coord.x - 1, chunk_coord.z));
        }
        if lx == voxel_core::CHUNK_SIZE_X - 1 {
            self.dirty_chunk(ChunkCoord::new(chunk_coord.x + 1, chunk_coord.z));
        }
        if lz == 0 {
            self.dirty_chunk(ChunkCoord::new(chunk_coord.x, chunk_coord.z - 1));
        }
        if lz == voxel_core::CHUNK_SIZE_Z - 1 {
            self.dirty_chunk(ChunkCoord::new(chunk_coord.x, chunk_coord.z + 1));
        }
    }

    fn dirty_chunk(&mut self, coord: ChunkCoord) {
        if self.loaded_chunks.contains_key(&coord) {
            // We don't set chunk.dirty = true here because voxel data didn't change (no need to save to disk)
            // but we do need to remesh it.
            self.manager.mark_meshing(coord);
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