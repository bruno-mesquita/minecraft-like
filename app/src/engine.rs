use std::{sync::Arc, time::Instant};
use tracing::info;
use voxel_core::{BlockCoord, ChunkCoord, EngineConfig, FrameMetrics, RenderConfig, SimulationConfig};
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

        Ok(Self {
            config,
            renderer,
            world,
            simulation: Simulation::new(),
            metrics: FrameMetrics::default(),
            last_frame_at: Instant::now(),
            accumulator: 0.0,
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

        let fixed_dt = self.config.simulation.fixed_dt_seconds;
        let mut steps = 0;
        let mut frame_input = input.to_player_input(self.config.render.mouse_sensitivity);

        while self.accumulator >= fixed_dt && steps < MAX_SIMULATION_STEPS_PER_FRAME {
            self.simulation
                .tick(&self.world, frame_input, &self.config.simulation, fixed_dt);
            self.tick_world();
            self.accumulator -= fixed_dt;
            steps += 1;
            frame_input.look_delta = glam::Vec2::ZERO;
            frame_input.jump_pressed = false;
        }

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
        self.renderer.render(&camera)
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