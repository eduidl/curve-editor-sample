mod app;
mod gpu;
mod renderer;
mod spline;
mod state;
mod ui;

use winit::event_loop::EventLoop;

// ---- Native entry point (called from src/main.rs) ----------------------------

#[cfg(not(target_arch = "wasm32"))]
pub fn run_native() {
    let event_loop = EventLoop::new().expect("failed to create EventLoop");
    event_loop
        .run_app(&mut app::App::default())
        .expect("run_app failed");
}

// ---- Wasm entry point --------------------------------------------------------

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).ok();

    use winit::platform::web::EventLoopExtWebSys;
    let event_loop = EventLoop::new().expect("failed to create EventLoop");
    event_loop.spawn_app(app::App::default());
}
