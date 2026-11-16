//! Scene 5: hot staging (storyboard §2.5, 25–30 s). The onboard view of the
//! ship's side over the Earth's limb. At 25.95 a blue-white vapour burst
//! erupts at the engine end; from 27.4 the booster falls away and leaves the
//! frame through the bottom edge by 29.8.

use crate::utils::easing::Easing;
use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::particles::EmitSpec;
use super::super::raster::{shake, Camera, Pixbuf, Rgb};
use super::super::timeline::{ease_between, SceneId};
use super::onboard::to_px;
use super::{space, Slot, World};

const SKY: Rgb = hex(0x094287);
const SKY_HIGH: Rgb = hex(0x094287);
const BAND: [Rgb; 3] = [hex(0x4887ce), hex(0x5a8bbf), hex(0x094287)];
const SPACE: Rgb = hex(0x010205);
/// The lit lower hull: lighter steel than the storyboard's `#1f2636`, which
/// the reference shows only in the half-lit tiles next to the shadow.
const HULL: Rgb = hex(0x414550);
const HULL_SEAM: Rgb = hex(0x2a2f3f);
const SHADOW: Rgb = hex(0x030509);
const SHADOW_EDGE: Rgb = hex(0x1f2636);
const SHEEN: Rgb = hex(0xeaebe4);
const VAPOUR_CORE: Rgb = hex(0xfafcfb);
const VAPOUR: Rgb = hex(0xc9d2d9);
const VAPOUR_EDGE: Rgb = hex(0x5a8bbf);
const GLARE_CORE: Rgb = hex(0xfafcfb);
const BOOSTER: Rgb = hex(0x1f2636);
const ENGINE_GLOW: Rgb = hex(0xe7bb68);
const FLAKE: Rgb = hex(0xcfdce6);

pub const START: f64 = 25.0;
/// The staging burst starts here and grows for 0.6 s.
pub const BURST: f64 = 25.95;
const BURST_GROW: f64 = 0.6;
const BURST_FADE: f64 = 1.5;
/// The booster appears below the engine end here and is gone by `BOOSTER_GONE`.
pub const BOOSTER_START: f64 = 27.4;
pub const BOOSTER_GONE: f64 = 29.8;

/// The hull, extended past the measurement region to the bottom edge.
pub const HULL_POLY: [(f64, f64); 5] = [(0.0, 0.0), (0.40, 0.0), (0.66, 0.45), (-0.03, 1.01), (0.0, 1.01)];
/// The engine end of the ship (the burst's centre).
pub const APEX: (f64, f64) = (0.66, 0.45);
/// The hull's upper part, in shadow.
pub(super) const SHADOW_POLY: [(f64, f64); 5] = [(0.0, 0.0), (0.33, 0.0), (0.50, 0.30), (0.42, 0.44), (0.0, 0.48)];
/// The dark aft-flap block.
pub(super) const FLAP: (f64, f64, f64, f64) = (0.08, 0.45, 0.22, 0.70);

/// The Earth's limb shared with scene 6: a circle through two points, centre
/// below-right, radius 2.5 × the screen width (storyboard §2.6).
pub fn limb_circle(aspect: f64, p: (f64, f64), q: (f64, f64)) -> ((f64, f64), f64) {
    space::circle_through(p, q, 2.5 * aspect, aspect, (0.3, 1.0))
}

pub(super) const LIMB_P: (f64, f64) = (0.42, 0.98);
pub(super) const LIMB_Q: (f64, f64) = (1.00, 0.86);

/// The falling booster at time t.
#[derive(Debug, Clone, Copy)]
pub struct Booster {
    /// Body corners (u, v): top-left, top-right, bottom-right, bottom-left
    /// in the body's frame; the "top" end is the one nearest the ship.
    pub corners: [(f64, f64); 4],
    /// The engine end (u, v), where the glow and sparks sit.
    pub engine: (f64, f64),
    /// Rotation so far (radians, anticlockwise on screen).
    pub rotation: f64,
    /// Body scale (it recedes): 1 → 0.6.
    pub scale: f64,
    /// Engine glow, 1 → 0 over the last 0.6 s.
    pub glow: f64,
}

const BOOSTER_LEN: f64 = 0.30;
const BOOSTER_WIDTH: f64 = 0.04;
/// Where the booster's top end starts, just below the engine end.
const BOOSTER_TOP: (f64, f64) = (0.66, 0.47);
/// Fall direction (world, y down): mostly down, a little left.
const FALL_DIR: f64 = 115.0;
/// Fall speed: `InQuad` from 0.04 H/s to this over the fall. The storyboard's
/// by-eye 0.20 H/s cannot carry the body out of the frame by 29.8.
const FALL_V0: f64 = 0.04;
const FALL_V1: f64 = 0.75;

