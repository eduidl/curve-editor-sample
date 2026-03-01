const SEGMENTS_PER_SPAN: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveType {
    CatmullRom,
    CatmullRomCentripetal,
    BSplineInterp,
}

#[derive(Debug, Clone)]
pub struct Spline {
    pub control_points: Vec<[f32; 2]>, // NDC coordinates
    pub name: String,
    pub curve_type: CurveType,
    cached_curve: Vec<[f32; 2]>,
    pub dirty: bool,
}

impl Spline {
    pub fn new(name: String) -> Self {
        Self {
            control_points: Vec::new(),
            name,
            curve_type: CurveType::CatmullRom,
            cached_curve: Vec::new(),
            dirty: true,
        }
    }

    pub fn push_point(&mut self, p: [f32; 2]) {
        self.control_points.push(p);
        self.dirty = true;
    }

    pub fn move_point(&mut self, index: usize, pos: [f32; 2]) {
        if index < self.control_points.len() {
            self.control_points[index] = pos;
            self.dirty = true;
        }
    }

    pub fn curve_vertices(&mut self) -> &[[f32; 2]] {
        if self.dirty {
            self.tessellate();
        }
        &self.cached_curve
    }

    fn tessellate(&mut self) {
        self.cached_curve.clear();
        match self.curve_type {
            CurveType::CatmullRom => self.tessellate_catmull_rom(),
            CurveType::CatmullRomCentripetal => self.tessellate_catmull_rom_centripetal(),
            CurveType::BSplineInterp => self.tessellate_bspline_interp(),
        }
        self.dirty = false;
    }

    fn tessellate_catmull_rom(&mut self) {
        let pts = &self.control_points;
        let n = pts.len();
        if n < 2 {
            return;
        }

        // Duplicate endpoints as phantom points so the tangent at each end
        // equals (P1 - P0) / 2, converging gently at the endpoints.
        let mut ext: Vec<[f32; 2]> = Vec::with_capacity(n + 2);
        ext.push(pts[0]); // duplicate first
        ext.extend_from_slice(pts);
        ext.push(pts[n - 1]); // duplicate last

        // Each span i uses ext[i..i+4] as the four control points.
        for i in 0..n - 1 {
            let p0 = ext[i];
            let p1 = ext[i + 1];
            let p2 = ext[i + 2];
            let p3 = ext[i + 3];

            let start_s = if i == 0 { 0 } else { 1 };
            for s in start_s..=SEGMENTS_PER_SPAN {
                let t = s as f32 / SEGMENTS_PER_SPAN as f32;
                let t2 = t * t;
                let t3 = t2 * t;
                let interp = |a: f32, b: f32, c: f32, d: f32| -> f32 {
                    0.5 * ((2.0 * b)
                        + (-a + c) * t
                        + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                        + (-a + 3.0 * b - 3.0 * c + d) * t3)
                };
                self.cached_curve.push([
                    interp(p0[0], p1[0], p2[0], p3[0]),
                    interp(p0[1], p1[1], p2[1], p3[1]),
                ]);
            }
        }
    }

    /// Centripetal Catmull-Rom via the Barry-Goldman algorithm.
    /// Knot interval = dist^0.5 between consecutive points.
    /// Guaranteed no cusps or self-intersections.
    fn tessellate_catmull_rom_centripetal(&mut self) {
        let pts = &self.control_points;
        let n = pts.len();
        if n < 2 {
            return;
        }

        let mut ext: Vec<[f32; 2]> = Vec::with_capacity(n + 2);
        ext.push(pts[0]);
        ext.extend_from_slice(pts);
        ext.push(pts[n - 1]);

        // Centripetal knot interval: dist^0.5 = (dx^2 + dy^2)^0.25
        let knot_interval = |pa: [f32; 2], pb: [f32; 2]| -> f32 {
            let dx = pb[0] - pa[0];
            let dy = pb[1] - pa[1];
            (dx * dx + dy * dy).sqrt().sqrt()
        };

        for i in 0..n - 1 {
            let p0 = ext[i];
            let p1 = ext[i + 1];
            let p2 = ext[i + 2];
            let p3 = ext[i + 3];

            let t0 = 0.0f32;
            let t1 = t0 + knot_interval(p0, p1);
            let t2 = t1 + knot_interval(p1, p2);
            let t3 = t2 + knot_interval(p2, p3);

            // Linear interpolation on the knot axis with zero-denominator guard.
            let lerp = |ta: f32, tb: f32, pa: [f32; 2], pb: [f32; 2], t: f32| -> [f32; 2] {
                let d = tb - ta;
                if d.abs() < 1e-10 {
                    return [(pa[0] + pb[0]) * 0.5, (pa[1] + pb[1]) * 0.5];
                }
                let alpha = (t - ta) / d;
                [
                    pa[0] + alpha * (pb[0] - pa[0]),
                    pa[1] + alpha * (pb[1] - pa[1]),
                ]
            };

            let start_s = if i == 0 { 0 } else { 1 };
            for s in start_s..=SEGMENTS_PER_SPAN {
                let u = s as f32 / SEGMENTS_PER_SPAN as f32;
                let t = t1 + u * (t2 - t1);

                // Level 1 (linear)
                let a1 = lerp(t0, t1, p0, p1, t);
                let a2 = lerp(t1, t2, p1, p2, t);
                let a3 = lerp(t2, t3, p2, p3, t);
                // Level 2
                let b1 = lerp(t0, t2, a1, a2, t);
                let b2 = lerp(t1, t3, a2, a3, t);
                // Level 3
                self.cached_curve.push(lerp(t1, t2, b1, b2, t));
            }
        }
    }

