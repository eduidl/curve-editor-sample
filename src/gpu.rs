use std::sync::Arc;
use winit::window::Window;

// MSAA: 4x on native, disabled on wasm (WebGL2 has constraints).
pub const MSAA_SAMPLES: u32 = if cfg!(target_arch = "wasm32") { 1 } else { 4 };

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,
    /// Some when MSAA_SAMPLES > 1, None otherwise.
    pub msaa_view: Option<wgpu::TextureView>,
}

impl GpuContext {
    pub async fn new(window: Arc<Window>) -> Self {
        let backends = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                wgpu::Backends::PRIMARY
            }
            #[cfg(target_arch = "wasm32")]
            {
                // Use WebGL2 only. Attempting BROWSER_WEBGPU first taints the
                // canvas and prevents WebGL2 from working as a fallback.
                wgpu::Backends::GL
            }
        };

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("failed to find GPU adapter");

        let limits = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                wgpu::Limits::default()
            }
            #[cfg(target_arch = "wasm32")]
            {
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
            }
        };

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: limits,
                ..Default::default()
            })
            .await
            .expect("failed to create device/queue");

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let msaa_view = Self::create_msaa_view(&device, &surface_config);

        Self {
            device,
            queue,
            surface,
            surface_config,
            surface_format,
            msaa_view,
        }
    }

    fn create_msaa_view(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> Option<wgpu::TextureView> {
        if MSAA_SAMPLES <= 1 {
            return None;
        }
        Some(
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("msaa_texture"),
                    size: wgpu::Extent3d {
                        width: config.width,
                        height: config.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: MSAA_SAMPLES,
                    dimension: wgpu::TextureDimension::D2,
                    format: config.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default()),
        )
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.surface_config.width = new_size.width;
        self.surface_config.height = new_size.height;
        self.surface.configure(&self.device, &self.surface_config);
        self.msaa_view = Self::create_msaa_view(&self.device, &self.surface_config);
    }
}
