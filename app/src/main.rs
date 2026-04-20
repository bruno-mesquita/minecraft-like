use glam::{Vec2, Vec3};
use pollster::block_on;
use std::{sync::Arc, time::Instant};
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};
use voxel_core::{BlockCoord, ChunkCoord, EngineConfig, FrameMetrics, SimulationConfig};
use voxel_render::{sample_chunk_surface, Camera, Renderer};
use voxel_sim::{PlayerInput, Simulation};
use voxel_world::World;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{DeviceEvent, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowAttributes, WindowId},
};

fn main() {
    init_tracing();

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            error!(%error, "failed to initialize event loop");
            return;
        }
    };
    let mut app = App::new(EngineConfig::default());
    event_loop.run_app(&mut app).expect("app should run");
}

struct App {
    config: EngineConfig,
    window: Option<Arc<Window>>,
    engine: Option<Engine>,
    input: InputState,
}

impl App {
    fn new(config: EngineConfig) -> Self {
        Self {
            config,
            window: None,
            engine: None,
            input: InputState::default(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = WindowAttributes::default()
            .with_title("Minecraft Rust Prototype")
            .with_inner_size(LogicalSize::new(
                self.config.render.window_width,
                self.config.render.window_height,
            ));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("window should be created"),
        );

        match block_on(Engine::new(window.clone(), self.config.clone())) {
            Ok(mut engine) => {
                capture_cursor(&window, true);
                self.input.cursor_captured = true;
                engine.prime_world();
                self.window = Some(window);
                self.engine = Some(engine);
            }
            Err(error) => {
                error!(%error, "failed to initialize engine");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Some(engine) = self.engine.as_mut() {
                    if let Err(error) = engine.update_and_render(&mut self.input) {
                        match error {
                            wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                                engine.resize(window.inner_size());
                            }
                            wgpu::SurfaceError::OutOfMemory => {
                                error!("surface out of memory");
                                event_loop.exit();
                            }
                            wgpu::SurfaceError::Timeout => {
                                warn!("surface timeout");
                            }
                            wgpu::SurfaceError::Other => {
                                warn!("surface returned an unspecified error");
                            }
                        }
                    }
                }
                self.input.end_frame();
            }
            WindowEvent::Resized(size) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.resize(size);
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } => {
                if !self.input.cursor_captured {
                    capture_cursor(window, true);
                    self.input.cursor_captured = true;
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.input.handle_key(code, event.state, event.repeat);
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        self.input.cursor_captured = false;
                        capture_cursor(window, false);
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: winit::event::DeviceId, event: DeviceEvent) {
        if !self.input.cursor_captured {
            return;
        }

        if let DeviceEvent::MouseMotion { delta } = event {
            self.input.mouse_delta += Vec2::new(delta.0 as f32, delta.1 as f32);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

struct Engine {
    config: EngineConfig,
    renderer: Renderer,
    world: World,
    simulation: Simulation,
    metrics: FrameMetrics,
    last_frame_at: Instant,
    accumulator: f32,
}

impl Engine {
    async fn new(window: Arc<Window>, config: EngineConfig) -> Result<Self, String> {
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

    fn prime_world(&mut self) {
        let center = ChunkCoord::new(0, 0);
        self.world.request_visible_chunks(center);
        while self.world.loaded_chunk(center).is_none() {
            self.world.pump_generation(&mut self.metrics);
        }

        if let Some(surface) = sample_chunk_surface(&self.world, center) {
            self.simulation.player.position = spawn_position_from_surface(surface, &self.config.simulation);
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

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.renderer.resize(size);
    }

    fn update_and_render(&mut self, input: &mut InputState) -> Result<(), wgpu::SurfaceError> {
        let now = Instant::now();
        let frame_delta = (now - self.last_frame_at).as_secs_f32().min(0.25);
        self.last_frame_at = now;
        self.accumulator += frame_delta;

        let fixed_dt = self.config.simulation.fixed_dt_seconds;
        let mut steps = 0;
        let mut frame_input = input.to_player_input(self.config.render.mouse_sensitivity);

        while self.accumulator >= fixed_dt && steps < 4 {
            self.simulation
                .tick(&self.world, frame_input, &self.config.simulation, fixed_dt);
            self.tick_world();
            self.accumulator -= fixed_dt;
            steps += 1;
            frame_input.look_delta = Vec2::ZERO;
            frame_input.jump_pressed = false;
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

    fn current_camera(&self) -> Camera {
        Camera::from_transform(
            self.simulation.camera_transform(&self.config.simulation),
            &self.config.render,
            self.renderer.surface_size(),
            self.config.streaming.keep_radius * voxel_core::CHUNK_SIZE_X,
        )
    }
}

#[derive(Default)]
struct InputState {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    sprint: bool,
    jump_held: bool,
    jump_pressed: bool,
    mouse_delta: Vec2,
    cursor_captured: bool,
}

impl InputState {
    fn handle_key(&mut self, code: KeyCode, state: ElementState, repeat: bool) {
        let pressed = state == ElementState::Pressed;

        match code {
            KeyCode::KeyW => self.forward = pressed,
            KeyCode::KeyS => self.back = pressed,
            KeyCode::KeyA => self.left = pressed,
            KeyCode::KeyD => self.right = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.sprint = pressed,
            KeyCode::Space => {
                if pressed && !repeat && !self.jump_held {
                    self.jump_pressed = true;
                }
                self.jump_held = pressed;
            }
            _ => {}
        }
    }

    fn to_player_input(&self, mouse_sensitivity: f32) -> PlayerInput {
        PlayerInput {
            move_forward: axis(self.forward, self.back),
            move_right: axis(self.right, self.left),
            look_delta: self.mouse_delta * mouse_sensitivity,
            jump_pressed: self.jump_pressed,
            sprint_held: self.sprint,
        }
    }

    fn end_frame(&mut self) {
        self.mouse_delta = Vec2::ZERO;
        self.jump_pressed = false;
    }
}

fn axis(positive: bool, negative: bool) -> f32 {
    match (positive, negative) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    }
}

fn spawn_position_from_surface(surface: BlockCoord, simulation: &SimulationConfig) -> Vec3 {
    Vec3::new(
        surface.x as f32 + 0.5,
        surface.y as f32 + 1.0 + (0.9 - simulation.eye_height),
        surface.z as f32 + 0.5,
    )
}

fn capture_cursor(window: &Window, capture: bool) {
    if capture {
        let _ = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        window.set_cursor_visible(false);
    } else {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
    }
}

fn init_tracing() {
    let _ = fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
}
