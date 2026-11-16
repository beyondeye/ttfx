//! Scene 4: high altitude (storyboard §2.4, 22–25 s). A near-black navy sky,
//! the engines as a white point with a pink/mauve plume and a wide dim glow,
//! and in the last second a translucent cone ahead of the staging whiteout.

use crate::utils::easing::Easing;
use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::particles::EmitSpec;
use super::super::raster::{Camera, Pixbuf, Rgb};
use super::super::timeline::{ease_between, SceneId};
use super::{space, Slot, World};

const SKY_TOP: Rgb = hex(0x0e1826);
const SKY: Rgb = hex(0x0d1725);
const PLUME_DIM: Rgb = hex(0x392d3a);
const PLUME: Rgb = hex(0x694f5c);
const CONE: Rgb = hex(0x414550);
const CORE: Rgb = hex(0xe6e1db);

const START: f64 = 22.0;
const END: f64 = 25.0;

/// The engine point at time t: it drifts up 0.01 H/s.
pub fn engine(t: f64) -> (f64, f64) {
    (0.50, 0.46 - 0.01 * (t - START))
}

fn plume_spec() -> EmitSpec {
    EmitSpec {
        life: 1.2,
        speed: (0.15, 0.15),
        dir: std::f64::consts::FRAC_PI_2,
        spread: 0.0,
        size: (1.0, 2.0),
        stops: [CORE, PLUME, PLUME_DIM],
        nstops: 3,
        drag: 0.6,
        buoy: 0.0,
        alpha: 1.0,
    }
}

pub fn step(world: &mut World, rng: &mut Rng, t: f64, dt: f64) {
    let (u, v) = engine(t);
    let x = u * world.aspect;
    world.spawn(rng, Slot::S4Plume, &plume_spec(), x, v, SceneId::S4, dt, 1500.0);
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = Camera::default();
    buf.vgradient(0.0, buf.h as f64, SKY_TOP, SKY);
    let (eu, ev) = engine(t);
    // The wide dim glow around the plume: most of it is a mix that is "near"
    // both the sky and the plume's dim colour.
    let (gx, gy) = cam.px(buf, eu, ev + 0.03);
    buf.radial(gx, gy, cam.lx(buf, 0.46), cam.ly(buf, 0.40), PLUME_DIM, PLUME_DIM, 0.70, 0.0);
    // The plume: down from the engine to v ≈ 0.80, pulsing ±10 % at 2 Hz.
    let pulse = 1.0 + 0.10 * (std::f64::consts::TAU * 2.0 * (t - START)).sin();
    let len = (0.80 - ev) * pulse;
    let (x0, y0) = cam.px(buf, eu, ev);
    let (x1, y1) = cam.px(buf, eu, ev + len);
    let (w0, w1) = (cam.lx(buf, 0.02), cam.lx(buf, 0.035));
    let quad = [(x0 - w0, y0), (x0 + w0, y0), (x1 + w1, y1), (x1 - w1, y1)];
    buf.polygon_linear(&quad, x0, y0, x1, y1, PLUME, PLUME_DIM, 0.85);
    // The cone opening in the last second.
    let c = ease_between(t, END - 1.0, END, Easing::OutQuad);
    if c > 0.0 {
        let clen = cam.ly(buf, c * (1.1 - ev));
        let half = 25f64.to_radians().tan() * clen * buf.hx() / buf.hy();
        let tri = [(x0, y0), (x0 + half, y0 + clen), (x0 - half, y0 + clen)];
        buf.polygon(&tri, CONE, 0.25);
    }
    world.pool.draw(buf, &cam, SceneId::S4.tag(), 1.0);
    // The engines: a white point with a glare.
    space::glare(buf, &cam, (eu, ev), 0.05, 0.0, CORE, PLUME, 0.9);
    buf.ellipse(x0, y0, cam.lx(buf, 0.015), cam.ly(buf, 0.015), CORE, 1.0);
}
