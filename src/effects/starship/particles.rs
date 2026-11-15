//! One particle pool of fixed capacity (storyboard §1.7).
//!
//! Positions are world coordinates in H (`x = u·A`, `y = v`, y down),
//! velocities in H/s, buoyancy in H/s² (negative = upward). The pool is
//! allocated once; a spawn when it is full is dropped, but its random numbers
//! are still drawn so `--seed` runs stay identical whatever the load.

use crate::utils::rng::Rng;

use super::raster::{ramp, Camera, Pixbuf, Rgb};

pub const POOL_CAPACITY: usize = 5_000;

#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub age: f64,
    pub life: f64,
    pub size0: f32,
    pub size1: f32,
    pub stops: [Rgb; 3],
    pub nstops: u8,
    pub drag: f64,
    pub buoy: f64,
    pub owner: u8,
    /// Extra alpha multiplier (billow edge detail is fainter).
    pub alpha: f32,
}

/// An emitter's parameters, as the storyboard's particle lines state them.
#[derive(Debug, Clone, Copy)]
/// The spawn rate is not part of it: it often varies over a shot, so the
/// caller passes it to `World::spawn` each step.
pub struct EmitSpec {
    pub life: f64,
    /// Speed range, H/s (a uniform pick, then ±20 % jitter).
    pub speed: (f64, f64),
    /// Direction in radians (0 = +x, π/2 = down) and its half-spread.
    pub dir: f64,
    pub spread: f64,
    pub size: (f32, f32),
    pub stops: [Rgb; 3],
    pub nstops: u8,
    pub drag: f64,
    pub buoy: f64,
    pub alpha: f32,
}

pub struct Pool {
    pub items: Vec<Particle>,
}

impl Default for Pool {
    fn default() -> Pool {
        Pool::new()
    }
}

impl Pool {
    pub fn new() -> Pool {
        Pool { items: Vec::with_capacity(POOL_CAPACITY) }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn kill_owner(&mut self, owner: u8) {
        self.items.retain(|p| p.owner != owner);
    }

    /// Advance every particle by `dt` seconds of storyboard time.
    pub fn step(&mut self, dt: f64) {
        let mut i = 0;
        while i < self.items.len() {
            let p = &mut self.items[i];
            p.age += dt;
            if p.age >= p.life {
                self.items.swap_remove(i);
                continue;
            }
            let k = (-p.drag * dt).exp();
            p.vx *= k;
            p.vy = p.vy * k + p.buoy * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            i += 1;
        }
    }

    /// Spawn `n` particles around (x, y) H. Every spawn draws the same six
    /// random numbers whether or not the pool has room.
    pub fn spawn(&mut self, rng: &mut Rng, spec: &EmitSpec, x: f64, y: f64, owner: u8, n: usize) {
        for _ in 0..n {
            let jx = rng.uniform(-0.01, 0.01);
            let jy = rng.uniform(-0.01, 0.01);
            let life = spec.life * rng.uniform(0.8, 1.2);
            let speed = rng.uniform(spec.speed.0, spec.speed.1) * rng.uniform(0.8, 1.2);
            let dir = spec.dir
                + rng.uniform(-spec.spread, spec.spread)
                + rng.uniform(-15f64.to_radians(), 15f64.to_radians());
            if self.items.len() >= POOL_CAPACITY {
                continue;
            }
            self.items.push(Particle {
                x: x + jx,
                y: y + jy,
                vx: speed * dir.cos(),
                vy: speed * dir.sin(),
                age: 0.0,
                life,
                size0: spec.size.0,
                size1: spec.size.1,
                stops: spec.stops,
                nstops: spec.nstops,
                drag: spec.drag,
                buoy: spec.buoy,
                owner,
                alpha: spec.alpha,
            });
        }
    }

    /// Draw this owner's particles through a camera, faded by `fade`.
    pub fn draw(&self, buf: &mut Pixbuf, cam: &Camera, owner: u8, fade: f32) {
        let a_h = buf.aspect();
        // Sizes are pixels at the 137×66 reference grid; keep them a constant
        // fraction of the screen on other grids.
        let grid = buf.h as f64 / 66.0;
        for p in self.items.iter().filter(|p| p.owner == owner) {
            let t = (p.age / p.life) as f32;
            let alpha = (1.0 - t) * (1.0 - t) * p.alpha * fade;
            if alpha <= 0.004 {
                continue;
            }
            let c = ramp(&p.stops[..p.nstops as usize], t);
            let size = (p.size0 + (p.size1 - p.size0) * t) as f64 * cam.zoom.max(0.25) * grid;
            let (px, py) = cam.px(buf, p.x / a_h, p.y);
            if size < 2.0 {
                buf.blend(px.floor() as i64, py.floor() as i64, c, alpha);
            } else {
                buf.ellipse(px, py, size / 2.0, size / 2.0, c, alpha);
            }
        }
    }
}

/// Fractional spawn accumulator: how many to spawn this step.
pub fn due(acc: &mut f64, rate: f64, dt: f64) -> usize {
    *acc += rate.max(0.0) * dt;
    let n = acc.floor();
    *acc -= n;
    n as usize
}
