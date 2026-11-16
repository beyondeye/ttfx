//! Scene 11: return and catch (storyboard §2.9). 11a wide (45–47 s), 11b
//! descent and catch (47–51.5), 11c top-down (51.5–54), 11d wide caught
//! (54–58), each separated by a hard cut; 11d runs on into the branding.

use crate::utils::easing::Easing;
use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::particles::EmitSpec;
use super::super::raster::{lerp3, shake, Camera, Pixbuf, Rgb};
use super::super::timeline::{ease_between, SceneId};
use super::{branding, noise2, vstops, Slot, World};

const LIGHT_CLOUD: Rgb = hex(0xc3b8ac);
const SKY: Rgb = hex(0xaaa197);
const SKY_LOW: Rgb = hex(0xa8a69e);
const SKY_DARK: Rgb = hex(0x8f8c85);
const SKY_11B: Rgb = hex(0xa6a29a);
const MID_GREY: Rgb = hex(0x9f9c94);
const GROUND_HI: Rgb = hex(0x74695c);
const GROUND_MID: Rgb = hex(0x5d5448);
const GROUND_LO: Rgb = hex(0x463f33);
const LATTICE: Rgb = hex(0x2d2c22);
const LATTICE_LIT: Rgb = hex(0x37352b);
const DARK_STEEL: Rgb = hex(0x2f2e2b);
const STEEL: Rgb = hex(0x9a8d7c);
const STEEL_LIT: Rgb = hex(0xc7bbab);
const FLAME: Rgb = hex(0xe2af7b);
const FLAME_CORE: Rgb = hex(0xf3d592);
const SAND: Rgb = hex(0x9a8d7c);
const SAND_DARK: Rgb = hex(0x9e9786);
const WATER: Rgb = hex(0x7b8894);
const SKY_11D: Rgb = hex(0xa3a39b);
const SEA_11D: Rgb = hex(0x9f9c94);
const GROUND_11D: Rgb = hex(0x56544a);
const MARSH_11D: Rgb = hex(0x37352b);

/// 11b: the ship's centre (u, v), length and width (H).
const SHIP_11B: (f64, f64) = (0.50, 0.25);
const SHIP_LEN: f64 = 0.35;
const SHIP_W: f64 = 0.35 / 5.8;
/// 11b: the chopstick arms (H) and the tower width (H).
const ARM_LEN: f64 = 0.20;
const TOWER_W: f64 = 0.12;
/// 11b: tower top schedule (v) and the settle.
const TOWER_START: f64 = 1.10;
const TOWER_REST: f64 = 0.30;
const SETTLE_FROM: f64 = 49.0;
const SETTLE_DUR: f64 = 1.32;
const SETTLE: Easing = Easing::CubicBezier(0.3, 0.866, 0.0, 1.078);

/// 11b: the tower top at 49.0, where the settle starts. It makes the speed
/// continuous: the linear rise (1.10 → v over 2 s) matches the settle's
/// initial speed k·(v − 0.30)/1.32, with k the eased curve's initial slope
/// (0.866/0.3 analytically). It is sampled at x = 1e-3: below 1e-4 ttfx's
/// solver switches to a linear approximation with a lower slope.
fn tower_at_settle() -> f64 {
    let e = 1e-3;
    let k = SETTLE.ease(e) / e;
    let lin = 1.0 / (SETTLE_FROM - 47.0);
    let set = k / SETTLE_DUR;
    (TOWER_START * lin + TOWER_REST * set) / (lin + set)
}

/// 11b: the tower top (v) at time t. It rises against the tracking camera
/// at a constant speed, then settles with the storyboard's bezier (with a
/// slight overshoot) to rest at 0.30 from 50.32 s.
pub fn tower_top_11b(t: f64) -> f64 {
    let at_settle = tower_at_settle();
    if t <= 47.0 {
        TOWER_START
    } else if t < SETTLE_FROM {
        TOWER_START + (at_settle - TOWER_START) * (t - 47.0) / (SETTLE_FROM - 47.0)
    } else if t < SETTLE_FROM + SETTLE_DUR {
        at_settle + (TOWER_REST - at_settle) * SETTLE.ease((t - SETTLE_FROM) / SETTLE_DUR)
    } else {
        TOWER_REST
    }
}

