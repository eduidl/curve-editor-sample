use crate::state::{AppState, EditMode};
use wgpu::util::DeviceExt;

// ---- Vertex format -----------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

// ---- WGSL shader -------------------------------------------------------------

const SHADER_SRC: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

// ---- Renderer ----------------------------------------------------------------

pub struct SplineRenderer {
    curve_pipeline: wgpu::RenderPipeline,
    point_pipeline: wgpu::RenderPipeline,
}

impl SplineRenderer {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("spline_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("spline_layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let color_target = wgpu::ColorTargetState {
            format: surface_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };

        let vertex_state = wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::desc()],
            compilation_options: Default::default(),
        };

        let fragment_state = wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(color_target.clone())],
            compilation_options: Default::default(),
        };

        // Curve pipeline (LineStrip)
        let curve_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("curve_pipeline"),
            layout: Some(&layout),
            vertex: vertex_state.clone(),
            fragment: Some(fragment_state.clone()),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: sample_count, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
            cache: None,
        });

        // Control point pipeline (TriangleList)
        let point_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("point_pipeline"),
            layout: Some(&layout),
            vertex: vertex_state,
            fragment: Some(fragment_state),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: sample_count, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
            cache: None,
        });

        Self {
            curve_pipeline,
            point_pipeline,
        }
    }

    pub fn render(
        &self,
        rpass: &mut wgpu::RenderPass,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        state: &mut AppState,
    ) {
        let editing_index = match &state.mode {
            EditMode::Editing { spline_index, .. } => Some(*spline_index),
            EditMode::Idle => None,
        };
        let drag_index = match &state.mode {
            EditMode::Editing { drag, .. } => *drag,
            EditMode::Idle => None,
        };
        let hover_index = match &state.mode {
            EditMode::Editing { hover, .. } => *hover,
            EditMode::Idle => None,
        };
        let window_size = state.window_size;

        // ---- Draw curves -----------------------------------------------------
        rpass.set_pipeline(&self.curve_pipeline);

        for (i, spline) in state.splines.iter_mut().enumerate() {
            let verts = spline.curve_vertices();
            if verts.len() < 2 {
                continue;
            }
            let is_editing = editing_index == Some(i);
            let color: [f32; 4] = if is_editing {
                [0.2, 0.8, 1.0, 1.0] // cyan
            } else {
                [0.5, 0.5, 0.5, 1.0] // grey
            };

            let vertices: Vec<Vertex> = verts
                .iter()
                .map(|&p| Vertex { position: p, color })
                .collect();

            let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let _ = queue; // suppress unused warning; queue used outside this fn for imgui
            rpass.set_vertex_buffer(0, buf.slice(..));
            rpass.draw(0..vertices.len() as u32, 0..1);
        }

        // ---- Draw control points (editing spline only) -----------------------
        if let Some(edit_idx) = editing_index {
            rpass.set_pipeline(&self.point_pipeline);

            let spline = &state.splines[edit_idx];
            let mut point_verts: Vec<Vertex> = Vec::new();

            for (i, &cp) in spline.control_points.iter().enumerate() {
                let is_drag = drag_index == Some(i);
                let is_hover = hover_index == Some(i);
                let (color, half_px): ([f32; 4], f32) = if is_drag {
                    ([1.0, 0.85, 0.0, 1.0], 7.0) // yellow, large (dragging)
                } else if is_hover {
                    ([1.0, 0.5, 0.1, 1.0], 7.0)  // orange, large (hovered)
                } else {
                    ([0.9, 0.9, 0.9, 1.0], 5.0)  // off-white, normal
                };
                let hx = half_px / window_size[0] * 2.0;
                let hy = half_px / window_size[1] * 2.0;
                let [cx, cy] = cp;
                let tl = Vertex { position: [cx - hx, cy + hy], color };
                let tr = Vertex { position: [cx + hx, cy + hy], color };
                let bl = Vertex { position: [cx - hx, cy - hy], color };
                let br = Vertex { position: [cx + hx, cy - hy], color };
                point_verts.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
            }

            if !point_verts.is_empty() {
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&point_verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                rpass.set_vertex_buffer(0, buf.slice(..));
                rpass.draw(0..point_verts.len() as u32, 0..1);
            }
        }
    }
}
