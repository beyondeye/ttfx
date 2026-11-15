//! Scene 1: liftoff (storyboard §2.1). 1a pad wide (0–6 s), hard cut, 1b aerial
//! (6–10 s), then a 0.5 s cross-fade into the ascent.

use crate::utils::easing::Easing;
use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::particles::EmitSpec;
use super::super::raster::{shake, Camera, Pixbuf, Rgb};
use super::super::timeline::{ease_between, SceneId};
use super::{billow, noise2, vstops, Slot, World};

const SKY_TOP: Rgb = hex(0x1e3d4e);
const SKY: Rgb = hex(0x23465a);
const SKY_HORIZON: Rgb = hex(0x406e88);
const GROUND_HAZE: Rgb = hex(0x799298);
const STEEL: Rgb = hex(0x60615a);
const STEEL_DARK: Rgb = hex(0x4b483d);
const STEEL_LIGHT: Rgb = hex(0x8d9da6);
const SMOKE: [Rgb; 3] = [hex(0xdacbad), hex(0xb3a086), hex(0x8f785f)];
const COLUMN: [Rgb; 3] = [hex(0xd9c9b3), hex(0xb8a790), hex(0x9e8b78)];
const FLAME: Rgb = hex(0xe2af7b);
const FLAME_CORE: Rgb = hex(0xf3d592);
const MARSH: Rgb = hex(0x272929);
const MARSH_DARK: Rgb = hex(0x11171b);

/// The 1a pad: the stack's base at liftoff.
const PAD: (f64, f64) = (0.47, 0.75);
const STACK_H: f64 = 0.63;
const STACK_W: f64 = 0.046;

/// Billows: offset from the pad at full size (u, v), radius (H), seed.
/// Back to front; the right cloud is the big one, as in the reference.
const BILLOWS_1A: &[(f64, f64, f64, u64)] = &[
    (0.50, -0.50, 0.20, 10),
    (0.44, -0.34, 0.22, 11),
    (0.36, -0.20, 0.22, 12),
    (-0.36, -0.20, 0.17, 13),
    (0.26, -0.10, 0.20, 14),
    (-0.42, -0.06, 0.19, 15),
    (-0.26, -0.08, 0.17, 16),
    (0.48, -0.02, 0.19, 17),
    (0.16, -0.04, 0.16, 18),
    (-0.34, 0.04, 0.15, 19),
    (-0.14, -0.02, 0.14, 20),
    (0.32, 0.05, 0.15, 21),
    (0.06, 0.03, 0.12, 22),
    (-0.06, 0.04, 0.11, 23),
];

fn smoke_spec(dir: f64) -> EmitSpec {
    EmitSpec {
        life: 4.0,
        speed: (0.10, 0.35),
        dir,
        spread: 12.5f64.to_radians(),
        size: (1.0, 4.0),
        stops: SMOKE,
        nstops: 3,
        drag: 0.8,
        buoy: -0.03,
        alpha: 1.0,
    }
}

fn flame_spec() -> EmitSpec {
    EmitSpec {
        life: 0.4,
        speed: (0.3, 0.3),
        dir: 90f64.to_radians(),
        spread: 40f64.to_radians(),
        size: (1.0, 1.0),
        stops: [FLAME_CORE, FLAME, hex(0x9e7f60)],
        nstops: 3,
        drag: 3.0,
        buoy: 0.0,
        alpha: 1.0,
    }
}

fn column_spec() -> EmitSpec {
    EmitSpec {
        life: 4.0,
        speed: (0.05, 0.05),
        dir: (-90.0f64 - 35.0).to_radians(),
        spread: 0.0,
        size: (2.0, 5.0),
        stops: [hex(0xd9c9b3), hex(0xb19e89), hex(0xb19e89)],
        nstops: 2,
        drag: 0.5,
        buoy: -0.01,
        alpha: 0.9,
    }
}

/// 1a: the stack's rise (H) at time t.
fn rise(t: f64) -> f64 {
    0.08 * ease_between(t, 0.0, 6.0, Easing::InQuad)
}

/// 1a: billow growth 0 → 1 over 3.3 s.
fn growth(t: f64) -> f64 {
    ease_between(t, 0.0, 3.3, Easing::CubicBezier(0.325, 0.912, 0.0, 0.7))
}

/// 1b: the rocket's base (u, v).
fn rocket_1b(t: f64) -> (f64, f64) {
    (0.52, 0.60 - 0.08 * (t - 6.0))
}