/// 11b: the arms' angle from horizontal (radians): 35° open, closing to 5°
/// over 50.3–51.3 with `OutCubic`.
pub fn arm_angle(t: f64) -> f64 {
    (35.0 - 30.0 * ease_between(t, 50.3, 51.3, Easing::OutCubic)).to_radians()
}

/// 11b: the ship's extent in world H (x0, x1, y0, y1), x = u·A.
pub fn ship_11b(aspect: f64) -> (f64, f64, f64, f64) {
    let (cx, cy) = (SHIP_11B.0 * aspect, SHIP_11B.1);
    (cx - SHIP_W / 2.0, cx + SHIP_W / 2.0, cy - SHIP_LEN / 2.0, cy + SHIP_LEN / 2.0)
}

/// 11b: the tower's front (left) edge in world H: the closed arms' tips
/// (5°) touch the ship's near side.
fn tower_front(aspect: f64) -> f64 {
    ship_11b(aspect).1 + ARM_LEN * 5f64.to_radians().cos()
}

/// 11b: both arms as (pivot, tip) in world H at time t. They pivot at the
/// tower's top-front and point at the ship, one above and one below.
pub fn arms(t: f64, aspect: f64) -> [((f64, f64), (f64, f64)); 2] {
    let pivot = (tower_front(aspect), tower_top_11b(t) + 0.02);
    let a = arm_angle(t);
    let tip = |side: f64| (pivot.0 - ARM_LEN * a.cos(), pivot.1 + side * ARM_LEN * a.sin());
    [(pivot, tip(-1.0)), (pivot, tip(1.0))]
}

#[allow(clippy::too_many_arguments)]
fn spec(life: f64, speed: (f64, f64), dir_deg: f64, spread_deg: f64, size: (f32, f32), stops: [Rgb; 2], drag: f64, buoy: f64) -> EmitSpec {
    EmitSpec {
        life,
        speed,
        dir: dir_deg.to_radians(),
        spread: spread_deg.to_radians(),
        size,
        stops: [stops[0], stops[1], stops[1]],
        nstops: 2,
        drag,
        buoy,
        alpha: 1.0,
    }
}

fn ship_11a(t: f64) -> (f64, f64) {
    (0.52, 0.22 + 0.02 * (t - 45.0))
}

/// 11d: the tower base, where the steam rises.
pub const BASE_11D: (f64, f64) = (0.67, 0.75);

pub fn step(world: &mut World, rng: &mut Rng, t: f64, dt: f64) {
    let a = world.aspect;
    let owner = SceneId::S11;
    if t < 47.0 {
        let (u, v) = ship_11a(t);
        let s = spec(0.5, (0.10, 0.10), 90.0, 0.0, (1.0, 1.0), [FLAME_CORE, FLAME], 2.0, 0.0);
        world.spawn(rng, Slot::S11Exhaust, &s, u * a, v + 0.02, owner, dt, 200.0);
    } else if t < 51.5 {
        let (x0, x1, _, y1) = ship_11b(a);
        let flame = spec(0.4, (0.3, 0.3), 90.0, 15.0, (1.0, 2.0), [FLAME_CORE, FLAME], 3.0, 0.0);
        world.spawn(rng, Slot::S11Flame, &flame, (x0 + x1) / 2.0, y1 + 0.02, owner, dt, 800.0);
        let top = tower_top_11b(t);
        let front = tower_front(a);
        let mid = (top + 1.0) / 2.0;
        if mid < 0.95 {
            let steam = spec(3.0, (0.05, 0.05), -90.0, 10.0, (2.0, 4.0), [hex(0xc4bcb0), hex(0xafaaa1)], 0.5, -0.02);
            let us = (front / a, (front + TOWER_W) / a);
            world.spawn_area(rng, Slot::S11Steam, &steam, us, (mid, 1.0), owner, dt, 600.0);
        }
    } else if t < 54.0 {
        let steam = spec(3.0, (0.03, 0.03), 0.0, 180.0, (2.0, 4.0), [hex(0xc8bcad), hex(0x9e9786)], 0.5, 0.0);
        world.spawn(rng, Slot::S11SteamTop, &steam, 0.50 * a, 0.45, owner, dt, 300.0);
    } else {
        let steam = spec(4.0, (0.04, 0.04), -90.0, 20.0, (2.0, 5.0), [hex(0xc7c0b4), hex(0xafaca3)], 0.4, -0.02);
        world.spawn(rng, Slot::S11SteamBase, &steam, BASE_11D.0 * a, BASE_11D.1, owner, dt, 300.0);
    }
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    if t < 47.0 {
        draw_11a(world, t, buf);
    } else if t < 51.5 {
        draw_11b(world, t, buf);
    } else if t < 54.0 {
        draw_11c(world, t, buf);
    } else {
        draw_11d(world, t, buf);
    }
}

