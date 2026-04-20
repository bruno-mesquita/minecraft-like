mod app;
mod engine;
mod input;

use tracing_subscriber::{fmt, EnvFilter};

fn main() {
    init_tracing();

    let event_loop = match winit::event_loop::EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            tracing::error!(%error, "failed to initialize event loop");
            return;
        }
    };
    let mut app = app::App::new(voxel_core::EngineConfig::default());
    event_loop.run_app(&mut app).expect("app should run");
}

fn init_tracing() {
    let _ = fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
}