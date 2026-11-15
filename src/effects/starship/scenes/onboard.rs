//! Scene 3: onboard camera (storyboard §2.3, 16–22 s). The ship's tiled side
//! on the left, engine fire at its aft end, the Earth drifting below.

use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::particles::EmitSpec;
use super::super::raster::{lerp3, shake, Camera, Pixbuf, Rgb};
use super::super::timeline::SceneId;
use super::{noise2, Slot, World};

const SEA_BRIGHT: Rgb = hex(0x3b70a7);
const SEA_MID: Rgb = hex(0x44709b);
const DEEP_SEA: Rgb = hex(0x255687);
const LAND: Rgb = hex(0x5d6570);
const LAND_DARK: Rgb = hex(0x565b67);
const HULL: Rgb = hex(0x5d6570);
const HULL_SEAM: Rgb = hex(0x525f6e);
const HULL_SEAM_DARK: Rgb = hex(0x555c66);
const TILES_DARK: Rgb = hex(0x454448);
const TILES_SEAM: Rgb = hex(0x3a3a42);
const CAMERA_HOUSING: Rgb = hex(0x262734);
const STAINLESS: Rgb = hex(0xeaebe4);
const FIRE_CORE: Rgb = hex(0xf1e9d3);
const FIRE: Rgb = hex(0xdf9e54);

const START: f64 = 16.0;
/// The hull outline (u, v) and the engine end at its apex.
const HULL_POLY: [(f64, f64); 5] = [(0.0, 0.0), (0.45, 0.0), (0.63, 0.52), (0.30, 0.82), (0.0, 0.82)];
const APEX: (f64, f64) = (0.63, 0.52);
/// A darker band of tiles running down the hull.
const BAND_POLY: [(f64, f64); 5] = [(0.10, 0.0), (0.31, 0.0), (0.47, 0.50), (0.42, 0.64), (0.19, 0.62)];

fn exhaust_spec() -> EmitSpec {
    EmitSpec {
        life: 0.8,
        speed: (0.25, 0.25),
        dir: 35f64.to_radians(),
        spread: 20f64.to_radians(),
        size: (1.0, 3.0),
        stops: [FIRE_CORE, FIRE, hex(0x999fa8)],
        nstops: 3,
        drag: 1.5,
        buoy: 0.0,
        alpha: 1.0,
    }
}

fn debris_spec() -> EmitSpec {
    EmitSpec {
        life: 1.0,
        speed: (0.15, 0.15),
        dir: 40f64.to_radians(),
        spread: 30f64.to_radians(),
        size: (1.0, 1.0),
        stops: [STAINLESS, hex(0x879299), hex(0x879299)],
        nstops: 2,
        drag: 0.0,
        buoy: 0.0,
        alpha: 1.0,
    }
}

fn camera(world: &World, t: f64) -> Camera {
    Camera { shake: shake(0.003, 11.0, t, &world.phases[1]), ..Camera::default() }
}

pub fn step(world: &mut World, rng: &mut Rng, _t: f64, dt: f64) {
    let a = world.aspect;
    let (x, y) = (APEX.0 * a, APEX.1);
    world.spawn(rng, Slot::S3Exhaust, &exhaust_spec(), x, y, SceneId::S3, dt, 1200.0);
    world.spawn(rng, Slot::S3Debris, &debris_spec(), x, y, SceneId::S3, dt, 60.0);
}

fn to_px(buf: &Pixbuf, cam: &Camera, pts: &[(f64, f64)], out: &mut [(f64, f64)]) {
    for (o, &(u, v)) in out.iter_mut().zip(pts) {
        *o = cam.px(buf, u, v);
    }
}

/// Inside test for a convex-ish polygon in pixel space (even-odd).
fn inside(pts: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut c = false;
    for i in 0..pts.len() {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        if (p.1 > y) != (q.1 > y) && x < p.0 + (y - p.1) / (q.1 - p.1) * (q.0 - p.0) {
            c = !c;
        }
    }
    c
}