/// A lattice tower between pixel columns x0..x1 from y_top down to y_bot:
/// a dark fill, two rails and X braces per bay.
#[allow(clippy::too_many_arguments)]
fn lattice(buf: &mut Pixbuf, x0: f64, x1: f64, y_top: f64, y_bot: f64, bay: f64, fill_a: f32, dark: Rgb, lit: Rgb) {
    if y_bot <= y_top || x1 <= x0 {
        return;
    }
    buf.rect(x0, y_top, x1, y_bot, dark, fill_a);
    let w = ((x1 - x0) * 0.12).max(1.0);
    buf.rect(x0, y_top, x0 + w, y_bot, dark, 1.0);
    buf.rect(x1 - w, y_top, x1, y_bot, dark, 1.0);
    let bay = bay.max(2.0);
    let mut y = y_top;
    while y < y_bot {
        let yb = (y + bay).min(y_bot);
        buf.line(x0, y, x1, yb, 1.0, lit, 0.9);
        buf.line(x1, y, x0, yb, 1.0, dark, 0.9);
        buf.rect(x0, y, x1, y + 1.0, dark, 1.0);
        y += bay;
    }
}

fn draw_11a(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = Camera::default();
    vstops(buf, &cam, &[(0.0, LIGHT_CLOUD), (0.16, LIGHT_CLOUD), (0.30, SKY), (0.48, SKY_LOW), (0.50, GROUND_HI), (0.62, GROUND_MID), (0.82, GROUND_LO)]);
    let (w, h) = (buf.w, buf.h);
    for y in 0..h {
        let v = (y as f64 + 0.5) / h as f64;
        for x in 0..w {
            let u = (x as f64 + 0.5) / w as f64;
            let p = &mut buf.px[y * w + x];
            if v < 0.50 {
                // Darker cloud masses under the overcast.
                let drift = 0.012 * (t - 45.0);
                let n = noise2((u + drift) * 9.0, v * 14.0, 1101) * 0.7 + noise2((u + drift) * 23.0, v * 31.0, 1102) * 0.3;
                if n > 0.62 && v > 0.12 {
                    *p = lerp3(*p, SKY_DARK, (((n - 0.62) / 0.12).min(1.0)) as f32);
                }
            } else {
                // Flat ground in bands of dry and wet marsh.
                let n = noise2(u * 6.0, v * 40.0, 1103);
                let c = if n < 0.48 { GROUND_HI } else if n < 0.58 { GROUND_MID } else { GROUND_LO };
                *p = lerp3(*p, c, 0.75);
            }
        }
    }
    for (u, height) in [(0.48, 0.22), (0.65, 0.20)] {
        let (xc, yb) = cam.px(buf, u, 0.50);
        let (_, yt) = cam.px(buf, u, 0.50 - height);
        let hw = (cam.lx(buf, 0.025) / 2.0).max(0.6);
        lattice(buf, xc - hw, xc + hw, yt, yb, cam.ly(buf, 0.03), 0.9, LATTICE, LATTICE_LIT);
    }
    let (su, sv) = ship_11a(t);
    let (x, y) = cam.px(buf, su, sv);
    buf.line(x, y - cam.ly(buf, 0.02), x, y + cam.ly(buf, 0.02), cam.lx(buf, 0.008).max(1.0), DARK_STEEL, 1.0);
    world.pool.draw(buf, &cam, SceneId::S11.tag(), 1.0);
}

