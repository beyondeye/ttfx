//! Scene 10: SECO (storyboard §2.8, 39–45 s). The lit hull upper left over
//! black space. At engine cutoff the plume blooms into a blue-white haze that
//! spreads to 0.60 H and then thins; in the last second faint stars show
//! where the haze is thin (a stylistic addition: the footage shows none).

use crate::utils::easing::Easing;
use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::particles::EmitSpec;
use super::super::raster::{lerp, ramp, Camera, Pixbuf, Rgb};
use super::super::timeline::{ease_between, SceneId};
use super::onboard::to_px;
use super::{space, Slot, World};

const SPACE: Rgb = hex(0x010205);
const SPACE_DEEP: Rgb = hex(0x000000);
const HULL: Rgb = hex(0x444348);
const HULL_SEAM: Rgb = hex(0x2e2e33);
const HULL_LIT: Rgb = hex(0xd5e5e9);
const LIMB: Rgb = hex(0x6f95ab);
const HAZE: [Rgb; 3] = [hex(0xd5e5e9), hex(0x565b67), hex(0x121b2d)];
const ICE: [Rgb; 2] = [hex(0xd5e5e9), hex(0x565b67)];

const END: f64 = 45.0;
/// Engine cutoff: the bloom starts here and grows for 4.6 s.
pub const BLOOM_START: f64 = 39.05;
pub const BLOOM_GROW: f64 = 4.6;
pub const BLOOM_MAX: f64 = 0.60;
const HAZE_EMIT_END: f64 = 43.7;
/// The bloom's centre: the engine end.
pub const APEX: (f64, f64) = (0.62, 0.55);
/// The hull as the reference frames show it (upper left; the storyboard's
/// "scene 5 shape" leaves too little black for the 39.6 signature).
pub const HULL_POLY: [(f64, f64); 6] = [(0.0, 0.0), (0.55, 0.0), (0.57, 0.42), (0.50, 0.52), (0.15, 0.55), (0.0, 0.62)];
/// The limb arc's ends (on the top edge) and how far it dips below it (H).
pub const LIMB_ENDS: [(f64, f64); 2] = [(0.42, 0.0), (0.60, 0.0)];
pub const LIMB_DEPTH: f64 = 0.05;
const LIMB_WIDTH: f64 = 0.015;

/// The limb arc's circle (centre in world H, radius) for a screen aspect A:
/// it passes through `LIMB_ENDS` and dips exactly `LIMB_DEPTH` at any aspect.
pub fn limb_arc(aspect: f64) -> ((f64, f64), f64) {
    space::arc_by_depth(LIMB_ENDS[0], LIMB_ENDS[1], LIMB_DEPTH, aspect, (0.0, -1.0))
}

/// The sunlit band across the hull's upper part.
const LIT_POLY: [(f64, f64); 4] = [(0.14, 0.0), (0.53, 0.0), (0.50, 0.22), (0.20, 0.28)];

/// The bloom at time t: radius (H) and overall haze alpha. The radius follows
/// `CubicBezier(0.05, 0.7, 0.1, 1)` over 4.6 s; the haze itself builds up
/// over the first 2.5 s (so the frame 0.55 s in is still mostly black), holds,
/// and thins to 0.6 by the end of the scene.
pub fn bloom(t: f64) -> (f64, f64) {
    let r = BLOOM_MAX * ease_between(t, BLOOM_START, BLOOM_START + BLOOM_GROW, Easing::CubicBezier(0.05, 0.7, 0.1, 1.0));
    let build = ease_between(t, BLOOM_START, BLOOM_START + 2.5, Easing::InOutSine);
    let thin = ease_between(t, BLOOM_START + BLOOM_GROW, END, Easing::InOutSine);
    (r, build * lerp(1.0, 0.6, thin))
}

/// Stars fade in over 44.0–44.5.
pub fn star_alpha(t: f64) -> f64 {
    ease_between(t, END - 1.0, END - 0.5, Easing::InOutSine)
}

fn haze_spec() -> EmitSpec {
    EmitSpec {
        life: 2.5,
        speed: (0.1, 0.3),
        dir: 0.0,
        spread: std::f64::consts::PI,
        size: (2.0, 4.0),
        stops: HAZE,
        nstops: 3,
        drag: 0.7,
        buoy: 0.0,
        alpha: 0.45,
    }
}

