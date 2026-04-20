use std::sync::Arc;
use voxel_core::EngineConfig;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{DeviceEvent, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::{Window, WindowId},
};

pub struct App {
    config: EngineConfig,
    window: Option<Arc<Window>>,
    engine: Option<super::engine::Engine>,
    input: super::input::InputState,
}

impl App {
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            window: None,
            engine: None,
            input: super::input::InputState::default(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        use winit::dpi::LogicalSize;
        use winit::window::WindowAttributes;

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

        use pollster::block_on;
        match block_on(super::engine::Engine::new(window.clone(), self.config.clone())) {
            Ok(mut engine) => {
                self.input.cursor_captured = super::input::capture_cursor(&window, true);
                engine.prime_world();
                self.window = Some(window);
                self.engine = Some(engine);
            }
            Err(error) => {
                tracing::error!(%error, "failed to initialize engine");
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
                                tracing::error!("surface out of memory");
                                event_loop.exit();
                            }
                            wgpu::SurfaceError::Timeout => {
                                tracing::warn!("surface timeout");
                            }
                            wgpu::SurfaceError::Other => {
                                tracing::warn!("surface returned an unspecified error");
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
            WindowEvent::CursorEntered { .. } => {
                if !self.input.cursor_captured {
                    self.input.cursor_captured = super::input::capture_cursor(window, true);
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } => {
                if !self.input.cursor_captured {
                    self.input.cursor_captured = super::input::capture_cursor(window, true);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.handle_cursor_moved(window, position);
            }
            WindowEvent::Focused(false) => {
                self.input.clear_focus_state();
                if super::input::capture_cursor(window, false) {
                    tracing::debug!("released cursor after losing focus");
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                    tracing::debug!(?code, state = ?event.state, repeat = event.repeat, "keyboard input");
                    self.input.handle_key(code, event.state, event.repeat);
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        self.input.clear_focus_state();
                        if super::input::capture_cursor(window, false) {
                            tracing::debug!("released cursor from escape");
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
            use glam::Vec2;
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