fn draw_ship_side(buf: &mut Pixbuf, cam: &Camera, aspect: f64, flame: bool) {
    let (x0, x1, y0, y1) = ship_11b(aspect);
    let (px0, py0) = cam.px(buf, x0 / aspect, y0);
    let (px1, py1) = cam.px(buf, x1 / aspect, y1);
    let nose = cam.ly(buf, 0.04);
    buf.polygon(&[(px0, py1), (px0, py0 + nose), ((px0 + px1) / 2.0, py0), (px1, py0 + nose), (px1, py1)], STEEL, 1.0);
    let lit = (px1 - px0) * 0.35;
    buf.polygon(&[(px0, py1), (px0, py0 + nose), ((px0 + px1) / 2.0, py0), (px0 + lit, py0 + nose), (px0 + lit, py1)], STEEL_LIT, 1.0);
    // Forward and aft flaps on both sides.
    let fw = cam.lx(buf, 0.025);
    for (yy, hh) in [(py0 + nose * 1.2, cam.ly(buf, 0.05)), (py1 - cam.ly(buf, 0.08), cam.ly(buf, 0.07))] {
        buf.rect(px0 - fw, yy, px0, yy + hh, DARK_STEEL, 1.0);
        buf.rect(px1, yy, px1 + fw, yy + hh, DARK_STEEL, 1.0);
    }
    if flame {
        let (cx, _) = cam.px(buf, (x0 + x1) / 2.0 / aspect, y1);
        buf.radial(cx, py1 + cam.ly(buf, 0.025), (px1 - px0) * 0.4, cam.ly(buf, 0.025), FLAME_CORE, FLAME, 1.0, 0.7);
    }
}

fn draw_11b(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = Camera::default();
    let a = buf.aspect();
    let top = tower_top_11b(t);
    // The horizon rises with the tower as the camera comes down; below it
    // the dark flats and the sea.
    let ground = top + 0.25;
    vstops(buf, &cam, &[(0.0, SKY_LOW), (0.30, SKY_11B), (0.50, MID_GREY), (ground - 0.001, MID_GREY), (ground, hex(0x56544a)), (1.2, LATTICE_LIT)]);
    let (w, h) = (buf.w, buf.h);
    for y in 0..h {
        let v = (y as f64 + 0.5) / h as f64;
        for x in 0..w {
            let u = (x as f64 + 0.5) / w as f64;
            let p = &mut buf.px[y * w + x];
            if v < ground {
                // A darker cloud deck low in the sky, lighter breaks above.
                let n = noise2(u * 5.0 + t * 0.05, v * 9.0, 1201);
                let deck = ((v - 0.18) / 0.25).clamp(0.0, 1.0);
                if n * deck > 0.38 {
                    *p = lerp3(*p, hex(0x7a7770), (((n * deck) - 0.38) / 0.08).min(1.0) as f32);
                } else if n > 0.7 && v < 0.3 {
                    *p = lerp3(*p, hex(0xb2a99e), ((n - 0.7) / 0.1).min(1.0) as f32);
                }
            } else if noise2(u * 9.0, v * 24.0, 1202) > 0.5 {
                *p = LATTICE_LIT;
            }
        }
    }
    world.pool.draw(buf, &cam, SceneId::S11.tag(), 1.0);
    // The tower: lattice from its top down past the frame.
    let front = tower_front(a);
    let (x0, yt) = cam.px(buf, front / a, top);
    let (x1, _) = cam.px(buf, (front + TOWER_W) / a, top);
    lattice(buf, x0, x1, yt, h as f64 + 1.0, cam.ly(buf, 0.05), 0.55, LATTICE, LATTICE_LIT);
    draw_ship_side(buf, &cam, a, true);
    let arm_w = cam.ly(buf, 0.022).max(1.0);
    for (p, q) in arms(t, a) {
        let (px, py) = cam.px(buf, p.0 / a, p.1);
        let (qx, qy) = cam.px(buf, q.0 / a, q.1);
        buf.line(px, py, qx, qy, arm_w, LATTICE, 1.0);
        buf.line(px, py - arm_w * 0.3, qx, qy - arm_w * 0.3, 1.0, LATTICE_LIT, 1.0);
    }
}

