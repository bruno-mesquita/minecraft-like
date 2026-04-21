use crate::chunk::Chunk;
use crate::generator::TerrainGenerator;
use crate::region::RegionStore;
use crate::state::ChunkState;
use crate::visibility::nearest_visible_coords;
use crate::voxel::AIR;
use rustc_hash::FxHashMap;
use std::{collections::VecDeque, io, time::Instant};
use voxel_core::{
    BlockCoord, ChunkCoord, FrameBudget, FrameMetrics, StreamingConfig, WorkPhase, WorldConfig,
    WorldCounters,
};

#[derive(Debug)]
pub struct World {
    pub config: WorldConfig,
    pub streaming: StreamingConfig,
    pub generator: TerrainGenerator,
    pub store: RegionStore,
    pub manager: ChunkManager,
    loaded_chunks: FxHashMap<ChunkCoord, Chunk>,
    pub modified_blocks: FxHashMap<BlockCoord, crate::voxel::BlockId>,
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
            modified_blocks: FxHashMap::default(),
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
            
            // In the new architecture, we don't load dense chunks from disk for now.
            // We just "generate" the metadata shell.
            let chunk = self.generator.generate_chunk(coord);
            metrics.record_phase(WorkPhase::Generate, started.elapsed());

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

        if self.sample_block(coord).id == block_id {
            return;
        }

        self.modified_blocks.insert(coord, block_id);
        
        let chunk_coord = ChunkCoord::from_block(coord);
        self.manager.mark_meshing(chunk_coord);

        // Dirty neighbors if on boundary
        let origin = chunk_coord.world_origin();
        let lx = coord.x - origin.x;
        let lz = coord.z - origin.z;

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
            self.manager.mark_meshing(coord);
        }
    }

    pub fn sample_block(&self, block: BlockCoord) -> crate::voxel::Voxel {
        if !(0..self.config.vertical_height).contains(&block.y) {
            return crate::voxel::Voxel::new(AIR);
        }

        if let Some(&id) = self.modified_blocks.get(&block) {
            return crate::voxel::Voxel::new(id);
        }

        crate::voxel::Voxel::new(self.generator.block_at(block))
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

    pub fn save_dirty_chunks(&mut self, _metrics: &mut FrameMetrics) -> io::Result<usize> {
        // Saving logic needs to be updated for the new architecture (saving modified_blocks)
        // For now, we skip it.
        Ok(0)
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

    pub fn pop_pending_mesh(&mut self) -> Option<ChunkCoord> {
        self.pending_mesh.pop_front()
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