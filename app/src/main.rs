use glam::{Vec2, Vec3};
use pollster::block_on;
use std::{sync::Arc, time::Instant};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};
use voxel_core::{BlockCoord, ChunkCoord, EngineConfig, FrameMetrics, SimulationConfig};
use voxel_render::{sample_chunk_surface, Camera, Renderer};
use voxel_sim::{PlayerController, PlayerInput, Simulation};
use voxel_world::World;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowAttributes, WindowId},
};

const MAX_SIMULATION_STEPS_PER_FRAME: usize = 32;

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
                self.input.cursor_captured = capture_cursor(&window, true);
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
                    self.input.cursor_captured = capture_cursor(window, true);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.handle_cursor_moved(position);
            }
            WindowEvent::Focused(false) => {
                self.input.clear_focus_state();
                if capture_cursor(window, false) {
                    debug!("released cursor after losing focus");
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    debug!(?code, state = ?event.state, repeat = event.repeat, "keyboard input");
                    self.input.handle_key(code, event.state, event.repeat);
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        self.input.clear_focus_state();
                        if capture_cursor(window, false) {
                            debug!("released cursor from escape");
                        }
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
            self.input.received_raw_mouse = true;
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

        while self.accumulator >= fixed_dt && steps < MAX_SIMULATION_STEPS_PER_FRAME {
            self.simulation
                .tick(&self.world, frame_input, &self.config.simulation, fixed_dt);
            self.tick_world();
            self.accumulator -= fixed_dt;
            steps += 1;
            frame_input.look_delta = Vec2::ZERO;
            frame_input.jump_pressed = false;
        }

        if self.accumulator >= fixed_dt {
            warn!(
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
    last_cursor_position: Option<PhysicalPosition<f64>>,
    received_raw_mouse: bool,
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
        self.received_raw_mouse = false;
    }

    fn clear_focus_state(&mut self) {
        self.forward = false;
        self.back = false;
        self.left = false;
        self.right = false;
        self.sprint = false;
        self.jump_held = false;
        self.jump_pressed = false;
        self.mouse_delta = Vec2::ZERO;
        self.cursor_captured = false;
        self.last_cursor_position = None;
        self.received_raw_mouse = false;
    }

    fn handle_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        let previous = self.last_cursor_position.replace(position);
        if !self.cursor_captured || self.received_raw_mouse {
            return;
        }

        let Some(previous) = previous else {
            return;
        };

        self.mouse_delta += Vec2::new((position.x - previous.x) as f32, (position.y - previous.y) as f32);
    }
}

fn axis(positive: bool, negative: bool) -> f32 {
    match (positive, negative) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    }
}

fn spawn_position_from_surface(surface: BlockCoord) -> Vec3 {
    let collider_half_height = PlayerController::default().collider.half_extents.y;
    Vec3::new(
        surface.x as f32 + 0.5,
        surface.y as f32 + 1.0 + collider_half_height,
        surface.z as f32 + 0.5,
    )
}

fn capture_cursor(window: &Window, capture: bool) -> bool {
    if capture {
        let grab_mode = window
            .set_cursor_grab(CursorGrabMode::Confined)
            .map(|_| CursorGrabMode::Confined)
            .or_else(|confined_error| {
                debug!(%confined_error, "confined cursor grab failed, trying locked");
                window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .map(|_| CursorGrabMode::Locked)
            });

        match grab_mode {
            Ok(mode) => {
                window.set_cursor_visible(false);
                let size = window.inner_size();
                let center = LogicalPosition::new(
                    f64::from(size.width.max(1)) / 2.0,
                    f64::from(size.height.max(1)) / 2.0,
                );
                if let Err(error) = window.set_cursor_position(center) {
                    debug!(%error, "failed to place cursor at window center after capture");
                }
                debug!(?mode, "captured cursor");
                true
            }
            Err(error) => {
                warn!(%error, "failed to capture cursor");
                window.set_cursor_visible(true);
                false
            }
        }
    } else {
        if let Err(error) = window.set_cursor_grab(CursorGrabMode::None) {
            warn!(%error, "failed to release cursor");
            return false;
        }
        window.set_cursor_visible(true);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_state_maps_keyboard_axes() {
        let mut input = InputState::default();
        input.handle_key(KeyCode::KeyW, ElementState::Pressed, false);
        input.handle_key(KeyCode::KeyD, ElementState::Pressed, false);
        input.handle_key(KeyCode::ShiftLeft, ElementState::Pressed, false);
        input.handle_key(KeyCode::Space, ElementState::Pressed, false);

        let player_input = input.to_player_input(1.0);
        assert_eq!(player_input.move_forward, 1.0);
        assert_eq!(player_input.move_right, 1.0);
        assert!(player_input.sprint_held);
        assert!(player_input.jump_pressed);
    }

    #[test]
    fn clear_focus_state_resets_pressed_input() {
        let mut input = InputState {
            forward: true,
            back: true,
            left: true,
            right: true,
            sprint: true,
            jump_held: true,
            jump_pressed: true,
            mouse_delta: Vec2::new(3.0, -2.0),
            cursor_captured: true,
            last_cursor_position: Some(PhysicalPosition::new(10.0, 12.0)),
            received_raw_mouse: true,
        };

        input.clear_focus_state();

        let player_input = input.to_player_input(1.0);
        assert_eq!(player_input.move_forward, 0.0);
        assert_eq!(player_input.move_right, 0.0);
        assert_eq!(player_input.look_delta, Vec2::ZERO);
        assert!(!player_input.jump_pressed);
        assert!(!player_input.sprint_held);
        assert!(!input.cursor_captured);
        assert!(input.last_cursor_position.is_none());
        assert!(!input.received_raw_mouse);
    }

    #[test]
    fn cursor_move_fallback_ignores_first_sample_after_capture() {
        let mut input = InputState {
            cursor_captured: true,
            ..InputState::default()
        };

        input.handle_cursor_moved(PhysicalPosition::new(100.0, 100.0));
        assert_eq!(input.mouse_delta, Vec2::ZERO);

        input.handle_cursor_moved(PhysicalPosition::new(112.0, 94.0));
        assert_eq!(input.mouse_delta, Vec2::new(12.0, -6.0));

        input.clear_focus_state();
        input.cursor_captured = true;
        input.handle_cursor_moved(PhysicalPosition::new(90.0, 90.0));
        assert_eq!(input.mouse_delta, Vec2::ZERO);
    }

    #[test]
    fn spawn_position_places_collider_above_surface() {
        let simulation = SimulationConfig::default();
        let surface = BlockCoord::new(0, 64, 0);
        let position = spawn_position_from_surface(surface);
        let collider = PlayerController::default().collider;

        assert_eq!(position.y - collider.half_extents.y, 65.0);
        assert_eq!(position.y + simulation.eye_height, 66.5);
    }
}

fn init_tracing() {
    let _ = fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
}
