//! Branding glyph cells with a bounded set of appearances (storyboard §3).
//!
//! The branding reveal draws the input characters with their own symbols.
//! ttfx keeps every distinct (symbol, colours) appearance for the life of the
//! process, so the colours a glyph cell can take come from small fixed sets:
//!
//! - **F**, the text ramp (`palette::RAMPS_TEXT`, 8 entries,
//!   `#6e6c66` → `#afaca3` → `#f3efe6`). Hold is always `#f3efe6`.
//! - **G**, the plate set: a fixed order over the palette (see [`plate`]).
//!   A tier uses a prefix. The plate g is always one of its entries with
//!   luma ≤ 0.50, so the text contrast is ≥ 0.938 − 0.50 = 0.438. Each
//!   glyph cell's plate is simulation state (`World::step_plates`): it
//!   follows the still, particle-free branding scene under the cap and moves
//!   by at most one plate step (≤ 48 RGB) per tick ([`step_plate`]), and it
//!   holds still while the cell is in its `▀` blend or on its path.
//! - The approach path q_j(g) from a plate colour g towards F's low end.
//!
//! A cell walks the states of [`cell_state`]:
//! - 0 is the plain scene;
//! - 1..=B blend its two half-block pixels to the plate colour g as `▀`
//!   pairs, which are palette pairs and so cost nothing extra;
//! - after that come glyph states (q_j(g), g) and (F_t[i], g).
//!
//! A glyph therefore appears and disappears only next to a flat `▀`(g, g)
//! cell, which looks identical. The state index ([`glyph_state`]) follows the
//! storyboard-time schedule, so hold and completion keep their times at any
//! frame rate and `--duration-scale`. A tick whose schedule would jump over
//! the flat state shows it instead, and the first and last active ticks are
//! the plain scene. At 60 fps and above (scale 1) it moves by at most one
//! state per tick.
//!
//! **Supported input.** One budget, [`SYMBOL_BUDGET`] = 10 240 appearances,
//! covers both the engine's own default visual per distinct input symbol
//! (`Animation::new` interns one) and the glyph appearances. For n_d distinct
//! symbols (n of them non-space), the tier is the first of [`TIERS`] with
//! `n_d + n·cost ≤ SYMBOL_BUDGET`:
//!
//! - every symbol is revealed for up to 5 119 distinct non-space symbols, which
//!   is any input that fits a 1080p screen;
//! - T0–T2 (up to 70 symbols) have foreground steps ≤ 48 RGB per tick, and
//!   coarser tiers have larger foreground steps. The `▀` handoff is smooth in
//!   every tier.
//! - **Overflow**: past that, the k = max(0, budget − n_d) most frequent
//!   symbols (ties by first occurrence) are drawn at T6 and the rest stay
//!   scene cells. With n_d ≥ budget (a full 4K grid of distinct symbols, say)
//!   no glyph is drawn. The engine's own defaults already use the budget.
//!
//! `tests/starship_pool_ceiling.rs` measures these cases against 64 MB.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::palette::{palette, ramp_colors, rgb_of, PalIdx, RAMPS_TEXT};
use super::raster::Rgb;

/// Engine default visuals plus glyph appearances, for any input.
pub const SYMBOL_BUDGET: usize = 10_240;
/// `▀` blend states between the scene and the plate colour (B).
pub const HANDOFF_STATES: usize = 7;
/// Length of a reveal or a dissolve, storyboard seconds.
pub const FADE_SECONDS: f64 = 0.33;
/// Hold luma cap target for the plate (§3: 0.50, with margin).
pub const CAP_LUMA: f64 = 0.48;

/// One rung of the ladder: |F_t|, approach steps m, |G_t|.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tier {
    pub f: usize,
    pub m: usize,
    pub g: usize,
}

impl Tier {
    /// Glyph appearances per symbol.
    pub const fn cost(&self) -> usize {
        (self.f + self.m) * self.g
    }

    /// The last state index S (hold).
    pub const fn states(&self) -> usize {
        HANDOFF_STATES + self.m + self.f
    }
}

pub const TIERS: [Tier; 7] = [
    Tier { f: 8, m: 4, g: 48 },
    Tier { f: 8, m: 4, g: 24 },
    Tier { f: 8, m: 4, g: 12 },
    Tier { f: 6, m: 2, g: 8 },
    Tier { f: 4, m: 1, g: 4 },
    Tier { f: 2, m: 0, g: 2 },
    Tier { f: 1, m: 0, g: 1 },
];

