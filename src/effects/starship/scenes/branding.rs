//! B: the branding reveal and the loop (storyboard §3, 58–64 s).
//!
//! The scene continues 11d's wide shot. Smoke billows build up over the
//! text from 57.5 s (so the density the reveal is ranked by exists at 58.0),
//! steam spawns over the text box, and the input characters are revealed in
//! order of the smoke density at their cells. The glyph cells themselves are
//! drawn at emission (`text.rs`, `Starship::emit`); this module only owns the
//! schedule and the smoke. The loop fade to scene 1 is generic (timeline.rs).

use crate::utils::easing::Easing;
use crate::utils::rng::Rng;

use super::super::palette::hex;
use super::super::particles::EmitSpec;
use super::super::raster::{Camera, Pixbuf, Rgb};
use super::super::timeline::SceneId;
use super::{billow, catch, Slot, World};

/// The text box when there are no glyph cells (§3: the default 30×5 box).
pub const DEFAULT_BOX: (f64, f64, f64, f64) = (0.39, 0.61, 0.42, 0.58);
/// Reveal starts lie in [REVEAL_START, REVEAL_START + REVEAL_SPAN].
pub const REVEAL_START: f64 = 58.0;
pub const REVEAL_SPAN: f64 = 1.13;
/// Dissolves start in [DISSOLVE_LAST − DISSOLVE_SPAN, DISSOLVE_LAST], in
/// reverse reveal order; each lasts `text::FADE_SECONDS`, so all are done by
/// 63.70.
pub const DISSOLVE_LAST: f64 = 63.37;
pub const DISSOLVE_SPAN: f64 = 0.87;
/// The billows start building over the text here (before 58.0, so the
/// reveal can be ranked by density).
pub const SMOKE_FROM: f64 = 57.5;
/// The emitters stop at the loop fade.
pub const SMOKE_STOP: f64 = 62.5;
/// After this no glyph is active (every dissolve is done by 63.70).
pub const TEXT_END: f64 = 63.75;

const SMOKE: [Rgb; 3] = [hex(0xc7c0b4), hex(0xb9b3a8), hex(0xafaca3)];

/// Billows over the text box: offset (fraction of the box), radius factor
/// (× box height), seed. Back to front.
const BILLOWS: [(f64, f64, f64, u64); 12] = [
    (0.10, 0.30, 0.55, 1501),
    (0.30, 0.20, 0.60, 1502),
    (0.55, 0.25, 0.65, 1503),
    (0.80, 0.30, 0.55, 1504),
    (0.95, 0.60, 0.50, 1505),
    (0.05, 0.75, 0.50, 1506),
    (0.25, 0.70, 0.60, 1507),
    (0.50, 0.65, 0.65, 1508),
    (0.72, 0.72, 0.60, 1509),
    (0.40, 0.45, 0.55, 1510),
    (0.65, 0.48, 0.50, 1511),
    (0.18, 0.50, 0.45, 1512),
];

/// The billows at time t: centre (u, v), radius (H) and alpha.
pub fn billows(world: &World, t: f64) -> impl Iterator<Item = (f64, f64, f64, f32, u64)> + '_ {
    let (u0, u1, v0, v1) = world.text_box;
    let box_h = (v1 - v0).max(0.08);
    let thin = 1.0 - 0.4 * ((t - 59.5) / 1.0).clamp(0.0, 1.0);
    BILLOWS.iter().enumerate().filter_map(move |(j, &(fu, fv, rk, seed))| {
        let start = SMOKE_FROM + j as f64 * 0.08;
        if t <= start {
            return None;
        }
        let g = Easing::OutCubic.ease(((t - start) / 1.2).min(1.0));
        let r = (box_h * rk).max(0.08) * g;
        let drift = 0.03 * (t - SMOKE_FROM);
        Some((u0 + (u1 - u0) * fu, v0 + (v1 - v0) * fv - drift, r, thin as f32, seed))
    })
}

/// Draw the billows (11d from 57.5, and B).
pub fn draw_smoke(world: &World, t: f64, buf: &mut Pixbuf, cam: &Camera) {
    if t < SMOKE_FROM {
        return;
    }
    for (u, v, r, a, seed) in billows(world, t) {
        billow(buf, cam, u, v, r, SMOKE, 0.85 * a, seed);
    }
}