/// 1b: the smoke column's foot and top (u, v).
fn column_1b(t: f64, aspect: f64) -> ((f64, f64), (f64, f64)) {
    let drift = 0.02 * (t - 6.0) / std::f64::consts::SQRT_2;
    ((0.50, 0.75), (0.15 - drift / aspect, 0.05 - drift))
}

fn camera_1a(world: &World, t: f64) -> Camera {
    let amp = if t < 0.3 { 0.0 } else { 0.004 + 0.004 * ((t - 0.3) / 1.7).clamp(0.0, 1.0) };
    Camera { shake: shake(amp, 7.0, t, &world.phases[0]), ..Camera::default() }
}

pub fn step(world: &mut World, rng: &mut Rng, t: f64, dt: f64) {
    let a = world.aspect;
    let owner = SceneId::S1;
    if t < 6.0 {
        let rate = 900.0 * Easing::OutCubic.ease((t / 1.5).min(1.0));
        let (px, py) = (PAD.0 * a, PAD.1 - rise(t));
        world.spawn(rng, Slot::S1Smoke, &smoke_spec(-12.5f64.to_radians()), px + 0.03, py, owner, dt, rate / 2.0);
        world.spawn(rng, Slot::S1Smoke, &smoke_spec((180.0f64 + 12.5).to_radians()), px - 0.03, py, owner, dt, rate / 2.0);
        world.spawn(rng, Slot::S1Flames, &flame_spec(), px, py, owner, dt, 400.0);
    } else {
        let ((fu, fv), (tu, tv)) = column_1b(t, a);
        let k = rng.random();
        let (cu, cv) = (fu + (tu - fu) * k, fv + (tv - fv) * k);
        world.spawn(rng, Slot::S1Column, &column_spec(), cu * a, cv, owner, dt, 700.0);
        let (ru, rv) = rocket_1b(t);
        world.spawn(rng, Slot::S1ColumnFlames, &flame_spec(), ru * a, rv, owner, dt, 400.0);
    }
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    if t < 6.0 {
        draw_1a(world, t, buf, true);
    } else {
        draw_1b(world, t, buf);
    }
}

/// Scene 1 at time t with no particles: the loop fade's target at t = 0.
pub fn draw_static(world: &World, t: f64, buf: &mut Pixbuf) {
    draw_1a(world, t, buf, false);
}

fn draw_1a(world: &World, t: f64, buf: &mut Pixbuf, particles: bool) {
    let cam = camera_1a(world, t);
    vstops(
        buf,
        &cam,
        &[(0.0, SKY_TOP), (0.40, SKY_TOP), (0.62, SKY), (0.72, SKY_HORIZON), (0.725, GROUND_HAZE), (0.86, STEEL), (1.0, STEEL_DARK)],
    );
    draw_tower(buf, &cam);
    draw_stack(buf, &cam, PAD.0, PAD.1 - rise(t), STACK_H, STACK_W, true);
    let g = growth(t);
    for &(du, dv, r, seed) in BILLOWS_1A {
        billow(buf, &cam, PAD.0 + du * g, PAD.1 + dv * g, r * g, SMOKE, 1.0, seed);
    }
    if particles {
        world.pool.draw(buf, &cam, SceneId::S1.tag(), 1.0);
    }
}

fn draw_tower(buf: &mut Pixbuf, cam: &Camera) {
    let (u0, u1, top, foot) = (0.43, 0.46, 0.05, 0.72);
    let (x0, y0) = cam.px(buf, u0, top);
    let (x1, y1) = cam.px(buf, u1, foot);
    for x in [x0, x1] {
        buf.line(x, y0, x, y1, 1.0, STEEL_DARK, 1.0);
    }
    // Bays of 0.05 H with an X brace each.
    let bay = 0.05;
    let mut v = top;
    while v + bay <= foot + 1e-9 {
        let (_, ya) = cam.px(buf, u0, v);
        let (_, yb) = cam.px(buf, u0, v + bay);
        buf.line(x0, ya, x1, yb, 1.0, STEEL, 0.8);
        buf.line(x1, ya, x0, yb, 1.0, STEEL, 0.8);
        v += bay;
    }
    // Chopstick arms at v 0.17, 0.10 H each side, open at ±15°.
    let (cx, cy) = cam.px(buf, (u0 + u1) / 2.0, 0.17);
    for side in [-1.0, 1.0] {
        let ang = 15f64.to_radians();
        let (dx, dy) = (side * cam.lx(buf, 0.10) * ang.cos(), -cam.ly(buf, 0.10) * ang.sin());
        buf.line(cx, cy, cx + dx, cy + dy, 1.0, STEEL_DARK, 1.0);
    }
}