/// The booster at time t (`None` before it appears and once it has gone),
/// for an aspect A = screen width / height.
pub fn booster(t: f64, aspect: f64) -> Option<Booster> {
    if !(BOOSTER_START..BOOSTER_GONE).contains(&t) {
        return None;
    }
    let d = BOOSTER_GONE - BOOSTER_START;
    let tau = t - BOOSTER_START;
    // s = ∫ v0 + (v1 − v0)(τ/d)² dτ
    let dist = |tau: f64| FALL_V0 * tau + (FALL_V1 - FALL_V0) * tau.powi(3) / (3.0 * d * d);
    let s = dist(tau);
    // Rotation and shrinking follow the distance fallen, not the clock, so
    // the receding body never appears to move back up while it falls slowly.
    let p = s / dist(d);
    let fall = FALL_DIR.to_radians();
    let top = (BOOSTER_TOP.0 * aspect + s * fall.cos(), BOOSTER_TOP.1 + s * fall.sin());
    let rotation = 10f64.to_radians() * p;
    let axis = fall - rotation;
    let scale = 1.0 - 0.4 * p;
    let (ax, ay) = (axis.cos(), axis.sin());
    let (nx, ny) = (-ay, ax);
    let (l, hw) = (BOOSTER_LEN * scale, BOOSTER_WIDTH * scale / 2.0);
    let end = (top.0 + ax * l, top.1 + ay * l);
    let uv = |p: (f64, f64)| (p.0 / aspect, p.1);
    let corners = [
        uv((top.0 + nx * hw, top.1 + ny * hw)),
        uv((top.0 - nx * hw, top.1 - ny * hw)),
        uv((end.0 - nx * hw, end.1 - ny * hw)),
        uv((end.0 + nx * hw, end.1 + ny * hw)),
    ];
    let glow = 1.0 - ease_between(t, BOOSTER_GONE - 0.6, BOOSTER_GONE, Easing::InOutSine);
    Some(Booster { corners, engine: uv(end), rotation, scale, glow })
}

/// The burst's radius (H) and alpha at time t.
pub fn burst(t: f64) -> (f64, f64) {
    if t < BURST {
        return (0.0, 0.0);
    }
    let r = 0.25 * ease_between(t, BURST, BURST + BURST_GROW, Easing::OutCubic);
    // It flashes in over 0.12 s rather than popping in at full opacity.
    let flash = ease_between(t, BURST, BURST + 0.12, Easing::OutQuad);
    let a = flash * (1.0 - ease_between(t, BURST + BURST_GROW, BURST + BURST_GROW + BURST_FADE, Easing::InOutSine));
    (r, a)
}

fn burst_spec() -> EmitSpec {
    EmitSpec {
        life: 1.5,
        speed: (0.3, 0.6),
        dir: 0.0,
        spread: std::f64::consts::PI,
        size: (1.0, 3.0),
        stops: [VAPOUR_CORE, VAPOUR, VAPOUR_EDGE],
        nstops: 3,
        drag: 2.0,
        buoy: 0.0,
        alpha: 1.0,
    }
}

fn spark_spec(fall_axis: f64) -> EmitSpec {
    EmitSpec {
        life: 0.5,
        speed: (0.08, 0.08),
        dir: fall_axis + std::f64::consts::PI,
        spread: 30f64.to_radians(),
        size: (1.0, 1.0),
        stops: [ENGINE_GLOW, hex(0x8d7581), hex(0x8d7581)],
        nstops: 2,
        drag: 1.0,
        buoy: 0.0,
        alpha: 1.0,
    }
}

pub fn flake_spec(life: f64, speed: (f64, f64), stops: [Rgb; 2]) -> EmitSpec {
    EmitSpec {
        life,
        speed,
        dir: -30f64.to_radians(),
        spread: 0.0,
        size: (1.0, 1.0),
        stops: [stops[0], stops[1], stops[1]],
        nstops: 2,
        drag: 0.0,
        buoy: 0.0,
        // Hundreds of flakes are live at once (storyboard peaks 400/750):
        // at full opacity they read as a snowstorm.
        alpha: 0.45,
    }
}

fn camera(world: &World, t: f64) -> Camera {
    // 0.003 H at rest, 0.012 H during the burst, with smooth ramps so the
    // frame never jumps.
    let up = ease_between(t, BURST, BURST + 0.05, Easing::InOutSine);
    let down = ease_between(t, 26.5, 26.6, Easing::InOutSine);
    let amp = 0.003 + 0.009 * (up - down).max(0.0);
    Camera { shake: shake(amp, 9.0, t, &world.phases[2]), ..Camera::default() }
}

