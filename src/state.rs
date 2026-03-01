use crate::spline::Spline;

#[derive(Debug, Clone, PartialEq)]
pub enum EditMode {
    Idle,
    Editing {
        spline_index: usize,
        drag: Option<usize>,
        /// Index of the hovered control point.
        hover: Option<usize>,
    },
}

pub struct AppState {
    pub splines: Vec<Spline>,
    pub mode: EditMode,
    /// Current mouse position in NDC.
    pub mouse_ndc: [f32; 2],
    /// Window size in logical pixels.
    pub window_size: [f32; 2],
    /// Target of the right-click context menu: (spline_index, point_index).
    pub context_menu: Option<(usize, usize)>,
    /// One-shot flag: open the context menu popup this frame.
    pub open_context_menu: bool,
}

impl AppState {
    pub fn new(window_size: [f32; 2]) -> Self {
        Self {
            splines: Vec::new(),
            mode: EditMode::Idle,
            mouse_ndc: [0.0, 0.0],
            window_size,
            context_menu: None,
            open_context_menu: false,
        }
    }

    pub fn new_line(&mut self) {
        if self.mode != EditMode::Idle {
            return;
        }
        let name = format!("Line {}", self.splines.len());
        self.splines.push(Spline::new(name));
        let idx = self.splines.len() - 1;
        self.mode = EditMode::Editing {
            spline_index: idx,
            drag: None,
            hover: None,
        };
    }

    pub fn start_edit(&mut self, index: usize) {
        if self.mode == EditMode::Idle && index < self.splines.len() {
            self.mode = EditMode::Editing {
                spline_index: index,
                drag: None,
                hover: None,
            };
        }
    }

    pub fn stop_edit(&mut self) {
        self.mode = EditMode::Idle;
        self.context_menu = None;
        self.open_context_menu = false;
    }

    /// Left mouse button pressed on the canvas.
    pub fn on_canvas_press(&mut self) {
        self.context_menu = None;
        self.open_context_menu = false;

        let mouse = self.mouse_ndc;
        let hit_radius = self.hit_radius_ndc();

        let edit_idx = match self.mode {
            EditMode::Editing { spline_index, .. } => spline_index,
            EditMode::Idle => return,
        };

        let hit = hit_test(&self.splines[edit_idx], mouse, hit_radius);
        if let Some(i) = hit {
            if let EditMode::Editing { ref mut drag, ref mut hover, .. } = self.mode {
                *drag = Some(i);
                *hover = None;
            }
        } else {
            self.splines[edit_idx].push_point(mouse);
        }
    }

    /// Mouse moved (called every frame).
    pub fn on_mouse_move(&mut self, ndc: [f32; 2]) {
        self.mouse_ndc = ndc;

        let (edit_idx, is_dragging) = match self.mode {
            EditMode::Editing { spline_index, drag, .. } => (spline_index, drag),
            EditMode::Idle => return,
        };

        if let Some(point_idx) = is_dragging {
            self.splines[edit_idx].move_point(point_idx, ndc);
        } else {
            let radius = self.hit_radius_ndc();
            let h = hit_test(&self.splines[edit_idx], ndc, radius);
            if let EditMode::Editing { ref mut hover, .. } = self.mode {
                *hover = h;
            }
        }
    }

    /// Left mouse button released.
    pub fn on_canvas_release(&mut self) {
        if let EditMode::Editing { ref mut drag, .. } = self.mode {
            *drag = None;
        }
    }

    /// Right mouse button pressed on the canvas.
    pub fn on_canvas_right_click(&mut self) {
        self.context_menu = None;
        self.open_context_menu = false;

        let edit_idx = match self.mode {
            EditMode::Editing { spline_index, .. } => spline_index,
            EditMode::Idle => return,
        };

        let radius = self.hit_radius_ndc();
        let mouse = self.mouse_ndc;
        if let Some(i) = hit_test(&self.splines[edit_idx], mouse, radius) {
            self.context_menu = Some((edit_idx, i));
            self.open_context_menu = true;
        }
    }

    /// Delete the specified control point.
    pub fn delete_point(&mut self, spline_index: usize, point_index: usize) {
        if spline_index >= self.splines.len() {
            return;
        }
        let spline = &mut self.splines[spline_index];
        if point_index >= spline.control_points.len() {
            return;
        }
        spline.control_points.remove(point_index);
        spline.dirty = true;

        if let EditMode::Editing { ref mut drag, ref mut hover, .. } = self.mode {
            *drag = None;
            *hover = None;
        }
        self.context_menu = None;
        self.open_context_menu = false;
    }

    pub fn resize(&mut self, new_size: [f32; 2]) {
        self.window_size = new_size;
    }

    pub fn hit_radius_ndc(&self) -> f32 {
        12.0 / self.window_size[0] * 2.0
    }
}

fn hit_test(spline: &Spline, mouse: [f32; 2], radius: f32) -> Option<usize> {
    let r2 = radius * radius;
    spline.control_points.iter().enumerate().find_map(|(i, &p)| {
        let dx = p[0] - mouse[0];
        let dy = p[1] - mouse[1];
        if dx * dx + dy * dy <= r2 { Some(i) } else { None }
    })
}

pub fn pixel_to_ndc(pixel: [f32; 2], window_size: [f32; 2]) -> [f32; 2] {
    [
        pixel[0] / window_size[0] * 2.0 - 1.0,
        1.0 - pixel[1] / window_size[1] * 2.0,
    ]
}
