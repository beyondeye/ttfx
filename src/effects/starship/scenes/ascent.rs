//! Scene 2: ascent (storyboard §2.2). 2a tilt-up and zoom-out (10.0–12.5 s),
//! continuing into 2b, the small rocket climbing a clear sky (12.5–16.0 s).
//! One rocket model throughout: body B = 0.55 H · z · s, plume 0.67 B.

use crate::utils::easing::Easing;
use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::particles::EmitSpec;
use super::super::raster::{lerp, lerp3, Camera, Pixbuf, Rgb};
use super::super::timeline::{ease_between, SceneId};
use super::{billow, stops_at, Slot, World};

const SKY: Rgb = hex(0x2c507a);
const SKY_LOW: Rgb = hex(0x305581);
const SKY_MID: Rgb = hex(0x3b6187);
const HAZE_GREY: Rgb = hex(0x7b8894);
const HAZE: Rgb = hex(0x4f6271);
const COAST: Rgb = hex(0x1e3550);
const LAND: Rgb = hex(0x272929);
const STEEL: Rgb = hex(0x7b8894);
const PLUME: Rgb = hex(0xd0b3bd);
const PLUME_EDGE: Rgb = hex(0xa692a0);
const SMOKE: [Rgb; 3] = [hex(0xc3b2b9), hex(0xa39ea3), hex(0x7d8488)];

const PIVOT: (f64, f64) = (0.50, 0.77);

/// The remaining smoke column, lower left, in world (u, v) with radius (H).
const BILLOWS: &[(f64, f64, f64, u64)] = &[
    (0.02, 0.62, 0.17, 31),
    (0.12, 0.52, 0.14, 32),
    (0.05, 0.40, 0.15, 33),
    (0.20, 0.66, 0.11, 34),
    (0.14, 0.30, 0.12, 35),
];

/// Camera tilt: the world slides down 0.75 H over 10.0–11.6.
fn pan(t: f64) -> f64 {
    0.75 * ease_between(t, 10.0, 11.6, Easing::InOutSine)
}

/// Zoom-out progress over 11.95–13.3 (0 → 1).
fn zoom_e(t: f64) -> f64 {
    ease_between(t, 11.95, 13.3, Easing::InOutSine)
}

fn zoom(t: f64) -> f64 {
    1.0 - 0.71 * zoom_e(t)
}

fn camera(t: f64) -> Camera {
    let z = zoom(t);
    Camera { pan: (0.0, pan(t) * z), zoom: z, pivot: PIVOT, shake: (0.0, 0.0) }
}

/// The rocket at time t: tip (u, v), body length B (H) and tilt (radians).
pub fn rocket(t: f64) -> ((f64, f64), f64, f64) {
    if t < 11.95 {
        let e = ease_between(t, 10.0, 11.6, Easing::InOutSine);
        ((0.50, lerp(0.20, 0.08, e)), 0.55, 0.0)
    } else if t < 13.4 {
        let e = zoom_e(t);
        let z = zoom(t);
        ((lerp(0.50, 0.51, e), PIVOT.1 + (0.08 - PIVOT.1) * z), 0.55 * z, (10.0 * e).to_radians())
    } else {
        let k = ((t - 13.4) / 2.6).clamp(0.0, 1.0);
        let p = Easing::CubicBezier(0.25, 0.65, 1.0, 1.0).ease(k);
        let s = 1.0 - 0.625 * k;
        ((lerp(0.51, 0.57, p), lerp(0.57, 0.11, p)), 0.55 * 0.29 * s, (10.0 + 10.0 * k).to_radians())
    }
}

/// Axis from the tip back towards the engines, in pixels per H.
fn axis(buf: &Pixbuf, tilt: f64) -> (f64, f64) {
    (-tilt.sin() * buf.hx(), tilt.cos() * buf.hy())
}

fn plume_spec(tilt: f64) -> EmitSpec {
    EmitSpec {
        life: 0.6,
        speed: (0.2, 0.2),
        // Straight back along the body: the drawn axis tip → engines is
        // (−sin tilt, cos tilt), i.e. the angle π/2 + tilt.
        dir: std::f64::consts::FRAC_PI_2 + tilt,
        spread: 0.0,
        size: (1.0, 2.0),
        stops: [PLUME, PLUME_EDGE, PLUME_EDGE],
        nstops: 2,
        drag: 1.0,
        buoy: 0.0,
        alpha: 1.0,
    }
}