/// The tier, and how many glyph symbols (the most frequent) are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPlan {
    pub tier: usize,
    pub k_symbols: usize,
}

/// The plan for an input with `n_distinct` distinct symbols (the engine's
/// default visuals), `n_nonspace` of them drawable glyphs.
pub fn plan_for(n_distinct: usize, n_nonspace: usize) -> TextPlan {
    for (i, t) in TIERS.iter().enumerate() {
        if n_distinct + n_nonspace * t.cost() <= SYMBOL_BUDGET {
            return TextPlan { tier: i, k_symbols: n_nonspace };
        }
    }
    let k = SYMBOL_BUDGET.saturating_sub(n_distinct).min(n_nonspace);
    TextPlan { tier: TIERS.len() - 1, k_symbols: k }
}

/// Glyph appearances the effect can create for such an input.
pub fn text_appearance_bound(n_distinct: usize, n_nonspace: usize) -> usize {
    let plan = plan_for(n_distinct, n_nonspace);
    plan.k_symbols * TIERS[plan.tier].cost()
}

/// The `k` most frequent symbols (ties by first occurrence), given in input
/// order with their counts.
pub fn glyph_symbols(symbol_counts: &[(String, usize)], k: usize) -> Vec<String> {
    let mut order: Vec<usize> = (0..symbol_counts.len()).collect();
    order.sort_by(|&a, &b| symbol_counts[b].1.cmp(&symbol_counts[a].1).then(a.cmp(&b)));
    order.truncate(k);
    order.sort_unstable();
    order.into_iter().map(|i| symbol_counts[i].0.clone()).collect()
}

/// Count symbols in first-occurrence order.
pub fn count_symbols<'a>(symbols: impl Iterator<Item = &'a str>) -> Vec<(String, usize)> {
    let mut at: HashMap<&str, usize> = HashMap::new();
    let mut out: Vec<(String, usize)> = Vec::new();
    for s in symbols {
        match at.get(s) {
            Some(&i) => out[i].1 += 1,
            None => {
                at.insert(s, out.len());
                out.push((s.to_string(), 1));
            }
        }
    }
    out
}

pub fn luma(c: [u8; 3]) -> f64 {
    (0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64) / 255.0
}

fn lumaf(c: Rgb) -> f64 {
    (0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64) / 255.0
}

fn dist2f(a: Rgb, b: [u8; 3]) -> f32 {
    let d = |x: f32, y: u8| (x - y as f32) * (x - y as f32);
    d(a[0], b[0]) + d(a[1], b[1]) + d(a[2], b[2])
}

fn dist(a: [u8; 3], b: [u8; 3]) -> f64 {
    let d = |x: u8, y: u8| (x as f64 - y as f64).powi(2);
    (d(a[0], b[0]) + d(a[1], b[1]) + d(a[2], b[2])).sqrt()
}

/// F: the 8 text ramp entries, dim → text colour.
pub fn text_ramp() -> &'static [PalIdx] {
    static F: OnceLock<Vec<PalIdx>> = OnceLock::new();
    F.get_or_init(|| {
        let pal = palette();
        let mut out: Vec<PalIdx> = Vec::new();
        for r in RAMPS_TEXT {
            for c in ramp_colors(r) {
                let idx = pal.index_of(c).expect("text ramp colours are palette entries");
                if out.last() != Some(&idx) {
                    out.push(idx);
                }
            }
        }
        out
    })
}

/// F_t: `f` evenly spaced entries of F, both ends kept (f = 1 is the top).
pub fn text_fg(tier: usize) -> &'static [PalIdx] {
    static FT: OnceLock<Vec<Vec<PalIdx>>> = OnceLock::new();
    &FT.get_or_init(|| {
        let all = text_ramp();
        TIERS
            .iter()
            .map(|t| {
                if t.f == 1 {
                    return vec![*all.last().expect("F")];
                }
                (0..t.f).map(|i| all[((i * (all.len() - 1)) as f64 / (t.f - 1) as f64).round() as usize]).collect()
            })
            .collect()
    })[tier]
}

