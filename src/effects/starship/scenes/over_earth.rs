//! Scene 9: ship over Earth (storyboard §2.7, 35–39 s). The camera on the
//! other side of the ship: a dark plum hull upper left, the cloudy Earth
//! beyond it, the bright sun with flare spikes, and black space below right.

use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::particles::EmitSpec;
use super::super::raster::{lerp3, shake, Camera, Pixbuf, Rgb};
use super::super::timeline::SceneId;
use super::onboard::to_px;
use super::{noise2, space, Slot, World};

/// Lit plum tiles, a shade above the storyboard's `#2d2633` (the seams and
/// shadow carry the darker tones).
const HULL: Rgb = hex(0x352d3d);
const HULL_SEAMS: [Rgb; 2] = [hex(0x2d2633), hex(0x3d3446)];
const HULL_LIT: Rgb = hex(0x5a5262);
const SHADOW: Rgb = hex(0x110d16);
const ENGINE_BAY: Rgb = hex(0x424554);
const OCEAN: Rgb = hex(0x61758e);
const CLOUD: Rgb = hex(0x99abb7);
const CLOUD_BRIGHT: Rgb = hex(0xe3edef);
const RIM: Rgb = hex(0x6f95ab);
const SPACE: Rgb = hex(0x191421);
const SPACE_GLOW: Rgb = hex(0x352d3d);
const SPACE_DEEP: Rgb = hex(0x0d0e15);
const SUN: Rgb = hex(0xddedf7);
const SUN_CORE: Rgb = hex(0xe3edef);
const STEEL: Rgb = hex(0xe3edef);
const STEEL_DIM: Rgb = hex(0x99abb7);
const NAV: Rgb = hex(0x62263c);
const ICE: Rgb = hex(0xe2ecee);

const START: f64 = 35.0;

/// The hull, extended to the bottom edge.
const HULL_POLY: [(f64, f64); 5] = [(0.0, 0.0), (0.30, 0.0), (0.62, 0.55), (0.55, 1.01), (0.0, 1.01)];
/// The hull's shadowed upper-left corner.
const SHADOW_POLY: [(f64, f64); 4] = [(0.0, 0.0), (0.16, 0.0), (0.12, 0.42), (0.0, 0.46)];
const PANEL: (f64, f64, f64, f64) = (0.30, 0.0, 0.55, 0.35);
const BAY: (f64, f64, f64, f64) = (0.08, 0.30, 0.30, 0.60);
const NAV_AT: (f64, f64) = (0.02, 0.77);
pub const SUN_AT: (f64, f64) = (0.86, 0.18);

/// The Earth: a circle through its visible edge, (0.68, 0) to (0.40, 0.82)
/// (storyboard §2.7), centre to the left behind the hull. Black space and the
/// sun lie right of it.
const EDGE_P: (f64, f64) = (0.68, 0.0);
const EDGE_Q: (f64, f64) = (0.40, 0.82);
const EARTH_R: f64 = 1.5;
/// The sun's core radius (H).
pub const SUN_CORE_R: f64 = 0.04;

/// The Earth's centre (world H) and radius for a screen aspect A.
pub fn earth(aspect: f64) -> ((f64, f64), f64) {
    space::circle_through(EDGE_P, EDGE_Q, EARTH_R, aspect, (-1.0, -0.3))
}

fn ice_spec() -> EmitSpec {
    EmitSpec {
        life: 5.0,
        speed: (0.03, 0.03),
        dir: -40f64.to_radians(),
        spread: 0.0,
        size: (1.0, 1.0),
        stops: [ICE, CLOUD, CLOUD],
        nstops: 2,
        drag: 0.0,
        buoy: 0.0,
        alpha: 1.0,
    }
}

fn camera(world: &World, t: f64) -> Camera {
    Camera { shake: shake(0.002, 5.0, t, &world.phases[3]), ..Camera::default() }
}

