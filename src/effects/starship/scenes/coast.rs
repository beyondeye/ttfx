//! Scene 6: coast (storyboard §2.6, 30–35 s). Scene 5's framing with the
//! burst and booster gone: the deep-blue Earth beyond the hull, the bright
//! atmosphere along the limb, sun glare low right and ice flakes drifting.

use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::raster::{Camera, Pixbuf, Rgb};
use super::super::timeline::SceneId;
use super::hot_staging::{flake_spec, limb_circle, FLAP, HULL_POLY, LIMB_P, LIMB_Q};
use super::onboard::to_px;
use super::{space, Slot, World};

const SKY: Rgb = hex(0x0e448a);
const SKY_HIGH: Rgb = hex(0x0a397b);
const BAND: [Rgb; 3] = [hex(0x5897d9), hex(0x477db9), hex(0x0e448a)];
const SPACE: Rgb = hex(0x010205);
/// Lit lower hull tiles; `#222736` is the half-lit band next to the shadow.
const HULL: Rgb = hex(0x414550);
/// Seams alternate a dark and a mid tone (half of them read as the half-lit band).
const HULL_SEAMS: [Rgb; 2] = [hex(0x070b15), hex(0x2a2f3f)];
const HULL_DIM: Rgb = hex(0x222736);
const SHADOW: Rgb = hex(0x0d0f16);
const GLARE_CORE: Rgb = hex(0xb8c9d5);
const GLARE: Rgb = hex(0x8d9da6);
const FLAKE: Rgb = hex(0xfafaf7);
const GLINT_CORE: Rgb = hex(0xeaebe4);
const GLINT: (f64, f64) = (0.06, 0.68);

const START: f64 = 30.0;

/// The shadowed upper hull: smaller than in scene 5 (the sun has moved on).
const SHADOW_POLY: [(f64, f64); 5] = [(0.0, 0.0), (0.30, 0.0), (0.44, 0.26), (0.36, 0.38), (0.0, 0.42)];

pub fn step(world: &mut World, rng: &mut Rng, _t: f64, dt: f64) {
    let flakes = flake_spec(5.0, (0.02, 0.06), [FLAKE, GLARE_CORE]);
    world.spawn_area(rng, Slot::S6Flakes, &flakes, (0.55, 1.0), (0.0, 0.55), SceneId::S6, dt, 150.0);
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = Camera::default();
    let a = buf.aspect();
    let (centre, r) = limb_circle(a, LIMB_P, LIMB_Q);
    space::limb(
        buf,
        &cam,
        centre,
        r,
        0.34,
        0.40,
        &space::LimbColours { inside: SPACE, band: &BAND, sky_near: SKY, sky_far: SKY_HIGH },
    );
    // The glare breathes ±8 % at 0.3 Hz.
    let breathe = 1.0 + 0.08 * (std::f64::consts::TAU * 0.3 * (t - START)).sin();
    // A blown-out core of 0.12 H (the storyboard's radius) inside a 0.24 H
    // halo: the reference's glare is a large bright patch.
    space::glare(buf, &cam, (0.95, 0.62), 0.24 * breathe, 0.5, GLARE_CORE, GLARE, 1.0);
    world.pool.draw(buf, &cam, SceneId::S6.tag(), 1.0);
    space::tiled_hull(buf, &cam, &HULL_POLY, HULL, HULL_SEAMS);
    let mut sh = [(0.0, 0.0); 5];
    to_px(buf, &cam, &SHADOW_POLY, &mut sh);
    let (sx0, sy0) = cam.px(buf, 0.0, 0.0);
    let (sx1, sy1) = cam.px(buf, 0.45, 0.30);
    buf.polygon_linear(&sh, sx0, sy0, sx1, sy1, SHADOW, HULL_DIM, 1.0);
    let (fx0, fy0) = cam.px(buf, FLAP.0, FLAP.1);
    let (fx1, fy1) = cam.px(buf, FLAP.2, FLAP.3);
    buf.rect(fx0, fy0, fx1, fy1, SHADOW, 0.9);
    // Sunlight glinting off the hull, lower left.
    space::glare(buf, &cam, GLINT, 0.08, 0.4, GLINT_CORE, GLARE, 0.9);
}
