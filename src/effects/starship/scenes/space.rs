//! Shared pieces of the space scenes (storyboard §2.4–2.8): the Earth's limb,
//! sun glare and flare spikes, tiled hulls and the SECO starfield.
//!
//! Positions are normalised screen coordinates (u, v); lengths are in H. The
//! limb geometry is computed in world units (x = u·A, y = v) so circles stay
//! round on every grid.

use super::super::raster::{hash01, lerp3, ramp, Camera, Pixbuf, Rgb};
use super::onboard::{inside, on_seam, to_px};

/// Normalised screen point → world (x = u·A, y = v), undoing the camera.
#[inline]
pub fn world_of(buf: &Pixbuf, cam: &Camera, x: f64, y: f64) -> (f64, f64) {
    let a = buf.aspect();
    let us = x / buf.w as f64;
    let vs = y / buf.h as f64;
    let u = cam.pivot.0 + (us - cam.pan.0 - cam.shake.0 / a - cam.pivot.0) / cam.zoom;
    let v = cam.pivot.1 + (vs - cam.pan.1 - cam.shake.1 - cam.pivot.1) / cam.zoom;
    (u * a, v)
}

/// The circle through the normalised points `p` and `q` with radius `r` H,
/// its centre on the side of the chord that `toward` (a world-space
/// direction) points to. Returns the centre (world H) and the radius used:
/// `r`, or half the chord if the chord is longer than the diameter (on a
/// very wide screen), so the circle always passes through both points.
pub fn circle_through(p: (f64, f64), q: (f64, f64), r: f64, aspect: f64, toward: (f64, f64)) -> ((f64, f64), f64) {
    let (mx, my, nx, ny, half) = chord(p, q, aspect, toward);
    let r = r.max(half);
    let k = (r * r - half * half).max(0.0).sqrt();
    ((mx + nx * k, my + ny * k), r)
}

/// The circle through the normalised points `p` and `q` whose arc between
/// them bulges `depth` H away from `toward` (the centre's side): the radius
/// follows from the chord, so the arc keeps its depth at any aspect.
pub fn arc_by_depth(p: (f64, f64), q: (f64, f64), depth: f64, aspect: f64, toward: (f64, f64)) -> ((f64, f64), f64) {
    let (mx, my, nx, ny, half) = chord(p, q, aspect, toward);
    let r = (half * half + depth * depth) / (2.0 * depth);
    ((mx + nx * (r - depth), my + ny * (r - depth)), r)
}

/// Chord midpoint (world H), the unit normal toward `toward`, half-length.
fn chord(p: (f64, f64), q: (f64, f64), aspect: f64, toward: (f64, f64)) -> (f64, f64, f64, f64, f64) {
    let (px, py) = (p.0 * aspect, p.1);
    let (qx, qy) = (q.0 * aspect, q.1);
    let (dx, dy) = (qx - px, qy - py);
    let len = (dx * dx + dy * dy).sqrt();
    let (mut nx, mut ny) = (-dy / len, dx / len);
    if nx * toward.0 + ny * toward.1 < 0.0 {
        nx = -nx;
        ny = -ny;
    }
    ((px + qx) / 2.0, (py + qy) / 2.0, nx, ny, len / 2.0)
}

/// Colours of a limb: the far side of the circle, the atmosphere band just
/// outside it (inner → outer), and the sky beyond (near → far).
pub struct LimbColours<'a> {
    pub inside: Rgb,
    pub band: &'a [Rgb],
    pub sky_near: Rgb,
    pub sky_far: Rgb,
}

/// Fill the screen by signed distance to the circle (`centre` world H,
/// radius `r` H): `inside` within it, then a `band` H wide atmosphere, then
/// the sky, which reaches `sky_far` at `sky_span` H from the circle.
pub fn limb(buf: &mut Pixbuf, cam: &Camera, centre: (f64, f64), r: f64, band: f64, sky_span: f64, c: &LimbColours) {
    let w = buf.w;
    for y in 0..buf.h {
        for x in 0..w {
            let (wx, wy) = world_of(buf, cam, x as f64 + 0.5, y as f64 + 0.5);
            let d = ((wx - centre.0).powi(2) + (wy - centre.1).powi(2)).sqrt() - r;
            let col = if d < 0.0 {
                c.inside
            } else if d < band {
                ramp(c.band, (d / band) as f32)
            } else {
                lerp3(c.sky_near, c.sky_far, ((d - band) / sky_span).clamp(0.0, 1.0) as f32)
            };
            buf.px[y * w + x] = col;
        }
    }
}