/// Whether pixel (x, y) lies on a seam of the hex tile grid (0.03 H tiles).
fn on_seam(buf: &Pixbuf, x: f64, y: f64) -> bool {
    let size = 0.03 * buf.hy();
    let (sx, sy) = (size * 1.0, size * 0.866);
    let row = (y / sy).floor();
    let mut best = (f64::MAX, f64::MAX);
    for dr in [-1.0, 0.0, 1.0] {
        let r = row + dr;
        let off = if (r as i64).rem_euclid(2) == 1 { sx / 2.0 } else { 0.0 };
        let col = ((x - off) / sx).round();
        for dc in [-1.0, 0.0, 1.0] {
            let (cx, cy) = ((col + dc) * sx + off, r * sy);
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            if d < best.0 {
                best = (d, best.0);
            } else if d < best.1 {
                best.1 = d;
            }
        }
    }
    // Seams are 1 px at the 137×66 reference grid; scale with the grid.
    best.1 - best.0 < 0.9 * buf.h as f64 / 66.0
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = camera(world, t);
    let a = buf.aspect();
    let (w, h) = (buf.w, buf.h);
    let drift = 0.01 * (t - START);
    let mut hull = [(0.0, 0.0); 5];
    let mut band = [(0.0, 0.0); 5];
    to_px(buf, &cam, &HULL_POLY, &mut hull);
    to_px(buf, &cam, &BAND_POLY, &mut band);
    for y in 0..h {
        for x in 0..w {
            let (xc, yc) = (x as f64 + 0.5, y as f64 + 0.5);
            // Earth: positions in drifting ground coordinates.
            let (u, v) = (xc / w as f64 - drift / a, yc / h as f64 - drift);
            let n = noise2(u * 9.0, v * 6.0, 707);
            let boundary = 0.85 - 0.40 * (v / 0.82) + 0.05 * (noise2(v * 5.0, 0.0, 808) - 0.5);
            let mut c = if u < boundary {
                lerp3(LAND, LAND_DARK, n as f32)
            } else {
                // Mostly open sea; deep water only in the far upper right.
                let deep = ((u - 0.80) * 4.0 - v * 3.0 + (n - 0.5)).clamp(0.0, 1.0) as f32;
                let sea = lerp3(SEA_BRIGHT, SEA_MID, n as f32);
                lerp3(sea, DEEP_SEA, deep)
            };
            if yc / h as f64 > 0.86 {
                c = CAMERA_HOUSING;
            }
            if inside(&hull, xc, yc) {
                let dark = inside(&band, xc, yc);
                let seam = on_seam(buf, xc - cam.shake.0 * buf.hx(), yc - cam.shake.1 * buf.hy());
                c = match (dark, seam) {
                    (false, false) => HULL,
                    (false, true) if (x + y) % 2 == 0 => HULL_SEAM,
                    (false, true) => HULL_SEAM_DARK,
                    (true, false) => TILES_DARK,
                    (true, true) => TILES_SEAM,
                };
            }
            buf.px[y * w + x] = c;
        }
    }
    // The stainless flap edge along the hull's upper-right side, 0.04 H wide.
    let inset = 0.07 / a;
    let edge = [(0.45, 0.0), APEX, (APEX.0 - inset, APEX.1 - 0.02), (0.45 - inset, 0.0)];
    let mut e = [(0.0, 0.0); 4];
    to_px(buf, &cam, &edge, &mut e);
    // A sheen across the band: bright at the outer edge, duller inside.
    let (ox, oy) = cam.px(buf, 0.45, 0.0);
    let (ix, iy) = cam.px(buf, 0.45 - inset, 0.0);
    buf.polygon_linear(&e, ox, oy, ix, iy, STAINLESS, hex(0xc9d2d9), 1.0);
    // Engine fire at the apex, pulsing ±20 % at 3 Hz.
    let r = 0.08 * (1.0 + 0.2 * (std::f64::consts::TAU * 3.0 * (t - START)).sin());
    let (fx, fy) = cam.px(buf, APEX.0, APEX.1);
    buf.radial(fx, fy, cam.lx(buf, r), cam.ly(buf, r), FIRE_CORE, FIRE, 1.0, 0.85);
    world.pool.draw(buf, &cam, SceneId::S3.tag(), 1.0);
    // The exposure settles over the first 0.5 s.
    if t < START + 0.5 {
        let k = (0.55 + 0.45 * ((t - START) / 0.5)) as f32;
        for p in buf.px.iter_mut() {
            for c in p.iter_mut() {
                *c *= k;
            }
        }
    }
}