fn ice_spec() -> EmitSpec {
    EmitSpec {
        life: 2.5,
        speed: (0.05, 0.05),
        dir: 0.0,
        spread: std::f64::consts::PI,
        size: (1.0, 1.0),
        stops: [ICE[0], ICE[1], ICE[1]],
        nstops: 2,
        drag: 0.0,
        buoy: 0.0,
        alpha: 1.0,
    }
}

pub fn step(world: &mut World, rng: &mut Rng, t: f64, dt: f64) {
    let (x, y) = (APEX.0 * world.aspect, APEX.1);
    if (BLOOM_START..HAZE_EMIT_END).contains(&t) {
        world.spawn(rng, Slot::S10Haze, &haze_spec(), x, y, SceneId::S10, dt, 1200.0);
    }
    world.spawn(rng, Slot::S10Ice, &ice_spec(), x, y, SceneId::S10, dt, 40.0);
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = Camera::default();
    buf.vgradient(0.0, buf.h as f64, SPACE, SPACE_DEEP);
    // The Earth's limb: a thin arc across the top.
    let a = buf.aspect();
    let (centre, r) = limb_arc(a);
    let (lcx, lcy) = cam.px(buf, centre.0 / a, centre.1);
    buf.radial(lcx, lcy, cam.lx(buf, r), cam.ly(buf, r), LIMB, LIMB, 0.0, 0.9);
    let inner = r - LIMB_WIDTH;
    buf.ellipse(lcx, lcy, cam.lx(buf, inner), cam.ly(buf, inner), SPACE, 1.0);
    // Stars sit under the haze, so the haze dims them where it is thick.
    space::stars(buf, t, star_alpha(t) as f32, |_, _| 1.0);
    // The bloom: core → mid → edge by radius, alpha falling to 0 at the rim.
    let (r, alpha) = bloom(t);
    if r > 0.0 && alpha > 0.0 {
        let (cx, cy) = cam.px(buf, APEX.0, APEX.1);
        let (rx, ry) = (cam.lx(buf, r), cam.ly(buf, r));
        let y0 = (cy - ry).floor().max(0.0) as i64;
        let y1 = ((cy + ry).ceil() as i64).min(buf.h as i64 - 1);
        let x0 = (cx - rx).floor().max(0.0) as i64;
        let x1 = ((cx + rx).ceil() as i64).min(buf.w as i64 - 1);
        for y in y0..=y1 {
            let dy = (y as f64 + 0.5 - cy) / ry;
            for x in x0..=x1 {
                let dx = (x as f64 + 0.5 - cx) / rx;
                let rho = (dx * dx + dy * dy).sqrt();
                if rho >= 1.0 {
                    continue;
                }
                let k = (1.0 - rho) as f32;
                buf.blend(x, y, ramp(&HAZE, rho as f32), alpha as f32 * k.powf(0.8));
            }
        }
    }
    // The plume before cutoff: a small bright glow at the engine end.
    let plume = 1.0 - ease_between(t, BLOOM_START, BLOOM_START + 1.0, Easing::InOutSine);
    if plume > 0.0 {
        let (px, py) = cam.px(buf, APEX.0, APEX.1);
        buf.radial(px, py, cam.lx(buf, 0.06), cam.ly(buf, 0.06), HAZE[0], HAZE[1], plume as f32, 0.0);
    }
    world.pool.draw(buf, &cam, SceneId::S10.tag(), 1.0);
    // The hull, lit grey with a sunlit band.
    space::tiled_hull(buf, &cam, &HULL_POLY, HULL, [HULL_SEAM; 2]);
    let mut lit = [(0.0, 0.0); 4];
    to_px(buf, &cam, &LIT_POLY, &mut lit);
    let (lx0, ly0) = cam.px(buf, 0.40, 0.0);
    let (lx1, ly1) = cam.px(buf, 0.30, 0.28);
    buf.polygon_linear(&lit, lx0, ly0, lx1, ly1, HULL_LIT, HULL, 0.9);
}