/// A sun glare: a core of radius `0.4·r` fading into a halo of radius `r`
/// (H), both through the camera. With `solid > 0` a flat, fully bright disc
/// of radius `solid·r` sits in the middle (a blown-out sun).
#[allow(clippy::too_many_arguments)]
pub fn glare(buf: &mut Pixbuf, cam: &Camera, at: (f64, f64), r: f64, solid: f64, core: Rgb, halo: Rgb, alpha: f32) {
    let (cx, cy) = cam.px(buf, at.0, at.1);
    buf.radial(cx, cy, cam.lx(buf, r), cam.ly(buf, r), halo, halo, 0.55 * alpha, 0.0);
    let rc = 0.4f64.max(solid * 1.6) * r;
    buf.radial(cx, cy, cam.lx(buf, rc), cam.ly(buf, rc), core, halo, alpha, 0.5 * alpha);
    if solid > 0.0 {
        let rs = solid * r;
        buf.ellipse(cx, cy, cam.lx(buf, rs), cam.ly(buf, rs), core, alpha);
    }
}

/// `n` thin flare spikes `len` H long from `at`, starting at angle `a0`
/// (radians); alpha runs `alpha` → 0 along each. One pixel wide at the
/// 137×66 reference grid.
#[allow(clippy::too_many_arguments)]
pub fn spikes(buf: &mut Pixbuf, cam: &Camera, at: (f64, f64), n: usize, len: f64, a0: f64, col: Rgb, alpha: f32) {
    let (cx, cy) = cam.px(buf, at.0, at.1);
    let half = 0.6 * buf.h as f64 / 66.0;
    for k in 0..n {
        let th = a0 + k as f64 * std::f64::consts::TAU / n as f64;
        let (dx, dy) = (th.cos() * cam.lx(buf, len), th.sin() * cam.ly(buf, len));
        let l2 = (dx * dx + dy * dy).max(1e-9);
        let l = l2.sqrt();
        let (nx, ny) = (-dy / l * half, dx / l * half);
        let tri = [(cx + nx, cy + ny), (cx + dx, cy + dy), (cx - nx, cy - ny)];
        buf.polygon_with(&tri, |x, y| {
            let t = (((x - cx) * dx + (y - cy) * dy) / l2).clamp(0.0, 1.0) as f32;
            (col, alpha * (1.0 - t) * (1.0 - t))
        });
    }
}

/// Fill the hull polygon (normalised points) with hex tiles: `base` with
/// seam lines alternating between the two `seams` tones by pixel parity (as
/// scene 3 does). The tiles stay fixed to the hull under the camera shake.
pub fn tiled_hull(buf: &mut Pixbuf, cam: &Camera, poly: &[(f64, f64)], base: Rgb, seams: [Rgb; 2]) {
    let mut pts = [(0.0, 0.0); 8];
    let n = poly.len().min(8);
    to_px(buf, cam, &poly[..n], &mut pts[..n]);
    let (sx, sy) = (cam.shake.0 * buf.hx(), cam.shake.1 * buf.hy());
    let h = buf.h;
    buf.polygon_with(&pts[..n], |x, y| {
        if on_seam(h, x - sx, y - sy) {
            (seams[(x as i64 + y as i64).rem_euclid(2) as usize], 1.0)
        } else {
            (base, 1.0)
        }
    });
}

/// Whether the normalised point lies inside the polygon (normalised points).
pub fn inside_uv(poly: &[(f64, f64)], u: f64, v: f64) -> bool {
    inside(poly, u, v)
}

/// The SECO starfield: 60 fixed stars (the same sky every loop, no RNG).
pub const STARS: usize = 60;
const STAR_SALT: u64 = 0x5ec0_57a5;

/// Star `i`: position (u, v), base luma, twinkle frequency (Hz) and phase.
/// Twinkle is ±30 %, so the brightest a star gets is `luma · 1.3 ≤ 0.35`.
pub fn star(i: usize) -> (f64, f64, f64, f64, f64) {
    let h = |k: i64| hash01(i as i64, k, STAR_SALT);
    let u = h(0);
    let v = h(1);
    let luma = 0.12 + 0.14 * h(2);
    let freq = 0.5 + h(3);
    let phase = h(4) * std::f64::consts::TAU;
    (u, v, luma, freq, phase)
}

/// A star's luma at time t (with twinkle).
pub fn star_luma(i: usize, t: f64) -> f64 {
    let (_, _, l, f, p) = star(i);
    l * (1.0 + 0.3 * (std::f64::consts::TAU * f * t + p).sin())
}

/// Draw the stars at time t, each faded by `alpha · weight(u, v)`.
pub fn stars(buf: &mut Pixbuf, t: f64, alpha: f32, weight: impl Fn(f64, f64) -> f32) {
    if alpha <= 0.0 {
        return;
    }
    let size = (buf.h as f64 / 66.0).max(1.0);
    for i in 0..STARS {
        let (u, v, ..) = star(i);
        let a = alpha * weight(u, v);
        if a <= 0.0 {
            continue;
        }
        let g = (star_luma(i, t) * 255.0) as f32;
        let c = [g * 0.95, g, (g * 1.08).min(255.0)];
        let (x, y) = (u * buf.w as f64, v * buf.h as f64);
        buf.rect(x, y, x + size, y + size, c, a);
    }
}