    /// Interpolating B-spline (uniform cubic).
    /// Solves for B-spline control points so the curve passes through all data points.
    fn tessellate_bspline_interp(&mut self) {
        let pts = &self.control_points;
        let n = pts.len();
        if n < 2 {
            return;
        }

        // Find B-spline control points C[0..n].
        // Fixed endpoints: C[0] = D[0], C[n-1] = D[n-1].
        // Interior condition: (1/6)*C_{i-1} + (4/6)*C_i + (1/6)*C_{i+1} = D_i
        let mut cx = vec![0.0f32; n];
        let mut cy = vec![0.0f32; n];
        cx[0] = pts[0][0];
        cy[0] = pts[0][1];
        cx[n - 1] = pts[n - 1][0];
        cy[n - 1] = pts[n - 1][1];

        let interior = n - 2;
        if interior > 0 {
            let a = vec![1.0f32 / 6.0; interior];
            let b = vec![4.0f32 / 6.0; interior];
            let c = vec![1.0f32 / 6.0; interior];

            let mut dx = vec![0.0f32; interior];
            let mut dy = vec![0.0f32; interior];
            for k in 0..interior {
                dx[k] = pts[k + 1][0];
                dy[k] = pts[k + 1][1];
            }
            // Adjust RHS for known endpoints.
            dx[0] -= (1.0 / 6.0) * cx[0];
            dy[0] -= (1.0 / 6.0) * cy[0];
            dx[interior - 1] -= (1.0 / 6.0) * cx[n - 1];
            dy[interior - 1] -= (1.0 / 6.0) * cy[n - 1];

            let sx = solve_tridiagonal(&a, &b, &c, &dx);
            let sy = solve_tridiagonal(&a, &b, &c, &dy);
            cx[1..=interior].copy_from_slice(&sx);
            cy[1..=interior].copy_from_slice(&sy);
        }

        // Phantom points: C_{-1} = 2*C_0 - C_1, C_n = 2*C_{n-1} - C_{n-2}
        let c_neg1 = [2.0 * cx[0] - cx[1], 2.0 * cy[0] - cy[1]];
        let c_nplus1 = [2.0 * cx[n - 1] - cx[n - 2], 2.0 * cy[n - 1] - cy[n - 2]];

        // Extended control point array: [C_{-1}, C_0, ..., C_{n-1}, C_n]
        let mut c_ext: Vec<[f32; 2]> = Vec::with_capacity(n + 2);
        c_ext.push(c_neg1);
        for k in 0..n {
            c_ext.push([cx[k], cy[k]]);
        }
        c_ext.push(c_nplus1);

        // Span i uses c_ext[i..i+4]. Uniform cubic B-spline basis functions.
        for i in 0..n - 1 {
            let q0 = c_ext[i];
            let q1 = c_ext[i + 1];
            let q2 = c_ext[i + 2];
            let q3 = c_ext[i + 3];

            let start_s = if i == 0 { 0 } else { 1 };
            for s in start_s..=SEGMENTS_PER_SPAN {
                let t = s as f32 / SEGMENTS_PER_SPAN as f32;
                let t2 = t * t;
                let t3 = t2 * t;
                let u = 1.0 - t;
                let b0 = u * u * u / 6.0;
                let b1 = (3.0 * t3 - 6.0 * t2 + 4.0) / 6.0;
                let b2 = (-3.0 * t3 + 3.0 * t2 + 3.0 * t + 1.0) / 6.0;
                let b3 = t3 / 6.0;
                self.cached_curve.push([
                    b0 * q0[0] + b1 * q1[0] + b2 * q2[0] + b3 * q3[0],
                    b0 * q0[1] + b1 * q1[1] + b2 * q2[1] + b3 * q3[1],
                ]);
            }
        }
    }
}

/// Thomas algorithm for a tridiagonal system.
/// a: sub-diagonal (a[0] unused), b: main diagonal,
/// c: super-diagonal (c[n-1] unused), d: right-hand side.
fn solve_tridiagonal(a: &[f32], b: &[f32], c: &[f32], d: &[f32]) -> Vec<f32> {
    let n = d.len();
    if n == 0 {
        return Vec::new();
    }

    let mut c_prime = vec![0.0f32; n];
    let mut d_prime = vec![0.0f32; n];
    let mut x = vec![0.0f32; n];

    // Forward elimination
    c_prime[0] = c[0] / b[0];
    d_prime[0] = d[0] / b[0];
    for i in 1..n {
        let m = b[i] - a[i] * c_prime[i - 1];
        c_prime[i] = c[i] / m; // unused when i == n-1
        d_prime[i] = (d[i] - a[i] * d_prime[i - 1]) / m;
    }

    // Back substitution
    x[n - 1] = d_prime[n - 1];
    for i in (0..n - 1).rev() {
        x[i] = d_prime[i] - c_prime[i] * x[i + 1];
    }

    x
}
