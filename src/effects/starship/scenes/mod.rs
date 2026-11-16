//! Scene state, stepping and drawing.
//!
//! `World::step` advances everything that has state (particles, emitters) by
//! one tick of storyboard time; `World::draw` is a pure function of that state
//! and the time, so `--start-at` can fast-forward by stepping alone.

pub mod ascent;
pub mod coast;
pub mod high_altitude;
pub mod hot_staging;
pub mod onboard;
pub mod over_earth;
pub mod pad;
pub mod seco;
pub mod space;

use crate::utils::rng::Rng;

use super::particles::{EmitSpec, Pool};
use super::raster::{hash01, Camera, Pixbuf};
use super::timeline::{self, scene_at, SceneId};

/// The owner tag of the stress layer's particles.
pub const STRESS_OWNER: u8 = 200;

/// Emitter accumulator slots (one per particle system).
#[derive(Debug, Clone, Copy)]
pub enum Slot {
    S1Smoke = 0,
    S1Flames,
    S1Column,
    S1ColumnFlames,
    S2Plume,
    S2Edge,
    S3Exhaust,
    S3Debris,
    S4Plume,
    S5Burst,
    S5Sparks,
    S5Flakes,
    S6Flakes,
    S9Ice,
    S10Haze,
    S10Ice,
}

const SLOTS: usize = 16;

pub struct World {
    pub pool: Pool,
    pub acc: [f64; SLOTS],
    /// Shake phases per shot, drawn from the seed in `build`.
    pub phases: [[f64; 4]; 4],
    pub stress: bool,
    /// Screen aspect A (width / height in H): world x = u·A.
    pub aspect: f64,
}

impl World {
    pub fn new(rng: &mut Rng, stress: bool, aspect: f64) -> World {
        let mut phases = [[0.0; 4]; 4];
        for shot in phases.iter_mut() {
            for p in shot.iter_mut() {
                *p = rng.uniform(0.0, std::f64::consts::TAU);
            }
        }
        World { pool: Pool::new(), acc: [0.0; SLOTS], phases, stress, aspect }
    }

    /// Advance from `t0` to `t1 = t0 + dt` (storyboard seconds).
    pub fn step(&mut self, rng: &mut Rng, t0: f64, t1: f64, dt: f64) {
        if self.stress {
            self.pool.step(dt);
            stress_spawn(&mut self.pool, rng);
            return;
        }
        match timeline::pool_event(t0, t1) {
            timeline::PoolEvent::Clear => {
                self.pool.clear();
                self.acc = [0.0; SLOTS];
            }
            timeline::PoolEvent::KillOwner(id) => self.pool.kill_owner(id.tag()),
            timeline::PoolEvent::None => {}
        }
        self.pool.step(dt);
        // Only the current scene's emitters run; a fading-out scene stops spawning.
        match scene_at(t1) {
            SceneId::S1 => pad::step(self, rng, t1, dt),
            SceneId::S2 => ascent::step(self, rng, t1, dt),
            SceneId::S3 => onboard::step(self, rng, t1, dt),
            SceneId::S4 => high_altitude::step(self, rng, t1, dt),
            SceneId::S5 => hot_staging::step(self, rng, t1, dt),
            SceneId::S6 => coast::step(self, rng, t1, dt),
            SceneId::S9 => over_earth::step(self, rng, t1, dt),
            SceneId::S10 => seco::step(self, rng, t1, dt),
            _ => {}
        }
    }

    /// Draw one scene's layer at time t (no blending).
    pub fn draw(&self, scene: SceneId, t: f64, buf: &mut Pixbuf) {
        if self.stress {
            draw_stress(self, t, buf);
            return;
        }
        match scene {
            SceneId::S1 => pad::draw(self, t, buf),
            SceneId::S2 => ascent::draw(self, t, buf),
            SceneId::S3 => onboard::draw(self, t, buf),
            SceneId::S4 => high_altitude::draw(self, t, buf),
            SceneId::S5 => hot_staging::draw(self, t, buf),
            SceneId::S6 => coast::draw(self, t, buf),
            SceneId::S9 => over_earth::draw(self, t, buf),
            SceneId::S10 => seco::draw(self, t, buf),
            _ => draw_placeholder(buf),
        }
    }