fn smoke_spec() -> EmitSpec {
    EmitSpec {
        life: 4.0,
        speed: (0.03, 0.03),
        dir: (-90.0f64).to_radians(),
        spread: 20f64.to_radians(),
        size: (2.0, 5.0),
        stops: [hex(0xc7c0b4), hex(0xafaca3), hex(0xafaca3)],
        nstops: 2,
        drag: 0.4,
        buoy: -0.01,
        alpha: 1.0,
    }
}

pub fn step(world: &mut World, rng: &mut Rng, t: f64, dt: f64) {
    if t >= SMOKE_STOP {
        return;
    }
    let rate = 300.0 + 500.0 * ((t - REVEAL_START) / 0.5).clamp(0.0, 1.0);
    let (u0, u1, v0, v1) = world.text_box;
    world.spawn_area(rng, Slot::BSmoke, &smoke_spec(), (u0, u1), (v0, v1), SceneId::B, dt, rate);
}

pub fn draw(world: &World, t: f64, buf: &mut Pixbuf) {
    catch::draw_11d(world, t, buf);
}

/// What the glyph plates are read from: the branding scene with its smoke
/// billows but without particles or camera shake, and never mixed with the
/// loop target. It changes only as slowly as the billows drift, so a plate
/// does not flicker with a passing particle or the shake.
pub fn draw_plate_source(world: &World, t: f64, buf: &mut Pixbuf) {
    draw_plate_base(world, buf);
    draw_plate_smoke(world, t, buf);
}

/// The static part of the plate source (11d, still, no particles).
pub fn draw_plate_base(world: &World, buf: &mut Pixbuf) {
    catch::draw_11d_base(world, buf, &catch::camera_11d(world, REVEAL_START, false), false);
}

/// The moving part of the plate source: the billows over the base.
pub fn draw_plate_smoke(world: &World, t: f64, buf: &mut Pixbuf) {
    draw_smoke(world, t, buf, &catch::camera_11d(world, t, false));
}

/// Rank the glyph cells by smoke density at 58.0 (on the tick that crosses
/// it) and set their reveal and dissolve starts. One random number is drawn
/// per cell, always, to break ties.
///
/// Density at a cell: the S11/B smoke particles in the 3×3 cells around it
/// (one pass over the pool into a per-cell grid), plus 4 × the number of
/// billows covering the cell's centre.
pub fn rank(world: &mut World, rng: &mut Rng) {
    let n = world.text.len();
    if n == 0 {
        return;
    }
    let a = world.aspect;
    let (du, dv) = world.cell;
    let (cols, rows) = ((1.0 / du).round() as usize, (1.0 / dv).round() as usize);
    world.density.fill(0);
    for p in world.pool.items.iter().filter(|p| p.owner == SceneId::S11.tag() || p.owner == SceneId::B.tag()) {
        let (u, v) = (p.x / a, p.y);
        if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
            world.density[(v * rows as f64) as usize * cols + (u * cols as f64) as usize] += 1;
        }
    }
    let discs: Vec<(f64, f64, f64)> = billows(world, REVEAL_START).map(|(u, v, r, _, _)| (u, v, r)).collect();
    let mut keyed: Vec<(f64, f64, usize)> = Vec::with_capacity(n);
    for i in 0..n {
        let (u, v) = (world.text[i].u, world.text[i].v);
        let (c, r) = (((u * cols as f64) as usize).min(cols - 1), ((v * rows as f64) as usize).min(rows - 1));
        let mut count = 0u32;
        for rr in r.saturating_sub(1)..=(r + 1).min(rows - 1) {
            for cc in c.saturating_sub(1)..=(c + 1).min(cols - 1) {
                count += world.density[rr * cols + cc];
            }
        }
        let cover = discs
            .iter()
            .filter(|&&(cu, cv, rad)| {
                let (dx, dy) = ((u - cu) * a, v - cv);
                dx * dx + dy * dy <= rad * rad
            })
            .count() as f64;
        let density = count as f64 + 4.0 * cover;
        world.text[i].density = density;
        keyed.push((density, rng.random(), i));
    }
    keyed.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.total_cmp(&y.1)));
    for (rank, &(_, _, i)) in keyed.iter().enumerate() {
        let f = if n > 1 { rank as f64 / (n - 1) as f64 } else { 0.0 };
        let start = REVEAL_START + REVEAL_SPAN * Easing::OutCubic.ease(f);
        world.text[i].start = start;
        world.text[i].dissolve = DISSOLVE_LAST - (start - REVEAL_START) * DISSOLVE_SPAN / REVEAL_SPAN;
    }
}
