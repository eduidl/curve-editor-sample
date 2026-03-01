use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    dpi::LogicalPosition,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

use crate::{
    gpu::GpuContext,
    renderer::SplineRenderer,
    state::{AppState, pixel_to_ndc},
    ui::build_ui,
};

// ---- Initialized graphics bundle ---------------------------------------------

struct AppGraphics {
    window: Arc<Window>,
    gpu: GpuContext,
    renderer: SplineRenderer,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    state: AppState,
}

// ---- App struct --------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
use std::{cell::RefCell, rc::Rc};

pub struct App {
    graphics: Option<AppGraphics>,
    #[cfg(target_arch = "wasm32")]
    pending: Rc<RefCell<Option<AppGraphics>>>,
}

#[allow(clippy::derivable_impls)]
impl Default for App {
    fn default() -> Self {
        Self {
            graphics: None,
            #[cfg(target_arch = "wasm32")]
            pending: Rc::new(RefCell::new(None)),
        }
    }
}

// ---- Helper: drain pending graphics into self.graphics ----------------------

impl App {
    fn poll_pending(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if self.graphics.is_none() {
            if let Ok(mut lock) = self.pending.try_borrow_mut() {
                if lock.is_some() {
                    self.graphics = lock.take();
                }
            }
        }
    }
}

// ---- Helper: build AppGraphics once GPU is ready ----------------------------

fn build_app_graphics(window: Arc<Window>, gpu: GpuContext) -> AppGraphics {
    let egui_ctx = egui::Context::default();

    let egui_state = egui_winit::State::new(
        egui_ctx.clone(),
        egui_ctx.viewport_id(),
        &*window,
        Some(window.scale_factor() as f32),
        None,
        Some(gpu.device.limits().max_texture_dimension_2d as usize),
    );

    let egui_renderer = egui_wgpu::Renderer::new(
        &gpu.device,
        gpu.surface_format,
        egui_wgpu::RendererOptions {
            msaa_samples: crate::gpu::MSAA_SAMPLES,
            depth_stencil_format: None,
            dithering: true,
            ..Default::default()
        },
    );

    let renderer = SplineRenderer::new(&gpu.device, gpu.surface_format, crate::gpu::MSAA_SAMPLES);

    let inner = window.inner_size();
    let state = AppState::new([inner.width as f32, inner.height as f32]);

    AppGraphics {
        window,
        gpu,
        renderer,
        egui_ctx,
        egui_state,
        egui_renderer,
        state,
    }
}

// ---- ApplicationHandler ------------------------------------------------------

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.graphics.is_some() {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        if self.pending.borrow().is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title("Catmull-Rom Editor")
            .with_inner_size(winit::dpi::LogicalSize::new(1024u32, 768u32));

        #[cfg(target_arch = "wasm32")]
        let window_attrs = {
            use winit::platform::web::WindowAttributesExtWebSys;
            // Use the browser viewport size as the initial canvas size.
            let (w, h) = web_sys::window()
                .map(|win| {
                    let w = win
                        .inner_width()
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(1024.0);
                    let h = win
                        .inner_height()
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(768.0);
                    (w as u32, h as u32)
                })
                .unwrap_or((1024, 768));
            window_attrs
                .with_inner_size(winit::dpi::LogicalSize::new(w, h))
                .with_append(true)
        };

        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("failed to create window"),
        );

        #[cfg(not(target_arch = "wasm32"))]
        {
            let gpu = pollster::block_on(GpuContext::new(window.clone()));
            self.graphics = Some(build_app_graphics(window, gpu));
        }

        #[cfg(target_arch = "wasm32")]
        {
            let pending = self.pending.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let gpu = GpuContext::new(window.clone()).await;
                *pending.borrow_mut() = Some(build_app_graphics(window, gpu));
            });
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        self.poll_pending();

        let Some(g) = self.graphics.as_mut() else {
            return;
        };

        // Forward event to egui.
        let event_response = g.egui_state.on_window_event(&g.window, &event);

        let want_mouse = g.egui_ctx.wants_pointer_input();

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                g.gpu.resize(size);
                g.state.resize([size.width as f32, size.height as f32]);
            }

            WindowEvent::ScaleFactorChanged { .. } => {
                let inner = g.window.inner_size();
                g.state.resize([inner.width as f32, inner.height as f32]);
            }

            WindowEvent::CursorMoved { position, .. } => {
                // Always update for smooth drags even when egui captures mouse.
                let scale = g.window.scale_factor();
                let logical: LogicalPosition<f32> = position.to_logical(scale);
                let ndc = pixel_to_ndc([logical.x, logical.y], g.state.window_size);
                g.state.on_mouse_move(ndc);
            }

            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) if !want_mouse => {
                    g.state.on_canvas_press();
                }
                (MouseButton::Left, ElementState::Released) => {
                    g.state.on_canvas_release();
                }
                (MouseButton::Right, ElementState::Pressed) if !want_mouse => {
                    g.state.on_canvas_right_click();
                }
                _ => {}
            },

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if key_event.state == ElementState::Pressed
                    && !g.egui_ctx.wants_keyboard_input()
                    && let Key::Named(NamedKey::Escape) = key_event.logical_key
                {
                    g.state.stop_edit();
                }
            }

            WindowEvent::RedrawRequested => {
                render_frame(g);
            }

            _ => {}
        }

        // Suppress unused variable warning for event_response.
        let _ = event_response;
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.poll_pending();

        if let Some(g) = self.graphics.as_mut() {
            g.window.request_redraw();
        }
    }
}