    /// Scene 1's frame at t = 0: the loop fade's target (no particles).
    pub fn draw_loop_target(&self, buf: &mut Pixbuf) {
        if self.stress {
            draw_stress(self, 0.0, buf);
            return;
        }
        pad::draw_static(self, 0.0, buf);
    }

    /// Spawn from `spec` at (x, y) H for one step of `dt` seconds at `rate`
    /// particles per second (fractional counts carry over in `slot`).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(&mut self, rng: &mut Rng, slot: Slot, spec: &EmitSpec, x: f64, y: f64, owner: SceneId, dt: f64, rate: f64) {
        let n = super::particles::due(&mut self.acc[slot as usize], rate, dt);
        self.pool.spawn(rng, spec, x, y, owner.tag(), n);
    }

    /// Spawn from `spec` at uniformly random points of the normalised
    /// rectangle `us × vs`. The position is drawn for every due spawn, also
    /// when the pool is full, so the RNG stream never depends on the load.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_area(
        &mut self,
        rng: &mut Rng,
        slot: Slot,
        spec: &EmitSpec,
        us: (f64, f64),
        vs: (f64, f64),
        owner: SceneId,
        dt: f64,
        rate: f64,
    ) {
        let n = super::particles::due(&mut self.acc[slot as usize], rate, dt);
        for _ in 0..n {
            let u = rng.uniform(us.0, us.1);
            let v = rng.uniform(vs.0, vs.1);
            self.pool.spawn(rng, spec, u * self.aspect, v, owner.tag(), 1);
        }
    }
}

pub fn draw_placeholder(buf: &mut Pixbuf) {
    let p = super::palette::PLACEHOLDER;
    buf.fill([p[0] as f32, p[1] as f32, p[2] as f32]);
}

/// Stress layer: keep the pool full of short-lived random particles.
fn stress_spawn(pool: &mut Pool, rng: &mut Rng) {
    use super::particles::POOL_CAPACITY;
    while pool.len() < POOL_CAPACITY {
        let c = |rng: &mut Rng| [rng.uniform(0.0, 255.0) as f32, rng.uniform(0.0, 255.0) as f32, rng.uniform(0.0, 255.0) as f32];
        let stops = [c(rng), c(rng), c(rng)];
        let spec = EmitSpec {
            life: rng.uniform(0.5, 1.5),
            speed: (0.05, 0.4),
            dir: rng.uniform(0.0, std::f64::consts::TAU),
            spread: 0.0,
            size: (1.0, rng.uniform(1.0, 5.0) as f32),
            stops,
            nstops: 3,
            drag: 0.5,
            buoy: -0.02,
            alpha: 0.8,
        };
        let (x, y) = (rng.uniform(0.0, 1.8), rng.uniform(0.0, 1.0));
        pool.spawn(rng, &spec, x, y, STRESS_OWNER, 1);
    }
}

/// Stress layer: a full-screen plasma whose colour moves every tick, plus the
/// particle pool on top.
fn draw_stress(world: &World, t: f64, buf: &mut Pixbuf) {
    let (w, h) = (buf.w, buf.h);
    let tt = t as f32;
    for y in 0..h {
        for x in 0..w {
            let (xf, yf) = (x as f32, y as f32);
            let r = 127.5 + 127.5 * (xf * 0.071 + tt * 3.1).sin();
            let g = 127.5 + 127.5 * (yf * 0.113 - tt * 2.3).cos();
            let b = 127.5 + 127.5 * ((xf + yf) * 0.053 + tt * 4.7).sin();
            buf.px[y * w + x] = [r, g, b];
        }
    }
    world.pool.draw(buf, &Camera::default(), STRESS_OWNER, 1.0);
}