pub fn step(world: &mut World, rng: &mut Rng, _t: f64, dt: f64) {
    world.spawn_area(rng, Slot::S9Ice, &ice_spec(), (0.72, 1.0), (0.30, 0.82), SceneId::S9, dt, 60.0);
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    let cam = camera(world, t);
    let a = buf.aspect();
    let (centre, earth_r) = earth(a);
    // The clouds scroll down-left at 0.01 H/s as the Earth turns.
    let drift = 0.01 * (t - START);
    let w = buf.w;
    for y in 0..buf.h {
        for x in 0..w {
            let (wx, wy) = space::world_of(buf, &cam, x as f64 + 0.5, y as f64 + 0.5);
            let d = ((wx - centre.0).powi(2) + (wy - centre.1).powi(2)).sqrt() - earth_r;
            let c = if d < 0.0 {
                // Cloud noise at a 0.05 H scale, threshold 0.55.
                let (nx, ny) = ((wx + drift) / 0.05, (wy - drift) / 0.05);
                let n = 0.65 * noise2(nx, ny, 909) + 0.35 * noise2(nx * 2.3, ny * 2.3, 910);
                let base = if n > 0.55 {
                    let k = ((n - 0.55) / 0.2).min(1.0) as f32;
                    lerp3(CLOUD, CLOUD_BRIGHT, k * k)
                } else {
                    OCEAN
                };
                // A thin bright rim of atmosphere at the edge.
                if d > -0.012 {
                    lerp3(base, RIM, 0.7)
                } else {
                    base
                }
            } else if d < 0.20 {
                // Space: a plum airglow along the Earth's edge…
                lerp3(SPACE_GLOW, SPACE, (d / 0.20) as f32)
            } else {
                // …deepening away from it.
                lerp3(SPACE, SPACE_DEEP, ((d - 0.20) / 0.35).clamp(0.0, 1.0) as f32)
            };
            buf.px[y * w + x] = c;
        }
    }
    // The sun with its glare and six flare spikes.
    space::glare(buf, &cam, SUN_AT, 0.12, 0.5, SUN, SUN, 1.0);
    space::spikes(buf, &cam, SUN_AT, 6, 0.30, 0.35, SUN, 0.8);
    let (sx, sy) = cam.px(buf, SUN_AT.0, SUN_AT.1);
    buf.ellipse(sx, sy, cam.lx(buf, SUN_CORE_R), cam.ly(buf, SUN_CORE_R), SUN_CORE, 1.0);
    world.pool.draw(buf, &cam, SceneId::S9.tag(), 1.0);
    // The hull: plum tiles, a shadowed corner, the steel flap panel and the
    // engine bay (both clipped to the hull), and the red navigation glow.
    space::tiled_hull(buf, &cam, &HULL_POLY, HULL, HULL_SEAMS);
    let mut hull = [(0.0, 0.0); 5];
    to_px(buf, &cam, &HULL_POLY, &mut hull);
    let (lx0, ly0) = cam.px(buf, 0.62, 0.55);
    let (lx1, ly1) = cam.px(buf, 0.35, 0.45);
    // Light from the sun catches the hull's outer edge.
    buf.polygon_with(&hull, |x, y| {
        let (dx, dy) = (lx1 - lx0, ly1 - ly0);
        let k = (((x - lx0) * dx + (y - ly0) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0) as f32;
        (HULL_LIT, 0.35 * (1.0 - k))
    });
    let mut sh = [(0.0, 0.0); 4];
    to_px(buf, &cam, &SHADOW_POLY, &mut sh);
    buf.polygon(&sh, SHADOW, 1.0);
    let panel = [(PANEL.0, PANEL.1), (PANEL.2, PANEL.1), (PANEL.2, PANEL.3), (PANEL.0, PANEL.3)];
    let (px0, py0) = cam.px(buf, PANEL.2, PANEL.1);
    let (px1, py1) = cam.px(buf, PANEL.0, PANEL.3);
    clipped_rect(buf, &cam, &panel, &hull, |x, y| {
        let (dx, dy) = (px1 - px0, py1 - py0);
        let k = (((x - px0) * dx + (y - py0) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0) as f32;
        (lerp3(STEEL, STEEL_DIM, k), 1.0)
    });
    let bay = [(BAY.0, BAY.1), (BAY.2, BAY.1), (BAY.2, BAY.3), (BAY.0, BAY.3)];
    clipped_rect(buf, &cam, &bay, &hull, |_, _| (ENGINE_BAY, 1.0));
    let (nx, ny) = cam.px(buf, NAV_AT.0, NAV_AT.1);
    buf.radial(nx, ny, cam.lx(buf, 0.03), cam.ly(buf, 0.03), NAV, NAV, 1.0, 0.0);
}

/// Fill a rectangle (normalised corners) only where it is inside `clip` (pixels).
fn clipped_rect(buf: &mut Pixbuf, cam: &Camera, rect: &[(f64, f64); 4], clip: &[(f64, f64)], shade: impl Fn(f64, f64) -> (Rgb, f32)) {
    let mut r = [(0.0, 0.0); 4];
    to_px(buf, cam, rect, &mut r);
    buf.polygon_with(&r, |x, y| {
        if super::onboard::inside(clip, x, y) {
            shade(x, y)
        } else {
            ([0.0; 3], 0.0)
        }
    });
}