// ---- Frame rendering ---------------------------------------------------------

fn render_frame(g: &mut AppGraphics) {
    // --- egui input ---
    let raw_input = g.egui_state.take_egui_input(&g.window);

    // --- run egui UI ---
    let full_output = g.egui_ctx.run(raw_input, |ctx| {
        build_ui(ctx, &mut g.state);
    });

    // --- handle platform output (cursor, clipboard, etc.) ---
    g.egui_state
        .handle_platform_output(&g.window, full_output.platform_output);

    // --- tessellate ---
    let tris = g
        .egui_ctx
        .tessellate(full_output.shapes, full_output.pixels_per_point);

    // --- update egui textures ---
    for (id, delta) in &full_output.textures_delta.set {
        g.egui_renderer
            .update_texture(&g.gpu.device, &g.gpu.queue, *id, delta);
    }

    // --- wgpu frame ---
    let output = match g.gpu.surface.get_current_texture() {
        Ok(t) => t,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            g.gpu
                .surface
                .configure(&g.gpu.device, &g.gpu.surface_config);
            return;
        }
        Err(e) => {
            log::error!("Surface error: {:?}", e);
            return;
        }
    };
    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = g
        .gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("main_encoder"),
        });

    let screen_desc = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [g.gpu.surface_config.width, g.gpu.surface_config.height],
        pixels_per_point: full_output.pixels_per_point,
    };

    g.egui_renderer.update_buffers(
        &g.gpu.device,
        &g.gpu.queue,
        &mut encoder,
        &tris,
        &screen_desc,
    );

    {
        // When MSAA is active: render into the MSAA texture, resolve to the surface.
        // When MSAA is disabled (wasm): render directly to the surface texture.
        let (color_view, resolve_target, store_op) = match &g.gpu.msaa_view {
            Some(msaa) => (msaa, Some(&view), wgpu::StoreOp::Discard),
            None => (&view, None, wgpu::StoreOp::Store),
        };

        // forget_lifetime() is required by egui_wgpu::Renderer::render which
        // takes RenderPass<'static>. The pass is dropped before encoder.finish().
        let mut rpass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.10,
                            a: 1.0,
                        }),
                        store: store_op,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            })
            .forget_lifetime();

        g.renderer
            .render(&mut rpass, &g.gpu.device, &g.gpu.queue, &mut g.state);

        g.egui_renderer.render(&mut rpass, &tris, &screen_desc);
    }

    g.gpu.queue.submit(std::iter::once(encoder.finish()));
    output.present();

    // Free unused egui textures.
    for id in &full_output.textures_delta.free {
        g.egui_renderer.free_texture(id);
    }
}