pub fn step(world: &mut World, rng: &mut Rng, t: f64, dt: f64) {
    let a = world.aspect;
    if (BURST..BURST + BURST_GROW).contains(&t) {
        world.spawn(rng, Slot::S5Burst, &burst_spec(), APEX.0 * a, APEX.1, SceneId::S5, dt, 2500.0 / BURST_GROW);
    }
    if let Some(b) = booster(t, a) {
        let axis = FALL_DIR.to_radians() - b.rotation;
        world.spawn(rng, Slot::S5Sparks, &spark_spec(axis), b.engine.0 * a, b.engine.1, SceneId::S5, dt, 200.0);
    }
    let flakes = flake_spec(4.0, (0.03, 0.03), [FLAKE, VAPOUR_EDGE]);
    world.spawn_area(rng, Slot::S5Flakes, &flakes, (0.55, 1.0), (0.0, 0.55), SceneId::S5, dt, 100.0);
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    draw_opts(world, t, buf, true);
}

/// Draw the scene; `with_booster: false` leaves out exactly the booster's
/// body and engine glow (a test hook: the difference is its footprint).
pub fn draw_opts(world: &World, t: f64, buf: &mut Pixbuf, with_booster: bool) {
    let cam = camera(world, t);
    let a = buf.aspect();
    let (centre, r) = limb_circle(a, LIMB_P, LIMB_Q);
    space::limb(
        buf,
        &cam,
        centre,
        r,
        0.32,
        0.45,
        &space::LimbColours { inside: SPACE, band: &BAND, sky_near: SKY, sky_far: SKY_HIGH },
    );
    space::glare(buf, &cam, (0.90, 0.60), 0.15, 0.35, GLARE_CORE, BAND[0], 1.0);
    world.pool.draw(buf, &cam, SceneId::S5.tag(), 1.0);
    if with_booster {
        if let Some(b) = booster(t, a) {
            draw_booster(buf, &cam, &b);
        }
    }
    // The hull: lit tiles, the upper part in shadow, the flap block.
    space::tiled_hull(buf, &cam, &HULL_POLY, HULL, [HULL_SEAM; 2]);
    let mut sh = [(0.0, 0.0); 5];
    to_px(buf, &cam, &SHADOW_POLY, &mut sh);
    let (sx0, sy0) = cam.px(buf, 0.0, 0.0);
    let (sx1, sy1) = cam.px(buf, 0.45, 0.30);
    buf.polygon_linear(&sh, sx0, sy0, sx1, sy1, SHADOW, SHADOW_EDGE, 1.0);
    let (fx0, fy0) = cam.px(buf, FLAP.0, FLAP.1);
    let (fx1, fy1) = cam.px(buf, FLAP.2, FLAP.3);
    buf.rect(fx0, fy0, fx1, fy1, SHADOW, 0.9);
    // A dull sheen along the hull's upper-right edge.
    let inset = 0.05 / a;
    let edge = [(0.40, 0.0), APEX, (APEX.0 - inset, APEX.1 + 0.01), (0.40 - inset, 0.0)];
    let mut e = [(0.0, 0.0); 4];
    to_px(buf, &cam, &edge, &mut e);
    let (ox, oy) = cam.px(buf, 0.40, 0.0);
    let (ix, iy) = cam.px(buf, 0.40 - inset, 0.0);
    buf.polygon_linear(&e, ox, oy, ix, iy, SHEEN, HULL, 1.0);
    // The staging burst over everything at the engine end.
    let (br, ba) = burst(t);
    if br > 0.0 && ba > 0.0 {
        let (cx, cy) = cam.px(buf, APEX.0, APEX.1);
        let (rx, ry) = (cam.lx(buf, br), cam.ly(buf, br));
        buf.radial(cx, cy, rx, ry, VAPOUR, VAPOUR_EDGE, ba as f32, 0.0);
        buf.radial(cx, cy, rx * 0.45, ry * 0.45, VAPOUR_CORE, VAPOUR, ba as f32, 0.3 * ba as f32);
    }
}

/// The booster body (exactly its corners) and its engine glow.
fn draw_booster(buf: &mut Pixbuf, cam: &Camera, b: &Booster) {
    let mut pts = [(0.0, 0.0); 4];
    to_px(buf, cam, &b.corners, &mut pts);
    buf.polygon(&pts, BOOSTER, 1.0);
    if b.glow > 0.0 {
        let (gx, gy) = cam.px(buf, b.engine.0, b.engine.1);
        let r = 0.03 * b.scale;
        buf.radial(gx, gy, cam.lx(buf, r), cam.ly(buf, r), ENGINE_GLOW, ENGINE_GLOW, b.glow as f32, 0.0);
    }
}