/// A smoke billow: a filled, noise-edged disc shaded lit (top-left) → body →
/// dark (bottom). Centre (u, v), radius `r` in H, `alpha` overall.
#[allow(clippy::too_many_arguments)]
pub fn billow(buf: &mut Pixbuf, cam: &Camera, u: f64, v: f64, r: f64, shades: [super::raster::Rgb; 3], alpha: f32, seed: u64) {
    if r <= 0.0 || alpha <= 0.0 {
        return;
    }
    let (cx, cy) = cam.px(buf, u, v);
    let (rx, ry) = (cam.lx(buf, r), cam.ly(buf, r));
    if rx < 0.5 && ry < 0.5 {
        return;
    }
    let y0 = (cy - ry * 1.15).floor().max(0.0) as i64;
    let y1 = ((cy + ry * 1.15).ceil() as i64).min(buf.h as i64 - 1);
    for y in y0..=y1 {
        let dy = (y as f64 + 0.5 - cy) / ry;
        // Lumpy rim: radius varies per row with smooth value noise.
        let n = smooth_noise(y as f64 * 0.35, seed);
        let rr = 1.0 + 0.15 * (n - 0.5) * 2.0;
        if dy.abs() > rr {
            continue;
        }
        let half = rx * (rr * rr - dy * dy).max(0.0).sqrt();
        let xa = ((cx - half) - 0.5).ceil().max(0.0) as i64;
        let xb = (((cx + half) - 0.5).ceil() as i64).min(buf.w as i64);
        for x in xa..xb {
            let dx = (x as f64 + 0.5 - cx) / rx;
            // Light from the upper left.
            let lit = (-(dx * 0.45 + dy * 0.9) / rr).clamp(-1.0, 1.0);
            let c = if lit > 0.42 {
                shades[0]
            } else if lit > -0.45 {
                super::raster::lerp3(shades[1], shades[0], ((lit + 0.45) / 0.8 * 0.35).max(0.0) as f32)
            } else {
                super::raster::lerp3(shades[1], shades[2], (((-lit) - 0.45) / 0.55).min(1.0) as f32)
            };
            buf.blend(x, y, c, alpha);
        }
    }
}

/// 1-D smooth value noise in [0, 1).
pub fn smooth_noise(x: f64, seed: u64) -> f64 {
    let i = x.floor();
    let f = x - i;
    let a = hash01(i as i64, 0, seed);
    let b = hash01(i as i64 + 1, 0, seed);
    let s = f * f * (3.0 - 2.0 * f);
    a + (b - a) * s
}

/// Fill the screen with a vertical gradient through `stops` given in world v
/// (seen through the camera's vertical mapping), clamped at both ends.
pub fn vstops(buf: &mut Pixbuf, cam: &Camera, stops: &[(f64, super::raster::Rgb)]) {
    let w = buf.w;
    for y in 0..buf.h {
        // Invert the camera's vertical mapping for this row's centre.
        let vs = (y as f64 + 0.5) / buf.h as f64;
        let v = cam.pivot.1 + (vs - cam.pan.1 - cam.shake.1 - cam.pivot.1) / cam.zoom;
        let c = stops_at(stops, v);
        buf.px[y * w..(y + 1) * w].fill(c);
    }
}

pub fn stops_at(stops: &[(f64, super::raster::Rgb)], v: f64) -> super::raster::Rgb {
    if v <= stops[0].0 {
        return stops[0].1;
    }
    for pair in stops.windows(2) {
        let ((v0, c0), (v1, c1)) = (pair[0], pair[1]);
        if v < v1 {
            return super::raster::lerp3(c0, c1, ((v - v0) / (v1 - v0)) as f32);
        }
    }
    stops[stops.len() - 1].1
}

/// 2-D smooth value noise in [0, 1) on the integer lattice.
pub fn noise2(x: f64, y: f64, seed: u64) -> f64 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |dx: i64, dy: i64| hash01(xi as i64 + dx, yi as i64 + dy, seed);
    let top = h(0, 0) + (h(1, 0) - h(0, 0)) * sx;
    let bot = h(0, 1) + (h(1, 1) - h(0, 1)) * sx;
    top + (bot - top) * sy
}