/// Scene 11d/branding colours under the text in hold (§2.9, §3), before
/// the cap.
const HOLD_REFS: &[u32] = &[0xc7c0b4, 0xafaca3, 0xa3a39b, 0xc3b8ac, 0x9f9c94, 0xb2a99e, 0xb9b3a8];
/// Darker scene-11d tones: the flats, the marsh, the tower, the paths.
const DARK_REFS: &[u32] = &[0x56544a, 0x37352b, 0x9e9786, 0x2d2c22, 0x463f33, 0x5d5448, 0x74695c, 0x9f9c94];

fn capped(c: [u8; 3]) -> [u8; 3] {
    let l = luma(c);
    if l <= CAP_LUMA {
        return c;
    }
    let k = CAP_LUMA / l;
    [(c[0] as f64 * k).round() as u8, (c[1] as f64 * k).round() as u8, (c[2] as f64 * k).round() as u8]
}

fn mix(a: [u8; 3], b: [u8; 3], w: f64) -> [u8; 3] {
    let m = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * w).round() as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

/// G: the plate order (48 entries), from palette entries only. A plate
/// stands for the still branding scene (`branding::draw_plate_source`), so
/// it only ever needs 11d's and the smoke's tones.
/// 1. Anchors, nearest palette entry each, in priority order: `#6e6c66`
///    (luma 0.42, so every prefix holds an entry usable under the cap), the
///    capped hold colours and the darker 11d tones (capped).
/// 2. Then a greedy farthest-point fill over the palette entries with luma
///    ≤ 0.60 within 24 of an anchor.
pub fn plate() -> &'static [PalIdx] {
    static G: OnceLock<Vec<PalIdx>> = OnceLock::new();
    G.get_or_init(|| {
        let pal = palette();
        let near_idx = |c: [u8; 3]| pal.nearest([c[0] as f32, c[1] as f32, c[2] as f32]).0 as usize;
        let mut anchors: Vec<[u8; 3]> = vec![rgb_of(0x6e6c66)];
        anchors.extend(HOLD_REFS.iter().map(|&h| capped(rgb_of(h))));
        anchors.extend(DARK_REFS.iter().map(|&h| capped(rgb_of(h))));
        // Halfway mixes between the anchors so far: the plate walks between
        // them one step (≤ PLATE_STEP) at a time.
        let base = anchors.clone();
        for (i, &x) in base.iter().enumerate() {
            for &y in &base[i + 1..] {
                anchors.push(mix(x, y, 0.5));
            }
        }
        let mut chosen: Vec<usize> = Vec::new();
        for &a in &anchors {
            let i = near_idx(a);
            if !chosen.contains(&i) && luma(pal.rgb[i]) <= 0.60 {
                chosen.push(i);
            }
        }
        let cands: Vec<usize> = (0..pal.len())
            .filter(|&i| luma(pal.rgb[i]) <= 0.60 && anchors.iter().any(|&a| dist(pal.rgb[i], a) <= 24.0))
            .collect();
        let mut mind: Vec<f64> =
            cands.iter().map(|&i| chosen.iter().map(|&c| dist(pal.rgb[i], pal.rgb[c])).fold(f64::MAX, f64::min)).collect();
        while chosen.len() < TIERS[0].g {
            let (best, d) = mind.iter().enumerate().fold((0, -1.0), |acc, (j, &d)| if d > acc.1 { (j, d) } else { acc });
            if d <= 0.0 {
                break;
            }
            let pick = cands[best];
            chosen.push(pick);
            for (j, &i) in cands.iter().enumerate() {
                mind[j] = mind[j].min(dist(pal.rgb[i], pal.rgb[pick]));
            }
        }
        chosen.truncate(TIERS[0].g);
        chosen.into_iter().map(|i| PalIdx(i as u16)).collect()
    })
}

/// Nearest plate entry of the tier's prefix (`capped`: only luma ≤ 0.50).
pub fn nearest_plate(tier: usize, c: Rgb, capped: bool) -> PalIdx {
    let pal = palette();
    let g = &plate()[..TIERS[tier].g];
    let mut best = (f32::MAX, g[0]);
    for &idx in g {
        let rgb = pal.rgb_of(idx);
        if capped && luma(rgb) > 0.50 {
            continue;
        }
        let d = dist2f(c, rgb);
        if d < best.0 {
            best = (d, idx);
        }
    }
    best.1
}