fn draw_11c(world: &World, t: f64, buf: &mut Pixbuf) {
    let zoom = 1.0 + 0.05 * ((t - 51.5) / 2.5).clamp(0.0, 1.0);
    let cam = Camera { zoom, ..Camera::default() };
    let (w, h) = (buf.w, buf.h);
    for y in 0..h {
        let vs = (y as f64 + 0.5) / h as f64;
        let v = 0.5 + (vs - 0.5) / zoom;
        for x in 0..w {
            let us = (x as f64 + 0.5) / w as f64;
            let u = 0.5 + (us - 0.5) / zoom;
            let n = noise2(u * 7.0, v * 5.0, 1301) * 0.7 + noise2(u * 19.0, v * 13.0, 1303) * 0.3;
            let edge = 0.88 + 0.04 * (noise2(v * 6.0, 0.0, 1302) - 0.5);
            buf.px[y * w + x] = if u > edge {
                WATER
            } else if n < 0.50 {
                STEEL_LIT
            } else if n < 0.56 {
                hex(0xb2a99e)
            } else if n > 0.72 {
                SAND_DARK
            } else {
                SAND
            };
        }
    }
    world.pool.draw(buf, &cam, SceneId::S11.tag(), 1.0);
    // The arms: dark lattice diagonals from the bottom corners, meeting under the ship.
    let meet = cam.px(buf, 0.50, 0.55);
    let bw = cam.lx(buf, 0.14);
    for corner in [(-0.04, 0.95), (1.04, 0.95)] {
        let c = cam.px(buf, corner.0, corner.1);
        buf.line(c.0, c.1, meet.0, meet.1, bw, DARK_STEEL, 0.95);
        buf.line(c.0, c.1, meet.0, meet.1, 1.0, LATTICE_LIT, 1.0);
        // Cross braces along the arm.
        for k in 1..8 {
            let f = k as f64 / 8.0;
            let (mx, my) = (c.0 + (meet.0 - c.0) * f, c.1 + (meet.1 - c.1) * f);
            buf.line(mx - bw * 0.5, my - bw * 0.3, mx + bw * 0.5, my + bw * 0.3, 1.0, DARK_STEEL, 1.0);
        }
    }
    // The ship, lying in the middle, seen from above.
    let (xa, ya) = cam.px(buf, 0.36, 0.0);
    let (xb, yb) = cam.px(buf, 0.64, 0.80);
    let nose = cam.ly(buf, 0.10);
    let mid = (xa + xb) / 2.0;
    let body = [(xa, yb), (xa, ya + nose), (mid, ya - nose * 0.3), (xb, ya + nose), (xb, yb)];
    buf.polygon_with(&body, |x, _| {
        let f = ((x - xa) / (xb - xa)) as f32;
        let lit = 1.0 - (f - 0.42).abs() * 3.0;
        let base = if (f - 0.5).abs() > 0.40 { hex(0x6e6c66) } else { STEEL };
        (lerp3(base, hex(0xb2a99e), lit.clamp(0.0, 1.0)), 1.0)
    });
    for k in 1..6 {
        let x = xa + (xb - xa) * k as f64 / 6.0;
        buf.line(x, ya + nose, x, yb, 1.0, STEEL, 0.6);
    }
    let fw = cam.lx(buf, 0.06);
    let (fy0, fy1) = (ya + nose * 1.2, ya + nose * 1.2 + cam.ly(buf, 0.10));
    buf.polygon(&[(xa, fy0), (xa - fw, fy0 + 2.0), (xa - fw, fy1), (xa, fy1)], DARK_STEEL, 1.0);
    buf.polygon(&[(xb, fy0), (xb + fw, fy0 + 2.0), (xb + fw, fy1), (xb, fy1)], DARK_STEEL, 1.0);
}

