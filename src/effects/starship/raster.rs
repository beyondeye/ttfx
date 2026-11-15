//! Drawing primitives over a half-block pixel buffer.
//!
//! The canvas is `cols × 2·rows` pixels: each cell shows its top pixel as the
//! `▀` foreground and its bottom pixel as the background. Positions are given
//! in normalised screen coordinates `(u, v)` ∈ [0,1)², v down; lengths in `H`
//! (the screen height). Storyboard §1.1: one H is `h` pixels vertically and
//! `h · 8/7` pixels horizontally (half-block pixels are 14×16 screen px).
//!
//! Nothing here allocates once the buffers exist (`scratch` is reused).

/// RGB, channels 0–255.
pub type Rgb = [f32; 3];

pub struct Pixbuf {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgb>,
    scratch: Vec<f64>,
}

impl Pixbuf {
    pub fn new(w: usize, h: usize) -> Pixbuf {
        Pixbuf { w, h, px: vec![[0.0; 3]; w * h], scratch: Vec::with_capacity(64) }
    }

    /// Horizontal pixels per H.
    #[inline]
    pub fn hx(&self) -> f64 {
        self.h as f64 * 8.0 / 7.0
    }

    /// Vertical pixels per H.
    #[inline]
    pub fn hy(&self) -> f64 {
        self.h as f64
    }

    /// Screen aspect A = width / height in H units.
    pub fn aspect(&self) -> f64 {
        self.w as f64 / self.hx()
    }

    pub fn fill(&mut self, c: Rgb) {
        self.px.fill(c);
    }

    pub fn copy_from(&mut self, other: &Pixbuf) {
        self.px.copy_from_slice(&other.px);
    }

    /// `self = mix(self, other, w)` per pixel.
    pub fn mix_with(&mut self, other: &Pixbuf, w: f32) {
        for (a, b) in self.px.iter_mut().zip(other.px.iter()) {
            for k in 0..3 {
                a[k] += (b[k] - a[k]) * w;
            }
        }
    }

    #[inline]
    pub fn blend(&mut self, x: i64, y: i64, c: Rgb, a: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h || a <= 0.0 {
            return;
        }
        let p = &mut self.px[y as usize * self.w + x as usize];
        if a >= 1.0 {
            *p = c;
        } else {
            for k in 0..3 {
                p[k] += (c[k] - p[k]) * a;
            }
        }
    }

    /// Vertical gradient between pixel rows `y0` (colour `c0`) and `y1` (`c1`),
    /// clamped outside; fills every column.
    pub fn vgradient(&mut self, y0: f64, y1: f64, c0: Rgb, c1: Rgb) {
        for y in 0..self.h {
            let yc = y as f64 + 0.5;
            let t = if y1 == y0 { 0.0 } else { ((yc - y0) / (y1 - y0)).clamp(0.0, 1.0) } as f32;
            let c = lerp3(c0, c1, t);
            self.px[y * self.w..(y + 1) * self.w].fill(c);
        }
    }

    /// Fill the axis-aligned pixel rectangle [x0,x1)×[y0,y1) (pixel centres).
    pub fn rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, c: Rgb, a: f32) {
        let (xa, xb) = (span_start(x0), span_end(x1, self.w));
        let (ya, yb) = (span_start(y0), span_end(y1, self.h));
        for y in ya..yb {
            for x in xa..xb {
                self.blend(x as i64, y as i64, c, a);
            }
        }
    }

    /// Filled ellipse centred at pixel (cx, cy) with radii (rx, ry) pixels.
    /// A radius under one pixel still draws the centre pixel.
    pub fn ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, c: Rgb, a: f32) {
        if rx < 0.75 && ry < 0.75 {
            self.blend(cx.floor() as i64, cy.floor() as i64, c, a);
            return;
        }
        let y0 = (cy - ry).floor().max(0.0) as i64;
        let y1 = ((cy + ry).ceil() as i64).min(self.h as i64 - 1);
        for y in y0..=y1 {
            let dy = (y as f64 + 0.5 - cy) / ry.max(0.5);
            if dy.abs() > 1.0 {
                continue;
            }
            let half = rx.max(0.5) * (1.0 - dy * dy).sqrt();
            let xa = span_start(cx - half);
            let xb = span_end(cx + half, self.w);
            for x in xa..xb {
                self.blend(x as i64, y, c, a);
            }
        }
    }

    /// Even-odd scanline fill of a polygon given in pixel coordinates.
    pub fn polygon(&mut self, pts: &[(f64, f64)], c: Rgb, a: f32) {
        self.polygon_with(pts, |_, _| (c, a));
    }

    /// A polygon filled with a linear gradient: each pixel takes the colour of
    /// its projection onto the pixel-space segment (x0, y0) → (x1, y1),
    /// clamped to `c0` before the start and `c1` after the end.
    #[allow(clippy::too_many_arguments)]
    pub fn polygon_linear(&mut self, pts: &[(f64, f64)], x0: f64, y0: f64, x1: f64, y1: f64, c0: Rgb, c1: Rgb, a: f32) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = (dx * dx + dy * dy).max(1e-12);
        self.polygon_with(pts, |x, y| {
            let t = (((x - x0) * dx + (y - y0) * dy) / len2).clamp(0.0, 1.0) as f32;
            (lerp3(c0, c1, t), a)
        });
    }

    /// Elliptical radial gradient centred at pixel (cx, cy) with radii
    /// (rx, ry): colour `inner` → `outer` and alpha `a_inner` → `a_outer` from
    /// the centre to the rim. Pixels outside the rim are untouched.
    #[allow(clippy::too_many_arguments)]
    pub fn radial(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, inner: Rgb, outer: Rgb, a_inner: f32, a_outer: f32) {
        let (rx, ry) = (rx.max(0.5), ry.max(0.5));
        let y0 = (cy - ry).floor().max(0.0) as i64;
        let y1 = ((cy + ry).ceil() as i64).min(self.h as i64 - 1);
        let x0 = (cx - rx).floor().max(0.0) as i64;
        let x1 = ((cx + rx).ceil() as i64).min(self.w as i64 - 1);
        for y in y0..=y1 {
            let dy = (y as f64 + 0.5 - cy) / ry;
            for x in x0..=x1 {
                let dx = (x as f64 + 0.5 - cx) / rx;
                let r = (dx * dx + dy * dy).sqrt();
                if r > 1.0 {
                    continue;
                }
                let t = r as f32;
                self.blend(x, y, lerp3(inner, outer, t), a_inner + (a_outer - a_inner) * t);
            }
        }
    }

    /// Even-odd scanline fill where `shade(x, y)` gives each pixel centre's
    /// colour and alpha.
    pub fn polygon_with(&mut self, pts: &[(f64, f64)], shade: impl Fn(f64, f64) -> (Rgb, f32)) {
        if pts.len() < 3 {
            return;
        }
        let (mut ymin, mut ymax) = (f64::MAX, f64::MIN);
        for p in pts {
            ymin = ymin.min(p.1);
            ymax = ymax.max(p.1);
        }
        let y0 = ymin.floor().max(0.0) as i64;
        let y1 = (ymax.ceil() as i64).min(self.h as i64 - 1);
        let mut xs = std::mem::take(&mut self.scratch);
        for y in y0..=y1 {
            let yc = y as f64 + 0.5;
            xs.clear();
            for i in 0..pts.len() {
                let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
                if (p.1 <= yc && q.1 > yc) || (q.1 <= yc && p.1 > yc) {
                    xs.push(p.0 + (yc - p.1) / (q.1 - p.1) * (q.0 - p.0));
                }
            }
            xs.sort_unstable_by(|a, b| a.total_cmp(b));
            for pair in xs.as_chunks::<2>().0 {
                let xa = span_start(pair[0]);
                let xb = span_end(pair[1], self.w);
                for x in xa..xb {
                    let (c, a) = shade(x as f64 + 0.5, y as f64 + 0.5);
                    self.blend(x as i64, y, c, a);
                }
            }
        }
        self.scratch = xs;
    }

    /// A thick line segment between two pixel points.
    #[allow(clippy::too_many_arguments)]
    pub fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, width: f64, c: Rgb, a: f32) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            self.ellipse(x0, y0, width / 2.0, width / 2.0, c, a);
            return;
        }
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        self.polygon(&[(x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny), (x0 - nx, y0 - ny)], c, a);
    }
}