fn edge_spec(dir: f64) -> EmitSpec {
    EmitSpec {
        life: 1.5,
        speed: (0.04, 0.04),
        dir,
        spread: 0.0,
        size: (2.0, 2.0),
        stops: [hex(0xc3b2b9), hex(0x7d8488), hex(0x7d8488)],
        nstops: 2,
        drag: 0.5,
        buoy: -0.01,
        alpha: 0.8,
    }
}

pub fn step(world: &mut World, rng: &mut Rng, t: f64, dt: f64) {
    let a = world.aspect;
    let owner = SceneId::S2;
    let cam = camera(t);
    // Plume particles leave the engines; spawn them in world coordinates.
    let ((tu, tv), b, tilt) = rocket(t);
    let (bu, bv) = (tu - tilt.sin() * b / a, tv + tilt.cos() * b);
    let (wu, wv) = (cam.pivot.0 + (bu - cam.pivot.0) / cam.zoom, cam.pivot.1 + (bv - cam.pan.1 - cam.pivot.1) / cam.zoom);
    world.spawn(rng, Slot::S2Plume, &plume_spec(tilt), wu * a, wv, owner, dt, 300.0);
    if t < 12.5 {
        let i = (rng.random() * BILLOWS.len() as f64) as usize % BILLOWS.len();
        let ang = rng.uniform(0.0, std::f64::consts::TAU);
        let (u, v, r, _) = BILLOWS[i];
        let (x, y) = (u * a + r * ang.cos(), v + r * ang.sin());
        world.spawn(rng, Slot::S2Edge, &edge_spec(ang), x, y, owner, dt, 300.0);
    }
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = camera(t);
    let e = zoom_e(t) as f32;
    let (w, h) = (buf.w, buf.h);
    // 2a: the world's sky, haze band and coast seen through the camera; 2b:
    // a clear sky with a lighter band at the bottom. The zoom crossfades them.
    let world_stops = [(-3.0, SKY), (-0.58, SKY), (-0.42, SKY_MID), (-0.30, HAZE_GREY), (-0.08, HAZE_GREY), (0.05, HAZE), (0.06, COAST), (0.40, LAND)];
    for y in 0..h {
        let vs = (y as f64 + 0.5) / h as f64;
        let vw = cam.pivot.1 + (vs - cam.pan.1 - cam.pivot.1) / cam.zoom;
        let c_world = stops_at(&world_stops, vw);
        let c_clear = stops_at(&[(0.0, SKY), (0.68, SKY_LOW), (0.80, SKY_MID), (1.0, SKY_MID)], vs);
        let c = lerp3(c_world, c_clear, e);
        buf.px[y * w..(y + 1) * w].fill(c);
    }
    // The smoke column's remains slide out of frame with the tilt.
    for &(u, v, r, seed) in BILLOWS {
        billow(buf, &cam, u, v, r, SMOKE, 1.0, seed);
    }
    world.pool.draw(buf, &cam, SceneId::S2.tag(), 1.0);
    draw_rocket(buf, t);
}

fn draw_rocket(buf: &mut Pixbuf, t: f64) {
    let cam = Camera::default();
    let ((tu, tv), b, tilt) = rocket(t);
    let (xt, yt) = cam.px(buf, tu, tv);
    let (ax, ay) = axis(buf, tilt);
    let (xb, yb) = (xt + ax * b, yt + ay * b);
    let width = (b / 13.7 * buf.hx()).max(1.0);
    // Faint grey trail, then the pink plume, then the steel body on top.
    let (p, trail) = (0.67 * b, 0.5 * b);
    let (xp, yp) = (xb + ax * p, yb + ay * p);
    for i in 0..6 {
        let k0 = i as f64 / 6.0;
        let k1 = (i + 1) as f64 / 6.0;
        let (x0, y0) = (xp + ax * trail * k0, yp + ay * trail * k0);
        let (x1, y1) = (xp + ax * trail * k1, yp + ay * trail * k1);
        buf.line(x0, y0, x1, y1, width * 1.2, hex(0x8d9da6), 0.5 * (1.0 - k0 as f32));
    }
    buf.line(xb, yb, xp, yp, width * 1.6, PLUME_EDGE, 1.0);
    buf.line(xb, yb, xb + ax * p * 0.8, yb + ay * p * 0.8, width, PLUME, 1.0);
    buf.line(xt, yt, xb, yb, width, STEEL, 1.0);
}