/// 11d: the wide shot of the caught ship. From 57.5 the branding smoke is
/// drawn over it; the branding (B) continues this shot.
pub fn draw_11d(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = camera_11d(world, t, true);
    draw_11d_base(world, buf, &cam, true);
    branding::draw_smoke(world, t, buf, &cam);
}

/// 11d's camera; `shaken: false` is the still camera the branding plates
/// read through.
pub fn camera_11d(world: &World, t: f64, shaken: bool) -> Camera {
    let shake = if shaken { shake(0.001, 3.0, t, &world.phases[0]) } else { (0.0, 0.0) };
    Camera { shake, ..Camera::default() }
}

/// 11d without the smoke billows; its particles only when `particles`.
pub fn draw_11d_base(world: &World, buf: &mut Pixbuf, cam: &Camera, particles: bool) {
    let cam = *cam;
    vstops(buf, &cam, &[(0.0, SKY_11D), (0.29, SKY_11D), (0.30, SEA_11D), (0.40, SEA_11D), (0.401, GROUND_11D), (1.0, GROUND_11D)]);
    let (w, h) = (buf.w, buf.h);
    for y in 0..h {
        let v = (y as f64 + 0.5) / h as f64 - cam.shake.1;
        for x in 0..w {
            let u = (x as f64 + 0.5) / w as f64;
            let p = &mut buf.px[y * w + x];
            if v < 0.29 {
                let n = noise2(u * 8.0, v * 12.0, 1401);
                if n > 0.7 {
                    *p = lerp3(*p, LIGHT_CLOUD, ((n - 0.7) / 0.08).min(1.0) as f32);
                }
            } else if v > 0.40 {
                let n = noise2(u * 5.0, v * 9.0, 1402);
                if n > 0.5 {
                    *p = MARSH_11D;
                }
                // Lighter paths across the flats.
                let path = (noise2(u * 3.0, v * 20.0, 1403) - 0.5).abs();
                if path < 0.025 {
                    *p = hex(0x9e9786);
                }
            }
        }
    }
    // The distant tower and the near tower with the ship held beside it.
    for (u, top, bot, wh) in [(0.85, 0.30, 0.40, 0.012), (0.67, 0.20, 0.75, 0.06)] {
        let (xc, yt) = cam.px(buf, u, top);
        let (_, yb) = cam.px(buf, u, bot);
        let hw = (cam.lx(buf, wh) / 2.0).max(0.6);
        lattice(buf, xc - hw, xc + hw, yt, yb, cam.ly(buf, 0.04), 0.7, MARSH_11D, LATTICE);
    }
    let (sx, sy0) = cam.px(buf, 0.67 - 0.06 / buf.aspect(), 0.16);
    let (_, sy1) = cam.px(buf, 0.67, 0.36);
    let sw = cam.lx(buf, 0.03);
    buf.rect(sx - sw / 2.0, sy0, sx + sw / 2.0, sy1, STEEL, 1.0);
    buf.rect(sx - sw / 2.0, sy0, sx, sy1, STEEL_LIT, 1.0);
    let (ax, ay) = cam.px(buf, 0.67, 0.27);
    buf.line(ax, ay, sx - sw / 2.0, ay, 1.0, LATTICE, 1.0);
    if particles {
        world.pool.draw(buf, &cam, SceneId::S11.tag(), 1.0);
        world.pool.draw(buf, &cam, SceneId::B.tag(), 1.0);
    }
}