/// The plate colour g of a glyph cell: the mean c of its two pixels under
/// the full luma cap (`c·min(1, CAP_LUMA/luma(c))`), from the tier's plate
/// entries with luma ≤ 0.50. The cap is applied "with the weight of the
/// fade" by the `▀` blend states, which move the cell from its own pixels
/// to g; the glyph states all use the capped g, so g changes only when the
/// scene under the cell does.
pub fn plate_of(tier: usize, top: Rgb, bottom: Rgb) -> PalIdx {
    let c = [(top[0] + bottom[0]) / 2.0, (top[1] + bottom[1]) / 2.0, (top[2] + bottom[2]) / 2.0];
    let l = lumaf(c);
    let k = if l > CAP_LUMA { (CAP_LUMA / l) as f32 } else { 1.0 };
    nearest_plate(tier, [c[0] * k, c[1] * k, c[2] * k], true)
}

/// The capped target colour of a cell whose pixels average to `c`.
pub fn capped_target(c: Rgb) -> Rgb {
    let l = lumaf(c);
    let k = if l > CAP_LUMA { (CAP_LUMA / l) as f32 } else { 1.0 };
    [c[0] * k, c[1] * k, c[2] * k]
}

/// Largest plate change per tick (RGB distance), the "near" threshold.
pub const PLATE_STEP: f64 = 48.0;

/// A plate entry with its RGB.
type PlateEntry = (PalIdx, [u8; 3]);

/// The tier's plate entries usable in hold (luma ≤ 0.50), with their RGB.
fn capped_plates(tier: usize) -> &'static [PlateEntry] {
    static CP: OnceLock<Vec<Vec<PlateEntry>>> = OnceLock::new();
    &CP.get_or_init(|| {
        let pal = palette();
        TIERS
            .iter()
            .map(|t| plate()[..t.g].iter().map(|&i| (i, pal.rgb_of(i))).filter(|&(_, c)| luma(c) <= 0.50).collect())
            .collect()
    })[tier]
}

/// One tick of a glyph cell's plate: towards the capped plate entry nearest
/// `c`, but by at most `PLATE_STEP`. If the nearest entry is further, the
/// plate moves to the entry within `PLATE_STEP` that is closest to the
/// target colour (and closer than now); if there is none it jumps (only the
/// coarse tiers have such gaps). `None` (no plate yet) takes the target.
/// Returns the plate and whether it has reached the target.
pub fn step_plate(tier: usize, cur: Option<PalIdx>, c: Rgb) -> (PalIdx, bool) {
    let target_c = capped_target(c);
    let cands = capped_plates(tier);
    let mut target = (f32::MAX, cands[0].0, cands[0].1);
    for &(idx, rgb) in cands {
        let d = dist2f(target_c, rgb);
        if d < target.0 {
            target = (d, idx, rgb);
        }
    }
    let Some(cur) = cur else { return (target.1, true) };
    let cur_rgb = palette().rgb_of(cur);
    if cur == target.1 || dist(cur_rgb, target.2) <= PLATE_STEP {
        return (target.1, true);
    }
    let here = dist2f(target_c, cur_rgb);
    let mut best: Option<(f32, PalIdx)> = None;
    for &(idx, rgb) in cands {
        if dist(cur_rgb, rgb) > PLATE_STEP {
            continue;
        }
        let d = dist2f(target_c, rgb);
        if d < here && best.is_none_or(|b| d < b.0) {
            best = Some((d, idx));
        }
    }
    match best {
        Some((_, idx)) => (idx, false),
        None => (target.1, true),
    }
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], w: f64) -> Rgb {
    let l = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * w) as f32;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2])]
}

fn lerp_f(a: Rgb, b: [u8; 3], w: f32) -> Rgb {
    [a[0] + (b[0] as f32 - a[0]) * w, a[1] + (b[1] as f32 - a[1]) * w, a[2] + (b[2] as f32 - a[2]) * w]
}

/// The approach step q_j(g) towards F_t's low end (j in 1..=m).
pub fn approach(tier: usize, g: PalIdx, j: usize) -> PalIdx {
    let pal = palette();
    let t = TIERS[tier];
    let lo = text_fg(tier)[0];
    pal.nearest(lerp_rgb(pal.rgb_of(g), pal.rgb_of(lo), j as f64 / (t.m + 1) as f64))
}

/// How a glyph cell is drawn in state `s`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellDraw {
    HalfBlock(PalIdx, PalIdx),
    Glyph(PalIdx, PalIdx),
}