#[inline]
fn span_start(x: f64) -> usize {
    // first pixel whose centre is >= x
    (x - 0.5).ceil().max(0.0) as usize
}

#[inline]
fn span_end(x: f64, limit: usize) -> usize {
    // one past the last pixel whose centre is < x
    ((x - 0.5).ceil().max(0.0) as usize).min(limit)
}

#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[inline]
pub fn lerp3(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Linear interpolation through evenly spaced stops.
pub fn ramp(stops: &[Rgb], t: f32) -> Rgb {
    match stops.len() {
        0 => [0.0; 3],
        1 => stops[0],
        n => {
            let x = t.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (x as usize).min(n - 2);
            lerp3(stops[i], stops[i + 1], x - i as f32)
        }
    }
}

/// 2-D shot camera: maps world points (u, v) to pixels. `zoom` scales about
/// `pivot`, `pan` shifts (both in normalised units) and `shake` adds a small
/// offset in H.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub pan: (f64, f64),
    pub zoom: f64,
    pub pivot: (f64, f64),
    pub shake: (f64, f64),
}

impl Default for Camera {
    fn default() -> Camera {
        Camera { pan: (0.0, 0.0), zoom: 1.0, pivot: (0.5, 0.5), shake: (0.0, 0.0) }
    }
}

impl Camera {
    /// World (u, v) → pixel (x, y).
    #[inline]
    pub fn px(&self, buf: &Pixbuf, u: f64, v: f64) -> (f64, f64) {
        let a = buf.aspect();
        let us = self.pivot.0 + (u - self.pivot.0) * self.zoom + self.pan.0 + self.shake.0 / a;
        let vs = self.pivot.1 + (v - self.pivot.1) * self.zoom + self.pan.1 + self.shake.1;
        (us * buf.w as f64, vs * buf.h as f64)
    }

    /// A horizontal length in H → pixels.
    #[inline]
    pub fn lx(&self, buf: &Pixbuf, h: f64) -> f64 {
        h * self.zoom * buf.hx()
    }

    /// A vertical length in H → pixels.
    #[inline]
    pub fn ly(&self, buf: &Pixbuf, h: f64) -> f64 {
        h * self.zoom * buf.hy()
    }
}

/// Shake offset (H) at time t: per axis, two sines at f and 1.7f.
pub fn shake(amp: f64, freq: f64, t: f64, phases: &[f64; 4]) -> (f64, f64) {
    use std::f64::consts::TAU;
    let s = |p0: f64, p1: f64| {
        amp * 0.5 * ((TAU * freq * t + p0).sin() + (TAU * 1.7 * freq * t + p1).sin())
    };
    (s(phases[0], phases[1]), s(phases[2], phases[3]))
}

/// Cheap deterministic value noise in [0, 1) (no RNG state).
pub fn hash01(x: i64, y: i64, salt: u64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E3779B97F4A7C15)
        ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F)
        ^ salt.wrapping_mul(0x165667B19E3779F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    (h >> 11) as f64 / (1u64 << 53) as f64
}