/// The stack: booster (lower 0.577) and ship, with grid fins and flaps.
/// `base` is the bottom (u, v); `len` and `width` in H.
fn draw_stack(buf: &mut Pixbuf, cam: &Camera, u: f64, base: f64, len: f64, width: f64, flame: bool) {
    let (xb, yb) = cam.px(buf, u, base);
    let (_, yt) = cam.px(buf, u, base - len);
    let hw = cam.lx(buf, width) / 2.0;
    let nose = cam.ly(buf, len * 0.06);
    buf.polygon(
        &[(xb - hw, yb), (xb - hw, yt + nose), (xb, yt), (xb + hw, yt + nose), (xb + hw, yb)],
        STEEL,
        1.0,
    );
    buf.line(xb - hw + 0.5, yt + nose, xb - hw + 0.5, yb, 1.0, STEEL_LIGHT, 0.6);
    // Interstage, grid fins (just below it) and the forward flaps.
    let y_inter = yb - (yb - yt) * 0.577;
    buf.line(xb - hw, y_inter, xb + hw, y_inter, 1.0, STEEL_DARK, 1.0);
    let fin = cam.lx(buf, width * 0.5);
    buf.rect(xb - hw - fin, y_inter + 1.0, xb - hw, y_inter + 2.0, STEEL_DARK, 1.0);
    buf.rect(xb + hw, y_inter + 1.0, xb + hw + fin, y_inter + 2.0, STEEL_DARK, 1.0);
    let y_flap = yt + nose + 1.0;
    buf.rect(xb - hw - 1.0, y_flap, xb - hw, y_flap + 2.0, STEEL_DARK, 1.0);
    buf.rect(xb + hw, y_flap, xb + hw + 1.0, y_flap + 2.0, STEEL_DARK, 1.0);
    if flame {
        buf.radial(xb, yb + cam.ly(buf, 0.03), hw * 2.2, cam.ly(buf, 0.045), FLAME_CORE, FLAME, 1.0, 0.8);
    }
}

fn draw_1b(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = Camera::default();
    let a = buf.aspect();
    let (w, h) = (buf.w, buf.h);
    // Marsh with water channels, and darker water beyond the coastline arc.
    for y in 0..h {
        for x in 0..w {
            let (u, v) = ((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64);
            let n = noise2(u * 18.0, v * 9.0, 101);
            let mut c = super::super::raster::lerp3(MARSH, hex(0x1d2224), (n * 1.4).min(1.0) as f32);
            let channel = (noise2(u * 7.0, v * 4.0, 202) - 0.5).abs();
            if channel < 0.03 {
                c = MARSH_DARK;
            }
            let (du, dv) = ((u - 1.25) * a, v - 1.2);
            if (du * du + dv * dv).sqrt() < 1.0 + 0.04 * (noise2(u * 10.0, v * 10.0, 303) - 0.5) {
                c = super::super::raster::lerp3(MARSH_DARK, hex(0x0e1826), (n * 0.6) as f32);
            }
            buf.px[y * w + x] = c;
        }
    }
    // Glow at the pad, under the column's foot.
    let (gx, gy) = cam.px(buf, 0.47, 0.74);
    buf.radial(gx, gy, cam.lx(buf, 0.16), cam.ly(buf, 0.08), FLAME_CORE, hex(0x9e7f60), 1.0, 0.6);
    // The smoke column, foot to top, widening to 0.35 H.
    let ((fu, fv), (tu, tv)) = column_1b(t, a);
    let n = 14;
    for i in 0..n {
        let k = i as f64 / (n - 1) as f64;
        let r = 0.12 + (0.27 - 0.12) * k.sqrt();
        billow(buf, &cam, fu + (tu - fu) * k, fv + (tv - fv) * k, r, COLUMN, 1.0, 400 + i as u64);
    }
    world.pool.draw(buf, &cam, SceneId::S1.tag(), 1.0);
    // The rocket climbing over it, with its short flame.
    let (ru, rv) = rocket_1b(t);
    draw_stack(buf, &cam, ru, rv, 0.30, 0.30 / 13.7, false);
    let (xb, yb) = cam.px(buf, ru, rv);
    buf.ellipse(xb, yb + cam.ly(buf, 0.03), cam.lx(buf, 0.012), cam.ly(buf, 0.03), FLAME_CORE, 1.0);
}
