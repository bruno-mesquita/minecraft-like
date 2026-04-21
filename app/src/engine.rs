use std::{sync::Arc, time::{Duration, Instant}};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tracing::info;
use voxel_core::{BlockCoord, ChunkCoord, EngineConfig, FrameMetrics};
use voxel_render::Renderer;
use voxel_sim::{PlayerController, PlayerInput, Simulation};
use voxel_world::World;
use winit::dpi::PhysicalSize;

const MAX_SIMULATION_STEPS_PER_FRAME: usize = 32;

pub struct Engine {
    config: EngineConfig,
    renderer: Renderer,
    world: World,
    simulation: Simulation,
    metrics: FrameMetrics,
    last_frame_at: Instant,
    accumulator: f32,

    // Metrics tracking
    sys: System,
    last_metrics_update: Instant,
    current_fps: f32,
    frame_count: usize,
    frame_timer: Instant,
}

impl Engine {
    pub async fn new(window: Arc<winit::window::Window>, config: EngineConfig) -> Result<Self, String> {
        let renderer = Renderer::new(window, &config.render).await?;
        let world = World::new(
            config.world.clone(),
            config.streaming.clone(),
            config.seed,
            "saves/default",
        );

        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );

        Ok(Self {
            config,
            renderer,
            world,
            simulation: Simulation::new(),
            metrics: FrameMetrics::default(),
            last_frame_at: Instant::now(),
            accumulator: 0.0,
            sys,
            last_metrics_update: Instant::now(),
            current_fps: 0.0,
            frame_count: 0,
            frame_timer: Instant::now(),
        })
    }

    pub fn prime_world(&mut self) {
        use voxel_render::sample_chunk_surface;

        let center = ChunkCoord::new(0, 0);
        self.world.request_visible_chunks(center);
        while self.world.loaded_chunk(center).is_none() {
            self.world.pump_generation(&mut self.metrics);
        }

        if let Some(surface) = sample_chunk_surface(&self.world, center) {
            self.simulation.player.position = spawn_position_from_surface(surface);
        }

        let camera = self.current_camera();
        self.world.request_visible_chunks(self.player_chunk());
        self.world.pump_generation(&mut self.metrics);
        self.renderer.sync_world(&mut self.world, &camera, &mut self.metrics);
        info!(
            seed = self.config.seed,
            player_position = ?self.simulation.player.position,
            uploaded_meshes = self.renderer.uploaded_meshes(),
            "engine primed"
        );
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.renderer.resize(size);
    }

    pub fn update_and_render(&mut self, input: &mut super::input::InputState) -> Result<(), wgpu::SurfaceError> {
        let now = Instant::now();
        let frame_delta = (now - self.last_frame_at).as_secs_f32().min(0.25);
        self.last_frame_at = now;
        self.accumulator += frame_delta;

        // Metrics calculation
        self.frame_count += 1;
        let elapsed_since_fps = now.duration_since(self.frame_timer);
        if elapsed_since_fps >= Duration::from_secs(1) {
            self.current_fps = self.frame_count as f32 / elapsed_since_fps.as_secs_f32();
            self.frame_count = 0;
            self.frame_timer = now;
        }

        if now.duration_since(self.last_metrics_update) >= Duration::from_millis(500) {
            self.sys.refresh_cpu_all();
            self.sys.refresh_memory();
            self.metrics.cpu_usage = self.sys.global_cpu_usage();
            self.metrics.ram_usage_mb = self.sys.used_memory() / 1024 / 1024;
            self.metrics.gpu_time_ms = self.renderer.retrieve_gpu_time();
            self.last_metrics_update = now;
        }
        self.metrics.fps = self.current_fps;

        let fixed_dt = self.config.simulation.fixed_dt_seconds;
        let mut steps = 0;
        let mut frame_input = input.to_player_input(self.config.render.mouse_sensitivity);

        // Process interactions once per frame (or we could do it in the loop if we want more precision)
        self.handle_interactions(frame_input);

        while self.accumulator >= fixed_dt && steps < MAX_SIMULATION_STEPS_PER_FRAME {
            self.simulation
                .tick(&self.world, frame_input, &self.config.simulation, fixed_dt);
            self.accumulator -= fixed_dt;
            steps += 1;
            frame_input.look_delta = glam::Vec2::ZERO;
            frame_input.jump_pressed = false;
            // Only handle actions on the first step to avoid repeating if lag occurs
            frame_input.action_primary = false;
            frame_input.action_secondary = false;
        }

        // Handle death
        if self.simulation.player.attributes.health <= 0.0 {
            info!("Player died! Respawning...");
            self.simulation.player.respawn(&self.config.simulation);
        }

        self.tick_world();

        if self.accumulator >= fixed_dt {
            tracing::warn!(
                remaining_accumulator = self.accumulator,
                fixed_dt,
                steps,
                "simulation step budget exhausted for frame"
            );
            self.accumulator = 0.0;
        }

        let camera = self.current_camera();
        self.renderer.sync_world(&mut self.world, &camera, &mut self.metrics);

        let attrs = &self.simulation.player.attributes;
        let debug_text = format!(
            "FPS: {:.1}\nCPU: {:.1}%\nGPU: {:.2}ms\nRAM: {}MB\n\nHEALTH: {:.1}/{:.1}\nSTAMINA: {:.1}/{:.1}\nHUNGER: {:.1}/{:.1}\nLEVEL: {} (EXP: {})",
            self.metrics.fps, self.metrics.cpu_usage, self.metrics.gpu_time_ms, self.metrics.ram_usage_mb,
            attrs.health, self.config.simulation.max_health,
            attrs.stamina, self.config.simulation.max_stamina,
            attrs.hunger, self.config.simulation.max_hunger,
            attrs.level, attrs.experience
        );

        self.renderer.render(&camera, Some(&debug_text))
    }

    fn handle_interactions(&mut self, input: PlayerInput) {
        if !input.action_primary && !input.action_secondary {
            return;
        }

        let camera_transform = self.simulation.camera_transform(&self.config.simulation);
        let ray_origin = camera_transform.position;
        let ray_dir = camera_transform.forward;

        if let Some(hit) = voxel_sim::raycast(&self.world, ray_origin, ray_dir, 5.0) {
            if input.action_primary {
                self.world.set_block(hit.coord, voxel_world::AIR);
            } else if input.action_secondary {
                let place_coord = BlockCoord::new(
                    hit.coord.x + hit.normal.x,
                    hit.coord.y + hit.normal.y,
                    hit.coord.z + hit.normal.z,
                );

                // Check if the new block would intersect the player
                let player_pos = self.simulation.player.position;
                let player_collider = self.simulation.player.collider;
                if !intersects_aabb(place_coord, player_pos, player_collider) {
                    // For now, let's just place STONE
                    self.world.set_block(place_coord, voxel_world::STONE);
                }
            }
        }
    }

    fn tick_world(&mut self) {
        let center = self.player_chunk();
        self.world.request_visible_chunks(center);
        self.world.pump_generation(&mut self.metrics);
        self.world.evict_far_chunks(center);
    }

    fn player_chunk(&self) -> ChunkCoord {
        ChunkCoord::from_block(BlockCoord::new(
            self.simulation.player.position.x.floor() as i32,
            self.simulation.player.position.y.floor() as i32,
            self.simulation.player.position.z.floor() as i32,
        ))
    }

    fn current_camera(&self) -> voxel_render::Camera {
        voxel_render::Camera::from_transform(
            self.simulation.camera_transform(&self.config.simulation),
            &self.config.render,
            self.renderer.surface_size(),
            self.config.streaming.keep_radius * voxel_core::CHUNK_SIZE_X,
        )
    }
}

fn spawn_position_from_surface(surface: BlockCoord) -> glam::Vec3 {
    let collider_half_height = PlayerController::default().collider.half_extents.y;
    glam::Vec3::new(
        surface.x as f32 + 0.5,
        surface.y as f32 + 1.0 + collider_half_height,
        surface.z as f32 + 0.5,
    )
}

fn intersects_aabb(block: BlockCoord, pos: glam::Vec3, collider: voxel_sim::Aabb) -> bool {
    let min = pos - collider.half_extents;
    let max = pos + collider.half_extents;

    let block_min = glam::Vec3::new(block.x as f32, block.y as f32, block.z as f32);
    let block_max = block_min + glam::Vec3::ONE;

    min.x < block_max.x && max.x > block_min.x &&
    min.y < block_max.y && max.y > block_min.y &&
    min.z < block_max.z && max.z > block_min.z
}