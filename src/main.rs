#[cfg(not(target_arch = "wasm32"))]
fn main() {
    env_logger::init();
    curve_editor_sample::run_native();
}

// Dummy main for wasm target (actual entry point is #[wasm_bindgen(start)] in lib.rs)
#[cfg(target_arch = "wasm32")]
fn main() {}