/// State `s` of a glyph cell over the scene pixels `top`, `bottom` with the
/// plate colour `g`.
pub fn cell_state(tier: usize, s: usize, top: Rgb, bottom: Rgb, g: PalIdx) -> CellDraw {
    let pal = palette();
    let t = TIERS[tier];
    let b = HANDOFF_STATES;
    if s == 0 {
        return CellDraw::HalfBlock(pal.nearest(top), pal.nearest(bottom));
    }
    if s < b {
        let w = s as f32 / b as f32;
        let gc = pal.rgb_of(g);
        return CellDraw::HalfBlock(pal.nearest(lerp_f(top, gc, w)), pal.nearest(lerp_f(bottom, gc, w)));
    }
    if s == b {
        return CellDraw::HalfBlock(g, g);
    }
    let j = s - b;
    if j <= t.m {
        return CellDraw::Glyph(approach(tier, g, j), g);
    }
    let f = text_fg(tier);
    CellDraw::Glyph(f[(j - t.m - 1).min(f.len() - 1)], g)
}

/// Every glyph (fg, bg) pair the tier can emit (the `▀` handoff pairs are
/// palette pairs and not counted here).
pub fn reachable_pairs(tier: usize) -> Vec<(PalIdx, PalIdx)> {
    let t = TIERS[tier];
    let mut out = Vec::new();
    for &g in &plate()[..t.g] {
        for j in 1..=t.m {
            out.push((approach(tier, g, j), g));
        }
        for &f in text_fg(tier) {
            out.push((f, g));
        }
    }
    out.sort_unstable_by_key(|p| (p.0 .0, p.1 .0));
    out.dedup();
    out
}

/// First tick index at or after time `x` (t = tick·dt).
fn first_tick_at(x: f64, dt: f64) -> i64 {
    (x / dt - 1e-9).ceil() as i64
}

/// The scheduled state at `tick` (storyboard time only, so any duration
/// scale keeps the same timing):
/// - the reveal rises 0 → `s_max` over `FADE_SECONDS` from `start`;
/// - the dissolve falls back to 0 over `FADE_SECONDS` from `dissolve`;
/// - the first active tick of the reveal and the last active tick of the
///   dissolve are always 0 (the plain scene).
fn scheduled_state(start: f64, dissolve: f64, tick: i64, dt: f64, s_max: usize) -> usize {
    let t = tick as f64 * dt;
    let s = s_max as f64;
    let i0 = first_tick_at(start, dt);
    let reveal = if tick <= i0 { 0.0 } else { (s * (t - start) / FADE_SECONDS).floor() };
    let end = dissolve + FADE_SECONDS;
    let i_last = first_tick_at(end, dt) - 1;
    let fall = if tick < first_tick_at(dissolve, dt) {
        s
    } else if tick >= i_last {
        0.0
    } else {
        (s * (end - t) / FADE_SECONDS).floor()
    };
    reveal.min(fall).clamp(0.0, s) as usize
}

/// The glyph state at `tick`: the scheduled state, except that a tick
/// whose schedule jumps across the flat `▀`(g, g) state B (glyph on one
/// side, `▀` blend on the other) shows B instead. A glyph therefore only
/// ever appears or vanishes next to a flat cell of its own bg, at any frame
/// rate and duration scale. At 60 fps and above (scale 1) the schedule
/// moves by at most one state per tick, so nothing is pinned.
pub fn glyph_state(start: f64, dissolve: f64, tick: u64, dt: f64, s_max: usize) -> usize {
    let i = tick as i64;
    let now = scheduled_state(start, dissolve, i, dt, s_max);
    let before = scheduled_state(start, dissolve, i - 1, dt, s_max);
    let b = HANDOFF_STATES;
    if (before < b && now > b) || (before > b && now < b) {
        b
    } else {
        now
    }
}

/// Whether (fg, bg) is one of the tier's glyph pairs (debug checks).
pub fn is_reachable(tier: usize, fg: PalIdx, bg: PalIdx) -> bool {
    static SETS: OnceLock<Vec<std::collections::HashSet<(u16, u16)>>> = OnceLock::new();
    SETS.get_or_init(|| {
        (0..TIERS.len()).map(|t| reachable_pairs(t).into_iter().map(|(f, b)| (f.0, b.0)).collect()).collect()
    })[tier]
        .contains(&(fg.0, bg.0))
}
