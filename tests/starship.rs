//! starship effect tests: the bounded palette, the timing controls, the
//! timeline and transitions, and the per-scene signatures from
//! aidocs/starship/storyboard.md (§1.3 metrics, §1.6 region, §4 signatures).

use clap::Parser;

use ttfx::cli::Cli;
use ttfx::effects::starship::palette::{self, palette, Ramp, ALL_RAMPS, P_CAP};
use ttfx::effects::starship::particles::POOL_CAPACITY;
use ttfx::effects::starship::raster::Pixbuf;
use ttfx::effects::starship::scenes::{branding, catch, hot_staging, over_earth, seco, space};
use ttfx::effects::starship::text::{self, CellDraw, TIERS};
use ttfx::effects::starship::timeline::{self, blend_weight, scene_at, Blend, SceneId, TOTAL_SECONDS};
use ttfx::effects::starship::{Starship, StarshipConfig, HALF_BLOCK};
use ttfx::engine::canvas::Anchor;
use ttfx::engine::ctx::{Clock, EngineCtx};
use ttfx::engine::effect::Effect;
use ttfx::engine::terminal::{CharacterFilter, TerminalConfig};
use ttfx::utils::rng::Rng;

const INPUT: &str = include_str!("fixtures/starship_input.txt");

/// 1080p and 4K grids at scale 1 (storyboard §1.1).
const GRID_1080: (i64, i64) = (137, 33);
const GRID_4K: (i64, i64) = (274, 67);

// ---- harness ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Cell {
    symbol: String,
    fg: [u8; 3],
    bg: [u8; 3],
}

struct Run {
    ctx: EngineCtx,
    fx: Starship,
}

fn config(start_at: f64, duration_scale: f64, stress: bool) -> StarshipConfig {
    StarshipConfig { start_at, duration_scale, stress }
}

fn run_with(grid: (i64, i64), fps: i64, seed: u64, cfg: StarshipConfig) -> Run {
    run_input(grid, fps, seed, cfg, INPUT)
}

fn run_input(grid: (i64, i64), fps: i64, seed: u64, cfg: StarshipConfig, input: &str) -> Run {
    let tc = TerminalConfig {
        frame_rate: fps,
        canvas_width: grid.0,
        canvas_height: grid.1,
        ignore_terminal_dimensions: true,
        reuse_canvas: true,
        anchor_canvas: Anchor::C,
        anchor_text: Anchor::C,
        ..TerminalConfig::default()
    };
    let mut ctx = EngineCtx::new(input, tc, Rng::seeded(seed), Clock::virtual_with_frame_rate(fps)).expect("ctx");
    let mut fx = Starship::new(cfg);
    fx.build(&mut ctx).expect("build");
    Run { ctx, fx }
}

fn run(grid: (i64, i64), fps: i64, seed: u64) -> Run {
    run_with(grid, fps, seed, StarshipConfig::default())
}

impl Run {
    fn frame(&mut self) -> Option<String> {
        self.fx.next_frame(&mut self.ctx)
    }

    /// Play until the frame at storyboard time `t` has been emitted.
    fn play_to(&mut self, t: f64) {
        // Storyboard time: a tick is `dt` = 1 / (fps · duration scale).
        let target = (t / self.fx.dt()).round() as u64;
        while self.ticks_done() <= target {
            self.frame().expect("frame before the end");
        }
    }

    fn ticks_done(&self) -> u64 {
        self.fx.ticks().1
    }

    /// What the engine shows per cell (row-major): symbol, fg, bg, read from
    /// its own cell visuals (what `ctx.frame()` formats).
    fn engine_cells(&self) -> Vec<Cell> {
        let (cols, rows) = self.fx.grid();
        let canvas = &self.ctx.terminal.canvas;
        let mut out = vec![Cell { symbol: String::new(), fg: [0; 3], bg: [0; 3] }; cols * rows];
        let all = CharacterFilter { input_chars: true, inner_fill_chars: true, outer_fill_chars: true, added_chars: false };
        for id in self.ctx.terminal.collect_characters(all) {
            let ch = &self.ctx.terminal.arena[id.0 as usize];
            let c = ch.motion.current_coord;
            let (col, row) = ((c.column - canvas.left) as usize, (canvas.top - c.row) as usize);
            if col >= cols || row >= rows {
                continue;
            }
            let v = ch.animation.current_character_visual;
            let colors = v.colors.expect("colours");
            let rgb = |c: Option<ttfx::utils::graphics::Color>| {
                let (r, g, b) = c.expect("set").rgb_ints();
                [r, g, b]
            };
            out[row * cols + col] = Cell { symbol: v.symbol.to_string(), fg: rgb(colors.fg_color), bg: rgb(colors.bg_color) };
        }
        out
    }

    /// The emitted half-block pixels of the last frame (2 per cell). A glyph
    /// cell counts as its bg in both halves: the scene-metric view.
    fn engine_pixels(&self) -> Vec<[u8; 3]> {
        let cols = self.fx.grid().0;
        let cells = self.engine_cells();
        let rows = cells.len() / cols.max(1);
        let mut out = vec![[0u8; 3]; cols * rows * 2];
        for (i, c) in cells.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let top = if c.symbol == HALF_BLOCK { c.fg } else { c.bg };
            out[2 * row * cols + col] = top;
            out[(2 * row + 1) * cols + col] = c.bg;
        }
        out
    }

    /// Per cell (row-major): the input character's own symbol, or `None` for
    /// fill cells.
    fn input_cells(&self) -> Vec<Option<String>> {
        let (cols, rows) = self.fx.grid();
        let canvas = &self.ctx.terminal.canvas;
        let mut out = vec![None; cols * rows];
        let input = CharacterFilter { input_chars: true, inner_fill_chars: false, outer_fill_chars: false, added_chars: false };
        for id in self.ctx.terminal.collect_characters(input) {
            let ch = &self.ctx.terminal.arena[id.0 as usize];
            let c = ch.motion.current_coord;
            let (col, row) = ((c.column - canvas.left) as usize, (canvas.top - c.row) as usize);
            if col < cols && row < rows {
                out[row * cols + col] = Some(ch.input_symbol.clone());
            }
        }
        out
    }

    /// Glyph-aware pixels: `▀`/`▄`/`█` exactly, any other glyph as bg plus a
    /// quarter of its ink in each half.
    fn cell_pixels(&self) -> Vec<[u8; 3]> {
        let cols = self.fx.grid().0;
        let cells = self.engine_cells();
        let rows = cells.len() / cols.max(1);
        let mut out = vec![[0u8; 3]; cols * rows * 2];
        for (i, c) in cells.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let ink = |k: f64| -> [u8; 3] {
                let m = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * k).round() as u8;
                [m(c.bg[0], c.fg[0]), m(c.bg[1], c.fg[1]), m(c.bg[2], c.fg[2])]
            };
            let (top, bottom) = match c.symbol.as_str() {
                "▀" => (c.fg, c.bg),
                "▄" => (c.bg, c.fg),
                "█" => (c.fg, c.fg),
                _ => (ink(0.25), ink(0.25)),
            };
            out[2 * row * cols + col] = top;
            out[(2 * row + 1) * cols + col] = bottom;
        }
        out
    }
}

// ---- metrics (storyboard §1.3, §1.6) ----------------------------------------

fn dist(a: [u8; 3], b: [u8; 3]) -> f64 {
    let d = |x: u8, y: u8| (x as f64 - y as f64).powi(2);
    (d(a[0], b[0]) + d(a[1], b[1]) + d(a[2], b[2])).sqrt()
}

fn luma(c: [u8; 3]) -> f64 {
    (0.2126 * c[0] as f64 + 0.7152 * c[1] as f64 + 0.0722 * c[2] as f64) / 255.0
}

fn hexrgb(h: u32) -> [u8; 3] {
    [(h >> 16) as u8, (h >> 8) as u8, h as u8]
}

/// Pixels inside the measurement region u ∈ [0,1), v ∈ [0, 0.82) (by centre).
fn region(px: &[[u8; 3]], cols: usize) -> Vec<[u8; 3]> {
    let h = px.len() / cols;
    let mut out = Vec::new();
    for y in 0..h {
        if (y as f64 + 0.5) / h as f64 >= 0.82 {
            break;
        }
        out.extend_from_slice(&px[y * cols..(y + 1) * cols]);
    }
    out
}

fn frac(px: &[[u8; 3]], pred: impl Fn([u8; 3]) -> bool) -> f64 {
    px.iter().filter(|&&p| pred(p)).count() as f64 / px.len().max(1) as f64
}

fn near(px: &[[u8; 3]], hex: u32) -> f64 {
    frac(px, |p| dist(p, hexrgb(hex)) <= 48.0)
}

fn near_black(px: &[[u8; 3]]) -> f64 {
    frac(px, |p| luma(p) < 0.08)
}

fn bright(px: &[[u8; 3]]) -> f64 {
    frac(px, |p| luma(p) > 0.70)
}

/// Mean per-pixel RGB distance (position-sensitive) and the fraction of
/// pixels that moved by more than 30.
fn frame_distance(a: &[[u8; 3]], b: &[[u8; 3]]) -> (f64, f64) {
    assert_eq!(a.len(), b.len());
    let (mut sum, mut big) = (0.0, 0usize);
    for (p, q) in a.iter().zip(b) {
        let d = dist(*p, *q);
        sum += d;
        if d > 30.0 {
            big += 1;
        }
    }
    (sum / a.len() as f64, big as f64 / a.len() as f64)
}

fn pixbuf_rgb(buf: &Pixbuf) -> Vec<[u8; 3]> {
    buf.px.iter().map(|c| [c[0].clamp(0.0, 255.0) as u8, c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8]).collect()
}

// ---- the bounded palette ----------------------------------------------------

#[test]
fn palette_cap_and_constancy() {
    let p = palette();
    assert!(p.len() <= P_CAP, "palette {} > cap {P_CAP}", p.len());
    let fresh = palette::Palette::build_uncached();
    assert_eq!(fresh.checksum(), p.checksum(), "the palette is a pure constant");
    assert_eq!(fresh.len(), p.len());
}

fn emitted_in_palette(r: &Run) {
    let pal = palette();
    for px in r.engine_pixels() {
        assert!(pal.index_of(px).is_some(), "emitted colour {px:?} is not a palette entry");
    }
}

#[test]
fn emitted_colours_in_palette_and_appearance_bound() {
    use std::collections::HashSet;
    let pal = palette();
    let mut seen: HashSet<([u8; 3], [u8; 3])> = HashSet::new();
    let mut glyphs: HashSet<(String, [u8; 3], [u8; 3])> = HashSet::new();
    let mut counts = Vec::new();
    for (seed, stress) in [(1, false), (2, false), (3, false), (4, true)] {
        let mut r = run_with(GRID_1080, 60, seed, config(0.0, 1.0, stress));
        let mut k = 0u64;
        while r.frame().is_some() {
            // Every 4th tick keeps the run fast; colours are checked each time.
            if k.is_multiple_of(4) {
                for c in r.engine_cells() {
                    if c.symbol != HALF_BLOCK {
                        assert!(pal.index_of(c.fg).is_some() && pal.index_of(c.bg).is_some());
                        glyphs.insert((c.symbol, c.fg, c.bg));
                    }
                }
                let px = r.engine_pixels();
                for pair in px.chunks(r.fx.grid().0 * 2).flat_map(|rows| {
                    let cols = rows.len() / 2;
                    (0..cols).map(move |c| (rows[c], rows[cols + c]))
                }) {
                    assert!(pal.index_of(pair.0).is_some() && pal.index_of(pair.1).is_some());
                    seen.insert(pair);
                }
            }
            k += 1;
            if stress && k > 240 {
                break;
            }
        }
        counts.push(seen.len());
    }
    let bound = pal.len() * pal.len();
    // The fixture: 13 glyph symbols plus the space fill.
    let text_bound = text::text_appearance_bound(14, 13);
    eprintln!("appearance counts after each run: {counts:?} (bound |P|² = {bound}); glyph appearances {} (bound {text_bound})", glyphs.len());
    assert!(seen.len() <= bound);
    assert!(!glyphs.is_empty(), "the branding drew glyph cells");
    assert!(glyphs.len() <= text_bound, "{} glyph appearances over the bound {text_bound}", glyphs.len());
}

#[test]
fn emission_path_shared() {
    // The ceiling test measures `emit_appearance`; no other call may create
    // appearances (the visual cache only reuses what it interned).
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/effects/starship");
    let mut calls = 0;
    for entry in walk(dir) {
        let text = std::fs::read_to_string(&entry).unwrap();
        calls += text.matches(".set_appearance(").count();
    }
    assert_eq!(calls, 1, "exactly one set_appearance call (inside emit_appearance)");
}

fn walk(dir: &str) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(p.to_str().unwrap()));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}

/// Palette-mapping error over synthetic content shaped like scenes 1–3: every
/// declared ramp as a continuous full-screen gradient, plus smoke and plume
/// colours alpha-blended over the sky ramps.
#[test]
fn palette_error_synthetic() {
    let pal = palette();
    let mut errs: Vec<f64> = Vec::new();
    let mut measure = |c: [f32; 3]| {
        let q = pal.rgb_of(pal.nearest(c));
        let c8 = [c[0].round() as u8, c[1].round() as u8, c[2].round() as u8];
        errs.push(dist(c8, q));
    };
    let lerp = |a: [u8; 3], b: [u8; 3], t: f32| -> [f32; 3] {
        [0, 1, 2].map(|k| a[k] as f32 + (b[k] as f32 - a[k] as f32) * t)
    };
    let ramps: Vec<Ramp> = ALL_RAMPS.iter().flat_map(|r| r.iter().copied()).collect();
    for r in &ramps {
        for i in 0..=134 {
            measure(lerp(hexrgb(r.a), hexrgb(r.b), i as f32 / 134.0));
        }
    }
    let overlays = [0xdacbad, 0xb3a086, 0xd9c9b3, 0xc3b2b9, 0xd0b3bd, 0xf1e9d3, 0xe2af7b];
    let skies = [0x23465a, 0x406e88, 0x2c507a, 0x3b6187, 0x44709b, 0x255687, 0x5d6570, 0x272929];
    let mut pairs: Vec<(u32, u32)> = overlays.iter().flat_map(|&o| skies.iter().map(move |&s| (o, s))).collect();
    // The space scenes' particles and glows over their own backgrounds.
    pairs.extend([
        (0xe6e1db, 0x0d1725),
        (0x694f5c, 0x0d1725),
        (0xc9d2d9, 0x1f2636),
        (0xc9d2d9, 0x094287),
        (0xfafaf7, 0x0e448a),
        (0xd5e5e9, 0x010205),
        (0x565b67, 0x010205),
        (0xe2ecee, 0x191421),
        // Scene 11: steam over the overcast and the flats, flame over steel.
        (0xc4bcb0, 0x9f9c94),
        (0xc4bcb0, 0xaaa197),
        (0xf3d592, 0x9a8d7c),
        (0xc7c0b4, 0x56544a),
    ]);
    for &(o, s) in &pairs {
        for i in 0..=12 {
            let a = 0.2 + 0.6 * i as f32 / 12.0;
            measure(lerp(hexrgb(s), hexrgb(o), a));
        }
    }
    errs.sort_by(|a, b| a.total_cmp(b));
    let mean = errs.iter().sum::<f64>() / errs.len() as f64;
    let p99 = errs[(errs.len() - 1) * 99 / 100];
    eprintln!("palette error (synthetic): n={} mean={mean:.2} p99={p99:.2} max={:.2} |P|={}", errs.len(), errs.last().unwrap(), pal.len());
    assert!(mean <= 6.0, "mean palette error {mean:.2} > 6");
    assert!(p99 <= 20.0, "p99 palette error {p99:.2} > 20");
}

#[test]
fn stress_changes_every_cell() {
    let mut r = run_with(GRID_1080, 60, 1, config(0.0, 1.0, true));
    let cells = (GRID_1080.0 * GRID_1080.1) as usize;
    let mut prev: Option<Vec<String>> = None;
    for k in 0..120 {
        let frame = r.frame().expect("frame");
        if k > 0 {
            assert_eq!(r.fx.changed_cells(), cells, "tick {k}: every cell must change");
        }
        let rows: Vec<String> = frame.split('\n').map(str::to_owned).collect();
        if let Some(p) = &prev {
            for (i, (a, b)) in p.iter().zip(rows.iter()).enumerate() {
                assert_ne!(a, b, "tick {k}: row {i} unchanged");
            }
        }
        prev = Some(rows);
    }
}

// ---- determinism and length -------------------------------------------------

#[test]
fn determinism() {
    let mut a = run(GRID_1080, 60, 7);
    let mut b = run(GRID_1080, 60, 7);
    for k in 0..600 {
        assert_eq!(a.frame(), b.frame(), "tick {k}");
    }
}

fn count_frames(r: &mut Run) -> u64 {
    let mut n = 0;
    while r.frame().is_some() {
        n += 1;
    }
    n
}

#[test]
fn length_60_and_120() {
    for fps in [60, 120] {
        let mut r = run(GRID_1080, fps, 1);
        let n = count_frames(&mut r);
        assert_eq!(n, (TOTAL_SECONDS * fps as f64) as u64, "frames at {fps} fps");
    }
}

// ---- timing controls --------------------------------------------------------

#[test]
fn duration_scale_length() {
    for (scale, expect) in [(0.5, 1920), (2.0, 7680)] {
        let mut r = run_with(GRID_1080, 60, 1, config(0.0, scale, false));
        assert_eq!(count_frames(&mut r), expect, "scale {scale}");
    }
}

#[test]
fn duration_scale_keeps_storyboard_time() {
    // At scale 2 the frame for storyboard 19.0 s is tick 19·60·2 and shows scene 3.
    let mut r = run_with(GRID_1080, 60, 1, config(0.0, 2.0, false));
    let tick = (19.0f64 * 60.0 * 2.0) as u64;
    while r.ticks_done() <= tick {
        r.frame().unwrap();
    }
    assert!((r.fx.time() - 19.0).abs() < 1e-9, "storyboard time {}", r.fx.time());
    assert_eq!(scene_at(r.fx.time()), SceneId::S3);
    // Pacing changes, content does not: the scene-3 signature still holds.
    let sig = SIGS.iter().find(|s| s.name == "3").unwrap();
    let fails = check_sig(sig, &r);
    assert!(fails.is_empty(), "scene 3 at 2x duration scale: {fails:?}");
    emitted_in_palette(&r);
}

#[test]
fn seek_matches_uninterrupted() {
    for scale in [1.0, 2.0] {
        for start in [11.5, 11.503] {
            let mut full = run_with(GRID_1080, 60, 3, config(0.0, scale, false));
            let target = (start * 60.0 * scale).round() as u64;
            let mut frames = Vec::new();
            let mut k = 0;
            while let Some(f) = full.frame() {
                if k >= target {
                    frames.push(f);
                }
                if frames.len() > 60 {
                    break;
                }
                k += 1;
            }
            let mut seek = run_with(GRID_1080, 60, 3, config(start, scale, false));
            for (i, f) in frames.iter().enumerate() {
                assert_eq!(seek.frame().as_ref(), Some(f), "scale {scale} start {start}: frame +{i}");
            }
        }
    }
}

#[test]
fn seek_near_end_still_plays() {
    let mut r = run_with(GRID_1080, 60, 1, config(63.99, 1.0, false));
    assert!(count_frames(&mut r) >= 1);
    let mut low = run_with(GRID_1080, 1, 1, config(63.9, 0.25, false));
    assert!(count_frames(&mut low) >= 1, "a low frame rate and a late seek still emit a frame");
}

#[test]
fn invalid_timing_args_rejected() {
    let parse = |args: &[&str]| {
        let mut v = vec!["ttfx", "starship"];
        v.extend_from_slice(args);
        Cli::try_parse_from(v)
    };
    for bad in ["0", "-1", "nan", "inf", "0.2", "4.5", "x"] {
        assert!(parse(&["--duration-scale", bad]).is_err(), "--duration-scale {bad} must be rejected");
    }
    for bad in ["-1", "nan", "inf", "64", "100", "x"] {
        assert!(parse(&["--start-at", bad]).is_err(), "--start-at {bad} must be rejected");
    }
    for good in [&["--duration-scale", "0.25"][..], &["--duration-scale", "4.0"], &["--start-at", "0"], &["--start-at", "63.5"]] {
        assert!(parse(good).is_ok(), "{good:?} must be accepted");
    }
}

// ---- timeline ---------------------------------------------------------------

#[test]
fn timeline_table_matches_storyboard() {
    let total: f64 = timeline::SCENES.iter().map(|s| s.dur).sum();
    assert!((total - TOTAL_SECONDS).abs() < 1e-9);
    for w in timeline::SCENES.windows(2) {
        assert!((w[0].end() - w[1].start).abs() < 1e-9, "scenes are contiguous");
    }
    assert_eq!(scene_at(10.2), SceneId::S2, "B is reported inside the fade");
}

#[test]
fn blend_weight_monotonic() {
    for (start, d) in [(10.0, 0.5), (30.0, 1.0), (timeline::LOOP_FADE_START, TOTAL_SECONDS - timeline::LOOP_FADE_START)] {
        let mut last = -1.0;
        for i in 0..=1000 {
            let t = start + d * i as f64 / 1000.0 - 1e-9;
            if t < start || t >= start + d {
                continue;
            }
            let w = match blend_weight(t) {
                Some(Blend::Fade { w, .. }) | Some(Blend::Loop { w }) => w,
                None => panic!("no blend at {t}"),
            };
            assert!(w >= last && (0.0..=1.0).contains(&w), "monotonic at {t}");
            last = w;
        }
    }
    assert!(blend_weight(10.5).is_none() && blend_weight(9.99).is_none());
}

/// Visual check of the palette (not run by default):
/// `cargo test --release --test starship palette_preview -- --ignored --nocapture`
/// Each line is one declared ramp, 120 cells wide: the top half of each cell is
/// the exact gradient, the bottom half what the palette emits for it.
#[test]
#[ignore]
fn palette_preview() {
    let pal = palette();
    let ramps: Vec<Ramp> = ALL_RAMPS.iter().flat_map(|r| r.iter().copied()).collect();
    for r in &ramps {
        let (a, b) = (hexrgb(r.a), hexrgb(r.b));
        let mut line = String::new();
        for i in 0..120 {
            let t = i as f32 / 119.0;
            let c = [0, 1, 2].map(|k| a[k] as f32 + (b[k] as f32 - a[k] as f32) * t);
            let q = pal.rgb_of(pal.nearest(c));
            line.push_str(&format!(
                "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀",
                c[0].round() as u8, c[1].round() as u8, c[2].round() as u8, q[0], q[1], q[2]
            ));
        }
        println!("{line}\x1b[0m #{:06x} → #{:06x}", r.a, r.b);
    }
}

// ---- signatures (storyboard §4) ---------------------------------------------

struct Sig {
    name: &'static str,
    t: f64,
    /// (colour, expected %, tolerance %)
    near: &'static [(u32, f64, f64)],
    near_black: (f64, f64),
    bright: (f64, f64),
}

const SIGS: &[Sig] = &[
    Sig { name: "1a", t: 3.0, near: &[(0x1e3d4e, 45.0, 15.0), (0xdacbad, 15.0, 12.0)], near_black: (0.0, 30.0), bright: (5.0, 100.0) },
    Sig {
        name: "1b",
        t: 8.0,
        near: &[(0x272929, 65.0, 12.0), (0x11171b, 60.0, 12.0), (0xd9c9b3, 11.0, 10.0)],
        near_black: (0.0, 30.0),
        bright: (3.0, 100.0),
    },
    Sig { name: "2a", t: 11.5, near: &[(0x3b6187, 55.0, 20.0)], near_black: (0.0, 10.0), bright: (0.0, 5.0) },
    Sig { name: "2b", t: 14.2, near: &[(0x2b4e77, 90.0, 10.0)], near_black: (0.0, 5.0), bright: (0.0, 3.0) },
    Sig {
        name: "3",
        t: 19.0,
        near: &[(0x497197, 47.0, 12.0), (0x5d6570, 50.0, 12.0), (0xeaebe4, 4.0, 10.0)],
        near_black: (0.0, 10.0),
        bright: (2.0, 100.0),
    },
    // `#0d1725` sits at the near-black threshold, so scene 4 has no near-black limit.
    Sig { name: "4", t: 23.5, near: &[(0x0d1725, 95.0, 10.0), (0x392d3a, 25.0, 12.0)], near_black: (0.0, 100.0), bright: (0.0, 2.0) },
    Sig {
        name: "5",
        t: 27.5,
        near: &[(0x094287, 30.0, 12.0), (0x1f2636, 30.0, 12.0), (0x0d0e15, 25.0, 12.0)],
        near_black: (3.0, 25.0),
        bright: (1.0, 100.0),
    },
    Sig {
        name: "6",
        t: 33.0,
        near: &[(0x0e448a, 30.0, 12.0), (0x222736, 30.0, 12.0), (0x477db9, 17.0, 10.0)],
        near_black: (3.0, 25.0),
        bright: (1.0, 100.0),
    },
    Sig {
        name: "9",
        t: 37.0,
        near: &[(0x2d2633, 44.0, 12.0), (0x110d16, 39.0, 12.0), (0x61758e, 12.0, 10.0), (0xe3edef, 7.0, 10.0)],
        near_black: (10.0, 35.0),
        bright: (5.0, 100.0),
    },
    Sig { name: "10a", t: 39.6, near: &[(0x010205, 60.0, 15.0)], near_black: (45.0, 100.0), bright: (3.0, 100.0) },
    Sig {
        name: "10b",
        t: 42.0,
        near: &[(0x010205, 35.0, 15.0), (0x121b2d, 37.0, 12.0)],
        near_black: (15.0, 55.0),
        bright: (3.0, 100.0),
    },
    Sig {
        name: "11a",
        t: 46.0,
        near: &[(0xb2a99e, 50.0, 12.0), (0x6a6054, 33.0, 12.0), (0x524c3e, 22.0, 12.0)],
        near_black: (0.0, 5.0),
        bright: (8.0, 100.0),
    },
    Sig {
        name: "11b",
        t: 49.3,
        near: &[(0x9f9c94, 66.0, 15.0), (0xada79e, 58.0, 12.0), (0x2d2c22, 16.0, 10.0)],
        near_black: (0.0, 5.0),
        bright: (0.0, 100.0),
    },
    Sig {
        name: "11c",
        t: 52.7,
        near: &[(0x9a8d7c, 51.0, 12.0), (0xc7bbab, 27.0, 12.0), (0x2f2e2b, 12.0, 10.0)],
        near_black: (0.0, 5.0),
        bright: (3.0, 100.0),
    },
    Sig {
        name: "11d",
        t: 56.0,
        near: &[(0xa3a39b, 52.0, 12.0), (0x56544a, 26.0, 12.0), (0x37352b, 25.0, 12.0)],
        near_black: (0.0, 5.0),
        bright: (3.0, 100.0),
    },
];

/// Measure one signature on a run already positioned at its time.
fn check_sig(sig: &Sig, r: &Run) -> Vec<String> {
    let cols = r.fx.grid().0;
    let px = r.engine_pixels();
    let reg = region(&px, cols);
    let mut fails = Vec::new();
    let mut report = format!("{} @ {:.1}s:", sig.name, sig.t);
    for &(hex, want, tol) in sig.near {
        let got = 100.0 * near(&reg, hex);
        report += &format!(" #{hex:06x}={got:.1}% (want {want}±{tol})");
        if (got - want).abs() > tol {
            fails.push(format!("{}: near #{hex:06x} = {got:.1}%, want {want}±{tol}", sig.name));
        }
    }
    let nb = 100.0 * near_black(&reg);
    let br = 100.0 * bright(&reg);
    report += &format!(" near-black={nb:.1}% bright={br:.1}%");
    if nb < sig.near_black.0 || nb > sig.near_black.1 {
        fails.push(format!("{}: near-black {nb:.1}% outside {:?}", sig.name, sig.near_black));
    }
    if br < sig.bright.0 || br > sig.bright.1 {
        fails.push(format!("{}: bright {br:.1}% outside {:?}", sig.name, sig.bright));
    }
    let ph = 100.0 * near(&reg, 0xff00ff);
    if ph > 0.0 {
        fails.push(format!("{}: {ph:.1}% placeholder pixels", sig.name));
    }
    eprintln!("{report}");
    fails
}

fn signatures_at(grid: (i64, i64), names: &[&str]) {
    let mut r = run(grid, 60, 1);
    let mut fails = Vec::new();
    for sig in SIGS.iter().filter(|s| names.contains(&s.name)) {
        r.play_to(sig.t);
        fails.extend(check_sig(sig, &r));
    }
    assert!(fails.is_empty(), "signature failures at {grid:?}:\n{}", fails.join("\n"));
}

#[test]
fn signatures_1080() {
    signatures_at(GRID_1080, &["1a", "1b", "2a", "2b", "3", "4", "5", "6", "9", "10a", "10b", "11a", "11b", "11c", "11d"]);
}

#[test]
fn signatures_4k() {
    signatures_at(GRID_4K, &["1a", "3", "5", "9", "10b", "11b", "11d"]);
}

/// Dev aid (not run by default): write the emitted pixels at the given times
/// as raw RGB (cols × 2·rows) for side-by-side checks against the animatic.
/// `STARSHIP_DUMP=<scratch dir> STARSHIP_DUMP_T=3,8 cargo test --release --test starship dump_frames -- --ignored`
#[test]
#[ignore]
fn dump_frames() {
    let dir = std::env::var("STARSHIP_DUMP").expect("STARSHIP_DUMP=<dir>");
    let times: Vec<f64> = std::env::var("STARSHIP_DUMP_T").unwrap_or("3".into()).split(',').map(|s| s.parse().unwrap()).collect();
    let grid = if std::env::var("STARSHIP_DUMP_4K").is_ok() { GRID_4K } else { GRID_1080 };
    let mut r = run(grid, 60, 1);
    for t in times {
        r.play_to(t);
        let bytes: Vec<u8> = r.cell_pixels().into_iter().flatten().collect();
        std::fs::write(format!("{dir}/t{t:05.2}_{}x{}.rgb", grid.0, grid.1), bytes).unwrap();
    }
}

/// Dev aid: print the per-tick distances around every boundary (calibration).
#[test]
#[ignore]
fn calibrate_distances() {
    for fps in [60i64, 120] {
        let mut r = run(GRID_1080, fps, 1);
        let mut prev: Option<Vec<[u8; 3]>> = None;
        let mut k = 0u64;
        let mut worst_fade: f64 = 0.0;
        let mut worst_any: (f64, f64) = (0.0, 0.0);
        while r.frame().is_some() {
            let px = r.engine_pixels();
            let t = r.fx.time();
            if let Some(p) = &prev {
                let (d, _) = frame_distance(p, &px);
                let boundary = [6.0, 10.0, 12.5, 16.0, 22.0, 25.0, 30.0, 31.0, 35.0, 39.0, 45.0].iter().any(|b| (t - b).abs() < 0.6 / fps as f64);
                if boundary {
                    eprintln!("fps {fps} t {t:.3} step {d:.2}");
                }
                if (10.0..10.5).contains(&t) || (12.4..12.6).contains(&t) || (30.0..31.0).contains(&t) || t >= 62.5 {
                    worst_fade = worst_fade.max(d);
                }
                if d > worst_any.0 && !boundary {
                    worst_any = (d, t);
                }
            }
            prev = Some(px);
            k += 1;
        }
        eprintln!("fps {fps}: frames {k}, worst step in fades/continuous/loop {worst_fade:.2}, worst non-boundary step {:.2} at {:.3}", worst_any.0, worst_any.1);
    }
}

// ---- transitions, motion and the loop (storyboard §1.5) ----------------------

/// Mean per-pixel RGB distance allowed between consecutive ticks inside a
/// cross-fade, across a continuous boundary, and from the last frame to the
/// rebuild's first. Measured steps there are ≤ 2 at 60 fps (calibrate_distances).
const STEP_LIMIT: f64 = 12.0;
/// A hard cut must move the frame by more than this (measured: 81–256).
const CUT_LIMIT: f64 = 25.0;

/// No scene is a placeholder any more: the dispatch matches every `SceneId`
/// (a compile-time property), and no emitted pixel is the old magenta.
#[test]
fn no_placeholder_anywhere() {
    let mut r = run(GRID_1080, 60, 1);
    let mut k = 0u64;
    while r.frame().is_some() {
        if k.is_multiple_of(2) {
            let px = r.engine_pixels();
            let ph = px.iter().filter(|&&p| dist(p, hexrgb(0xff00ff)) <= 48.0).count();
            assert_eq!(ph, 0, "{ph} placeholder-magenta pixels at {:.3}s", r.fx.time());
        }
        k += 1;
    }
}

#[test]
fn cuts_are_cuts() {
    let mut r = run(GRID_1080, 60, 1);
    for b in [6.0, 16.0, 22.0, 25.0, 35.0, 39.0, 45.0, 47.0, 51.5, 54.0] {
        r.play_to(b - 1.0 / 60.0);
        let before = r.engine_pixels();
        r.frame().unwrap();
        assert!((r.fx.time() - b).abs() < 1e-9);
        let (d, _) = frame_distance(&before, &r.engine_pixels());
        assert!(d > CUT_LIMIT, "cut at {b}: distance {d:.1} ≤ {CUT_LIMIT}");
    }
}

fn quantised(buf: &Pixbuf) -> Vec<[u8; 3]> {
    let pal = palette();
    pixbuf_rgb(buf).iter().map(|&c| pal.rgb_of(pal.nearest([c[0] as f32, c[1] as f32, c[2] as f32]))).collect()
}

#[test]
fn fades_are_smooth_and_hit_their_endpoints() {
    for (from, to, start, d) in [(SceneId::S1, SceneId::S2, 10.0, 0.5), (SceneId::S5, SceneId::S6, 30.0, 1.0)] {
        let mut r = run(GRID_1080, 60, 1);
        r.play_to(start - 1.0 / 60.0);
        let mut prev = r.engine_pixels();
        let ticks = (d * 60.0) as usize;
        for i in 0..ticks {
            r.frame().unwrap();
            let px = r.engine_pixels();
            let (step, _) = frame_distance(&prev, &px);
            assert!(step < STEP_LIMIT, "{from:?}→{to:?} tick {i}: step {step:.2}");
            if i == 0 {
                let only_a = quantised(r.fx.render_only(from));
                let (e, _) = frame_distance(&px, &only_a);
                assert!(e < 2.0, "{from:?}→{to:?}: first fade tick is not {from:?} (distance {e:.2})");
            }
            if i == ticks - 1 {
                let only_b = quantised(r.fx.render_only(to));
                let (e, _) = frame_distance(&px, &only_b);
                assert!(e < 3.0, "{from:?}→{to:?}: last fade tick is not {to:?} (distance {e:.2})");
            }
            prev = px;
        }
    }
}

/// Particles older than one tick existed before the boundary was crossed.
fn survivors(r: &Run, owner: SceneId, dt: f64) -> usize {
    r.fx.particles().iter().filter(|p| p.owner == owner.tag() && p.age > 1.5 * dt).count()
}

#[test]
fn continuous_2a_2b_keeps_the_world() {
    let dt = 1.0 / 60.0;
    let mut r = run(GRID_1080, 60, 1);
    r.play_to(12.5 - dt);
    let before = r.engine_pixels();
    let old = survivors(&r, SceneId::S2, dt);
    // The edge detail emitted until 12.5 must carry over (storyboard §2.2).
    let edge_before = r.fx.particles().iter().filter(|p| p.owner == SceneId::S2.tag() && p.size0 == 2.0).count();
    r.frame().unwrap();
    assert!((r.fx.time() - 12.5).abs() < 1e-9);
    // A clear at this tick would leave only particles spawned on it (age 0).
    let kept = survivors(&r, SceneId::S2, dt);
    let edge_after = r.fx.particles().iter().filter(|p| p.owner == SceneId::S2.tag() && p.size0 == 2.0 && p.age > 1.5 * dt).count();
    assert!(old > 0 && kept > 0, "pre-boundary particles must survive 12.5 (before {old}, after {kept})");
    assert!(edge_before > 0 && edge_after > edge_before / 2, "edge detail carries over: {edge_before} → {edge_after}");
    let (d, _) = frame_distance(&before, &r.engine_pixels());
    assert!(d < STEP_LIMIT, "step across 12.5: {d:.2}");
}

#[test]
fn hard_cuts_clear_the_pool() {
    let dt = 1.0 / 60.0;
    let mut r = run(GRID_1080, 60, 1);
    for (b, owner) in [
        (6.0, SceneId::S1),
        (16.0, SceneId::S2),
        (22.0, SceneId::S3),
        (25.0, SceneId::S4),
        (35.0, SceneId::S6),
        (39.0, SceneId::S9),
        (45.0, SceneId::S10),
        (47.0, SceneId::S11),
        (51.5, SceneId::S11),
        (54.0, SceneId::S11),
    ] {
        r.play_to(b - dt);
        assert!(r.fx.live_particles() > 0, "particles alive before the cut at {b}");
        r.frame().unwrap();
        assert!(r.fx.particles().iter().all(|p| p.age <= 1.5 * dt), "the cut at {b} leaves no older particles");
        assert_eq!(survivors(&r, owner, dt), 0);
    }
}

#[test]
fn ascent_plume_leaves_along_the_body() {
    use ttfx::effects::starship::scenes::ascent::rocket;
    let mut r = run(GRID_1080, 60, 1);
    r.play_to(14.2);
    let (_, _, tilt) = rocket(14.2);
    assert!(tilt.to_degrees() > 10.0, "the test needs a tilted rocket");
    // Drawn axis from the tip back to the engines: (−sin tilt, cos tilt).
    let axis = (-tilt.sin(), tilt.cos());
    let plume: Vec<_> = r.fx.particles().iter().filter(|p| p.owner == SceneId::S2.tag() && p.age < 0.05).collect();
    assert!(plume.len() > 5, "fresh plume particles exist ({})", plume.len());
    let (mut vx, mut vy) = (0.0, 0.0);
    for p in &plume {
        // Undo drag over its short age; the direction is what matters.
        vx += p.vx;
        vy += p.vy;
    }
    let n = (vx * vx + vy * vy).sqrt();
    let cos = (vx * axis.0 + vy * axis.1) / n;
    assert!(vx < 0.0, "a rocket leaning right exhausts to the left (mean vx {vx:+.4})");
    assert!(cos > 0.95, "mean plume velocity within ~18° of the drawn axis (cos {cos:.3})");
}

#[test]
fn gradient_primitives() {
    let red = [255.0, 0.0, 0.0];
    let blue = [0.0, 0.0, 255.0];
    let black = [0.0, 0.0, 0.0];
    // Radial: the centre pixel is the inner colour, the rim side the outer,
    // outside is untouched, and red falls off monotonically along a radius.
    let mut b = Pixbuf::new(41, 41);
    b.radial(20.5, 20.5, 10.0, 10.0, red, blue, 1.0, 1.0);
    let at = |b: &Pixbuf, x: usize, y: usize| b.px[y * b.w + x];
    assert!(at(&b, 20, 20)[0] > 250.0 && at(&b, 20, 20)[2] < 5.0);
    assert!(at(&b, 29, 20)[2] > 200.0, "near the rim is mostly the outer colour");
    assert_eq!(at(&b, 32, 20), black, "outside the rim is untouched");
    let reds: Vec<f32> = (20..30).map(|x| at(&b, x, 20)[0]).collect();
    assert!(reds.windows(2).all(|w| w[1] <= w[0]), "monotonic: {reds:?}");
    // Linear along a diagonal, clipped to a polygon, clamped past its ends.
    let mut l = Pixbuf::new(20, 20);
    let square = [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)];
    l.polygon_linear(&square, 5.0, 5.0, 15.0, 15.0, red, blue, 1.0);
    assert_eq!(at(&l, 0, 0), red, "clamped before the start");
    assert_eq!(at(&l, 19, 19), blue, "clamped after the end");
    let mid = at(&l, 9, 10);
    assert!((mid[0] - 127.5).abs() < 20.0 && (mid[2] - 127.5).abs() < 20.0, "midpoint is halfway: {mid:?}");
    let tri = [(0.0, 0.0), (20.0, 0.0), (0.0, 20.0)];
    let mut c = Pixbuf::new(20, 20);
    c.polygon_linear(&tri, 0.0, 0.0, 20.0, 0.0, red, blue, 1.0);
    assert_eq!(at(&c, 18, 18), black, "outside the polygon is untouched");
}

#[test]
fn implemented_shots_move() {
    let mut r = run(GRID_1080, 60, 1);
    for (a, b) in [
        (2.0, 2.5),
        (7.0, 7.5),
        (11.0, 11.5),
        (14.0, 14.5),
        (18.0, 18.5),
        (23.0, 23.5),
        (26.0, 26.5),
        (32.0, 32.5),
        (36.0, 36.5),
        (40.0, 40.5),
        (43.0, 43.5),
        (45.5, 46.0),
        (48.0, 48.5),
        (52.0, 52.5),
        (55.0, 55.5),
        (60.0, 60.5),
    ] {
        r.play_to(a);
        let pa = r.engine_pixels();
        r.play_to(b);
        // A frame-wide mean says little for a small rocket on a clear sky, so
        // count pixels that actually changed.
        let pb = r.engine_pixels();
        let moved = pa.iter().zip(&pb).filter(|(x, y)| x != y).count();
        assert!(moved >= 20, "frames at {a} and {b}: only {moved} pixels changed: frozen");
    }
}

#[test]
fn scene1_starts_without_particles() {
    let mut r = run(GRID_1080, 60, 1);
    r.frame().unwrap();
    assert_eq!(r.fx.time(), 0.0);
    assert_eq!(r.fx.live_particles(), 0);
}

#[test]
fn loop_rebuild_is_seamless() {
    for fps in [60, 120] {
        let mut a = run(GRID_1080, fps, 5);
        a.frame().unwrap();
        let first = a.engine_pixels();
        let mut last = first.clone();
        let mut prev_cells: Option<Vec<[u8; 3]>> = None;
        while a.frame().is_some() {
            let t = a.fx.time();
            if t >= 63.5 {
                // The last 0.5 s, tick by tick, like a fade, glyph-aware.
                let cp = a.cell_pixels();
                if let Some(p) = &prev_cells {
                    let (d, _) = frame_distance(p, &cp);
                    assert!(d < STEP_LIMIT, "loop fade step {d:.2} at {t:.3}s ({fps} fps)");
                }
                prev_cells = Some(cp);
            }
            if t >= 63.7 {
                let glyphs = a.engine_cells().iter().filter(|c| c.symbol != HALF_BLOCK).count();
                assert_eq!(glyphs, 0, "glyph cells left at {t:.3}s ({fps} fps)");
            }
            last = a.engine_pixels();
        }
        // The screensaver rebuilds with the same seed after None.
        let mut b = run(GRID_1080, fps, 5);
        b.frame().unwrap();
        let again = b.engine_pixels();
        assert_eq!(first, again, "the rebuilt first frame differs ({fps} fps)");
        let (d, _) = frame_distance(&last, &again);
        assert!(d < STEP_LIMIT, "last → first frame distance {d:.2} at {fps} fps");
    }
}

#[test]
fn palette_error_real_frames() {
    let mut r = run(GRID_1080, 60, 1);
    let mut errs = Vec::new();
    for sig in SIGS {
        r.play_to(sig.t);
        let src = pixbuf_rgb(r.fx.unquantised());
        for (a, b) in src.iter().zip(r.fx.pixels()) {
            errs.push(dist(*a, b));
        }
    }
    errs.sort_by(|a, b| a.total_cmp(b));
    let mean = errs.iter().sum::<f64>() / errs.len() as f64;
    let p99 = errs[(errs.len() - 1) * 99 / 100];
    eprintln!("palette error (scenes 1–3): mean={mean:.2} p99={p99:.2}");
    assert!(mean <= 6.0 && p99 <= 20.0, "palette error mean {mean:.2} p99 {p99:.2}");
}

// ---- the space scenes (storyboard §2.4–2.8) ---------------------------------

/// The pool never overflows (no spawn is dropped) over a whole run, and the
/// two big systems peak near the storyboard's figures (3 000 and 3 100).
#[test]
fn particle_budget_holds() {
    let mut r = run(GRID_1080, 60, 1);
    let (mut max_all, mut max5, mut max10, mut max11, mut max_b) = (0usize, 0usize, 0usize, 0usize, 0usize);
    while r.frame().is_some() {
        let n = r.fx.live_particles();
        max_all = max_all.max(n);
        match scene_at(r.fx.time()) {
            SceneId::S5 => max5 = max5.max(n),
            SceneId::S10 => max10 = max10.max(n),
            SceneId::S11 => max11 = max11.max(n),
            SceneId::B => max_b = max_b.max(n),
            _ => {}
        }
    }
    eprintln!("live particles: max {max_all}, scene 5 {max5}, scene 10 {max10}, scene 11 {max11}, B {max_b}");
    assert!((1000..=3500).contains(&max11), "scene 11 peak {max11}");
    assert!((2000..=4500).contains(&max_b), "B peak {max_b}");
    assert!(max_all < POOL_CAPACITY, "the pool filled up ({max_all}): spawns were dropped");
    assert!((1500..=4000).contains(&max5), "scene 5 peak {max5}");
    assert!((1500..=4000).contains(&max10), "scene 10 peak {max10}");
}

/// The 1080p aspect A = cols·14 / (rows·32).
const A_1080: f64 = 137.0 * 14.0 / (33.0 * 32.0);

/// The booster's whole body leaves the frame by 29.8 s (not just its glow).
#[test]
fn booster_falls_away() {
    let at = |t: f64| hot_staging::booster(t, A_1080);
    assert!(at(27.39).is_none() && at(25.0).is_none(), "no booster before 27.4");
    // Every corner moves down monotonically; the rotation reaches 10°.
    let mut prev: Option<[(f64, f64); 4]> = None;
    let mut k = 0;
    loop {
        let t = 27.4 + k as f64 / 120.0;
        if t >= 29.8 {
            break;
        }
        let b = at(t).expect("booster during the fall");
        if let Some(p) = prev {
            for (c, q) in b.corners.iter().zip(p) {
                assert!(c.1 > q.1, "t {t:.3}: a corner moved up");
            }
        }
        prev = Some(b.corners);
        k += 1;
    }
    let last = at(29.8 - 1e-6).unwrap();
    assert!((last.rotation.to_degrees() - 10.0).abs() < 0.1, "rotation {:.2}°", last.rotation.to_degrees());
    // Still in view late in the shot; wholly below the frame from 29.75.
    assert!(at(29.4).unwrap().corners.iter().any(|c| c.1 < 1.0), "the body is still visible at 29.4");
    for t in [29.75, 29.77, 29.79, 29.799] {
        let b = at(t).unwrap();
        assert!(b.corners.iter().all(|c| c.1 >= 1.0), "t {t}: the body is not wholly below the frame: {:?}", b.corners);
    }
    assert!(at(29.8).is_none() && at(30.5).is_none(), "gone from 29.8");

    // Render check on the same world state, with and without the booster:
    // the difference is exactly its footprint.
    let mut r = run(GRID_1080, 60, 1);
    let diff_at = |r: &mut Run, t: f64| -> (Vec<(usize, usize)>, Pixbuf) {
        r.play_to(t);
        let (cols, rows) = r.fx.grid();
        let world = r.fx.world().expect("world");
        let mut with = Pixbuf::new(cols, rows * 2);
        let mut without = Pixbuf::new(cols, rows * 2);
        hot_staging::draw_opts(world, r.fx.time(), &mut with, true);
        hot_staging::draw_opts(world, r.fx.time(), &mut without, false);
        let mut d = Vec::new();
        for y in 0..with.h {
            for x in 0..with.w {
                if with.px[y * with.w + x] != without.px[y * with.w + x] {
                    d.push((x, y));
                }
            }
        }
        (d, with)
    };
    let inside = |poly: &[(f64, f64)], x: f64, y: f64| {
        let mut c = false;
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            if (p.1 > y) != (q.1 > y) && x < p.0 + (y - p.1) / (q.1 - p.1) * (q.0 - p.0) {
                c = !c;
            }
        }
        c
    };
    let (diff, buf) = diff_at(&mut r, 28.5);
    let b = at(r.fx.time()).unwrap();
    let (w, h) = (buf.w as f64, buf.h as f64);
    // Corners and the glow disc in pixels (the camera only adds a small shake).
    let shake_px = 0.003 * h * 1.2;
    let poly: Vec<(f64, f64)> = b.corners.iter().map(|c| (c.0 * w, c.1 * h)).collect();
    let (ex, ey) = (b.engine.0 * w, b.engine.1 * h);
    let glow_rx = 0.03 * b.scale * buf.hx() + shake_px + 1.0;
    let glow_ry = 0.03 * b.scale * buf.hy() + shake_px + 1.0;
    let mut in_body = 0;
    let mut body_px = 0;
    for y in 0..buf.h {
        for x in 0..buf.w {
            if inside(&poly, x as f64 + 0.5, y as f64 + 0.5) {
                body_px += 1;
                if diff.contains(&(x, y)) {
                    in_body += 1;
                }
            }
        }
    }
    assert!(body_px > 0 && in_body as f64 >= 0.8 * body_px as f64, "the body is drawn at its corners: {in_body}/{body_px}");
    for &(x, y) in &diff {
        let (xc, yc) = (x as f64 + 0.5, y as f64 + 0.5);
        let near_body = poly.iter().any(|p| (p.0 - xc).abs() < shake_px + 1.5 && (p.1 - yc).abs() < shake_px + 1.5)
            || inside(&poly, xc, yc)
            || {
                // Grow the body by the shake for the containment test.
                let c = poly.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 / 4.0, a.1 + p.1 / 4.0));
                let grown: Vec<(f64, f64)> = poly
                    .iter()
                    .map(|p| {
                        let (dx, dy) = (p.0 - c.0, p.1 - c.1);
                        let l = (dx * dx + dy * dy).sqrt().max(1e-9);
                        (p.0 + dx / l * (shake_px + 1.5), p.1 + dy / l * (shake_px + 1.5))
                    })
                    .collect();
                inside(&grown, xc, yc)
            };
        let in_glow = ((xc - ex) / glow_rx).powi(2) + ((yc - ey) / glow_ry).powi(2) <= 1.0;
        assert!(near_body || in_glow, "pixel ({x}, {y}) depends on the booster layer but is outside its body and glow");
    }
    for t in [29.75, 29.8, 29.85] {
        let (diff, _) = diff_at(&mut r, t);
        assert!(diff.is_empty(), "t {t}: the booster layer still changes {} pixels", diff.len());
    }
    // No spark is spawned once the booster has gone.
    r.play_to(29.95);
    let t = r.fx.time();
    let young = r.fx.particles().iter().filter(|p| p.owner == SceneId::S5.tag() && p.life <= 0.61 && p.age < t - 29.8).count();
    assert_eq!(young, 0, "sparks spawned after 29.8");
}

#[test]
fn seco_stars_and_bloom() {
    assert_eq!(space::STARS, 60);
    for i in 0..space::STARS {
        let (u, v, ..) = space::star(i);
        assert!((0.0..1.0).contains(&u) && (0.0..1.0).contains(&v));
        for k in 0..200 {
            let l = space::star_luma(i, 39.0 + k as f64 * 0.03);
            assert!(l > 0.0 && l <= 0.35, "star {i} luma {l:.3} > 0.35");
        }
    }
    assert_eq!(seco::star_alpha(43.99), 0.0, "no stars before 44.0");
    assert!(seco::star_alpha(44.9) > 0.9, "stars by 44.9");
    let mut prev = 0.0;
    for k in 0..=600 {
        let t = 39.0 + k as f64 * 0.01;
        let (r, _) = seco::bloom(t);
        assert!(r >= prev, "bloom radius shrinks at {t:.2}");
        prev = r;
    }
    let (r_peak, _) = seco::bloom(seco::BLOOM_START + seco::BLOOM_GROW);
    assert!((r_peak - 0.60).abs() < 1e-9, "peak radius {r_peak}");
    let (_, a_end) = seco::bloom(45.0);
    assert!((a_end - 0.6).abs() < 1e-9, "haze alpha at 45.0 is {a_end}");
}

/// At 120 fps (the screensaver rate), consecutive ticks through the staging
/// burst and its shake, the booster's fall and the bloom's fast start never
/// jump: the mean step stays under the per-tick limit and the changed-pixel
/// fraction never spikes far above its median.
#[test]
fn events_step_smoothly() {
    let fps = 120;
    let mut r = run(GRID_1080, fps, 1);
    // (start, end, changed-fraction floor): the settling tower is a textured
    // lattice moving at sub-pixel speed, so a 1-px shift changes most of its
    // pixels at once; its floor is larger.
    for (a, b, floor) in [(25.95, 26.6, 0.02), (27.4, 29.8, 0.02), (39.05, 40.0, 0.02), (49.0, 50.4, 0.05), (50.3, 51.3, 0.05), (58.0, 59.5, 0.02)] {
        r.play_to(a - 1.0 / fps as f64);
        let mut prev = r.engine_pixels();
        let mut steps = Vec::new();
        while r.fx.time() < b {
            r.frame().unwrap();
            let px = r.engine_pixels();
            let changed = px.iter().zip(&prev).filter(|(x, y)| x != y).count() as f64 / px.len() as f64;
            let (d, _) = frame_distance(&prev, &px);
            steps.push((r.fx.time(), d, changed));
            prev = px;
        }
        let mut fr: Vec<f64> = steps.iter().map(|s| s.2).collect();
        fr.sort_by(|x, y| x.total_cmp(y));
        let median = fr[fr.len() / 2];
        let worst_d = steps.iter().map(|s| s.1).fold(0.0, f64::max);
        let worst_c = fr.last().copied().unwrap();
        let mut by_step = steps.clone();
        by_step.sort_by(|x, y| y.1.total_cmp(&x.1));
        let top: Vec<String> = by_step.iter().take(3).map(|s| format!("{:.3}s {:.2}", s.0, s.1)).collect();
        eprintln!("{a}–{b}: worst step {worst_d:.2} ({}), changed median {:.3} max {:.3}", top.join(", "), median, worst_c);
        for &(t, d, c) in &steps {
            assert!(d < STEP_LIMIT, "t {t:.3}: step {d:.2} ≥ {STEP_LIMIT}");
            // A floor (2 %) keeps a near-static window (median ≈ 0) from
            // flagging ordinary particle motion as a spike.
            assert!(c <= 3.0 * median + floor, "t {t:.3}: {:.1}% of pixels changed (median {:.1}%)", 100.0 * c, 100.0 * median);
        }
    }
}

/// Screen aspects A = cols·14 / (rows·32) to check geometry on: 4:3-ish,
/// 2560×1440, 4K, 1080p and a 274×33-cell ultrawide.
const ASPECTS: [f64; 5] = [
    100.0 * 14.0 / (33.0 * 32.0),
    182.0 * 14.0 / (45.0 * 32.0),
    274.0 * 14.0 / (67.0 * 32.0),
    137.0 * 14.0 / (33.0 * 32.0),
    274.0 * 14.0 / (33.0 * 32.0),
];

/// Scene 9's sun sits in black space right of the Earth (storyboard §2.7),
/// whatever the screen's shape: the colour signatures cannot see this.
#[test]
fn ship_over_earth_sun_is_in_space() {
    for a in ASPECTS {
        let ((cx, cy), r) = over_earth::earth(a);
        let (su, sv) = over_earth::SUN_AT;
        let d = ((su * a - cx).powi(2) + (sv - cy).powi(2)).sqrt() - r;
        assert!(d >= over_earth::SUN_CORE_R, "A {a:.3}: the sun's core is {d:.3} H from the Earth's edge (inside or touching it)");
        // The Earth is on the left of the sun: the edge point level with the
        // sun is further left.
        let edge_u = (cx + (r * r - (sv - cy).powi(2)).max(0.0).sqrt()) / a;
        assert!(edge_u < su, "A {a:.3}: the Earth's edge ({edge_u:.3}) is not left of the sun ({su})");
    }
}

/// SECO's limb is a thin arc through its two ends on the top edge, dipping
/// `LIMB_DEPTH` at any aspect, and on an ultrawide grid nothing below it is
/// drawn in the limb colour.
#[test]
fn seco_limb_arc_holds_on_any_aspect() {
    for a in ASPECTS {
        let ((cx, cy), r) = seco::limb_arc(a);
        for (u, v) in seco::LIMB_ENDS {
            let d = ((u * a - cx).powi(2) + (v - cy).powi(2)).sqrt();
            assert!((d - r).abs() < 1e-9, "A {a:.3}: the arc misses its end ({u}, {v}) by {:.4} H", d - r);
        }
        let lowest = cy + r;
        assert!((lowest - seco::LIMB_DEPTH).abs() < 1e-9 && lowest < 0.06, "A {a:.3}: the arc dips to v {lowest:.3}");
    }
    // Render probe on the ultrawide grid, before the bloom: outside the hull
    // and the engine's plume, no pixel below v = 0.07 is near the limb colour.
    let mut r = run((274, 33), 60, 1);
    r.play_to(39.0);
    let buf = r.fx.render_only(SceneId::S10);
    let (w, h) = (buf.w as f64, buf.h as f64);
    let hull: Vec<(f64, f64)> = seco::HULL_POLY.iter().map(|p| (p.0 * w, p.1 * h)).collect();
    let inside = |x: f64, y: f64| {
        let mut c = false;
        for i in 0..hull.len() {
            let (p, q) = (hull[i], hull[(i + 1) % hull.len()]);
            if (p.1 > y) != (q.1 > y) && x < p.0 + (y - p.1) / (q.1 - p.1) * (q.0 - p.0) {
                c = !c;
            }
        }
        c
    };
    let (ax, ay) = (seco::APEX.0 * w, seco::APEX.1 * h);
    let mut stray = 0;
    for y in 0..buf.h {
        for x in 0..buf.w {
            let (xc, yc) = (x as f64 + 0.5, y as f64 + 0.5);
            let near_plume = ((xc - ax) / (0.12 * buf.hx())).powi(2) + ((yc - ay) / (0.12 * buf.hy())).powi(2) <= 1.0;
            if yc / h < 0.07 || inside(xc, yc) || near_plume {
                continue;
            }
            let c = buf.px[y * buf.w + x];
            let px = [c[0].clamp(0.0, 255.0) as u8, c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8];
            if dist(px, hexrgb(0x6f95ab)) <= 48.0 {
                stray += 1;
            }
        }
    }
    assert_eq!(stray, 0, "{stray} limb-coloured pixels below the thin top arc on the ultrawide grid");
}

// ---- the catch (storyboard §2.9) --------------------------------------------

#[test]
fn catch_geometry() {
    // The tower settles: down to its overshoot minimum, back up, at rest.
    let samples: Vec<(f64, f64)> = (0..=4500).map(|i| 47.0 + i as f64 * 0.001).map(|t| (t, catch::tower_top_11b(t))).collect();
    let (t_min, v_min) = samples.iter().copied().fold((0.0, f64::MAX), |acc, x| if x.1 < acc.1 { x } else { acc });
    assert!(v_min < 0.30 && v_min > 0.28, "overshoot minimum {v_min:.4} at {t_min:.3}");
    for w in samples.windows(2) {
        let ((t0, v0), (_, v1)) = (w[0], w[1]);
        if t0 < t_min {
            assert!(v1 <= v0 + 1e-12, "tower top rises before its minimum at {t0:.3}");
        } else {
            assert!(v1 >= v0 - 1e-12, "tower top falls after its minimum at {t0:.3}");
        }
    }
    for &(t, v) in samples.iter().filter(|s| s.0 >= 50.33) {
        assert!((v - 0.30).abs() <= 0.002, "tower top {v:.4} at {t:.3}, not at rest");
    }
    // 2 ms: below x = 1e-4 the engine's bezier uses a linear approximation.
    let h = 2e-3;
    let before = (catch::tower_top_11b(49.0) - catch::tower_top_11b(49.0 - h)) / h;
    let after = (catch::tower_top_11b(49.0 + h) - catch::tower_top_11b(49.0)) / h;
    assert!((before - after).abs() / before.abs() < 0.03, "speed jumps at 49.0: {before:.4} → {after:.4}");
    // The arms: open at 35° until 50.3, closing monotonically to 5° at 51.3.
    assert!((catch::arm_angle(50.0).to_degrees() - 35.0).abs() < 1e-9);
    assert!((catch::arm_angle(51.3).to_degrees() - 5.0).abs() < 1e-9);
    let mut prev = catch::arm_angle(50.3);
    for i in 1..=100 {
        let a = catch::arm_angle(50.3 + i as f64 * 0.01);
        assert!(a <= prev + 1e-12, "the arms open again at {:.2}", 50.3 + i as f64 * 0.01);
        prev = a;
    }
    // On every aspect the closed tips touch the hull (within a pixel at the
    // coarsest 33-row grid) and never cross the ship's axis.
    let px_h = 1.0 / (2.0 * 33.0 * 8.0 / 7.0);
    for a in ASPECTS {
        let (x0, x1, _, _) = catch::ship_11b(a);
        for (_, tip) in catch::arms(51.3, a) {
            assert!((tip.0 - x1).abs() <= px_h, "aspect {a:.3}: tip at {:.4}, hull side at {x1:.4}", tip.0);
        }
        for i in 0..=450 {
            let t = 47.0 + i as f64 * 0.01;
            for (_, tip) in catch::arms(t, a) {
                assert!(tip.0 > (x0 + x1) / 2.0, "aspect {a:.3}: an arm crosses the ship's axis at {t:.2}");
            }
        }
    }
}

#[test]
fn continuous_11d_b_keeps_the_world() {
    let dt = 1.0 / 60.0;
    let mut r = run(GRID_1080, 60, 1);
    r.play_to(58.0 - dt);
    let before = r.engine_pixels();
    let old = survivors(&r, SceneId::S11, dt);
    r.frame().unwrap();
    assert!((r.fx.time() - 58.0).abs() < 1e-9);
    let kept = survivors(&r, SceneId::S11, dt);
    assert!(old > 0 && kept > old / 2, "11d's steam must survive 58.0 (before {old}, after {kept})");
    let (d, _) = frame_distance(&before, &r.engine_pixels());
    assert!(d < STEP_LIMIT, "step across 58.0: {d:.2}");
}

// ---- the branding (storyboard §3) and its bounded glyph cells ---------------

#[test]
fn text_budget_and_tiers() {
    for nd in 0..=20_000usize {
        for n in [nd.saturating_sub(1), nd] {
            let b = text::text_appearance_bound(nd, n);
            assert!(b + nd <= text::SYMBOL_BUDGET.max(nd), "n_d {nd}, n {n}: {b} glyph appearances");
        }
    }
    for (n, tier) in [(17, 0), (18, 1), (35, 1), (36, 2), (70, 2), (71, 3), (157, 3), (158, 4), (487, 4), (488, 5), (2047, 5), (2048, 6), (5119, 6)] {
        let p = text::plan_for(n + 1, n);
        assert_eq!((p.tier, p.k_symbols), (tier, n), "n = {n}");
    }
    let p = text::plan_for(5121, 5120);
    assert_eq!((p.tier, p.k_symbols), (6, text::SYMBOL_BUDGET - 5121), "overflow keeps the k most frequent");
    assert_eq!(text::plan_for(18_358, 18_357).k_symbols, 0, "n_d ≥ budget: no glyphs");
    let counts: Vec<(String, usize)> = [("a", 3), ("b", 1), ("c", 3), ("d", 2)].iter().map(|&(s, c)| (s.to_string(), c)).collect();
    assert_eq!(text::glyph_symbols(&counts, 2), ["a", "c"]);
    assert_eq!(text::glyph_symbols(&counts, 3), ["a", "c", "d"]);
    assert_eq!(text::glyph_symbols(&counts, 9).len(), 4);
    let ties: Vec<(String, usize)> = ["x", "y", "z"].iter().map(|s| (s.to_string(), 1)).collect();
    assert_eq!(text::glyph_symbols(&ties, 2), ["x", "y"], "ties go by first occurrence");
}

/// The reveal and dissolve state index, at the screensaver's and the CLI's
/// frame rates and at accelerated playback (`--duration-scale 0.25`):
/// - the plain scene on the first and last active tick, and 0 outside;
/// - hold from `start + FADE_SECONDS` (+ one tick), every dissolve done by
///   63.7, so the storyboard timing holds at any scale;
/// - a glyph never appears or vanishes except next to the flat state B;
/// - at scale 1 (60 and 120 fps) at most one state per tick.
#[test]
fn glyph_state_index_is_tick_aligned() {
    let mut rng = Rng::seeded(7);
    let b = text::HANDOFF_STATES;
    for (fps, scale) in [(60.0, 1.0), (120.0, 1.0), (60.0, 0.25), (120.0, 0.25), (60.0, 4.0)] {
        let dt = 1.0 / (fps * scale);
        let s_max = TIERS[0].states();
        let mut starts: Vec<f64> = (0..2000).map(|_| rng.uniform(58.0, branding::REVEAL_START + branding::REVEAL_SPAN)).collect();
        starts.extend([58.91875, 58.0, branding::REVEAL_START + branding::REVEAL_SPAN]);
        for start in starts {
            let dissolve = branding::DISSOLVE_LAST - (start - branding::REVEAL_START) * branding::DISSOLVE_SPAN / branding::REVEAL_SPAN;
            let end = dissolve + text::FADE_SECONDS;
            assert!(end <= 63.7 + 1e-9, "dissolve of {start} ends at {end}");
            let state = |i: u64| text::glyph_state(start, dissolve, i, dt, s_max);
            let i_from = ((start - 0.2) / dt) as u64;
            let i_to = ((end + 0.2) / dt) as u64;
            let mut prev = state(i_from);
            assert_eq!(prev, 0);
            let mut first_on: Option<u64> = None;
            let mut last_on: Option<u64> = None;
            for i in i_from + 1..=i_to {
                let s = state(i);
                let t = i as f64 * dt;
                let ctx = format!("start {start} at {fps} fps ×{scale}: {prev} → {s} at {t:.4}");
                if scale == 1.0 {
                    assert!(s.abs_diff(prev) <= 1, "{ctx}");
                }
                assert!(!((prev < b && s > b) || (prev > b && s < b)), "a glyph switch skips the flat state: {ctx}");
                if t + 1e-9 >= start && first_on.is_none() {
                    first_on = Some(i);
                    assert_eq!(s, 0, "the first active tick is not the plain scene: {ctx}");
                }
                if t >= start + text::FADE_SECONDS + dt + 1e-9 && t < dissolve - 1e-9 {
                    assert_eq!(s, s_max, "not in hold: {ctx}");
                }
                if t < end - 1e-9 {
                    last_on = Some(i);
                } else {
                    assert_eq!(s, 0, "glyph left after its dissolve: {ctx}");
                }
                prev = s;
            }
            assert_eq!(state(last_on.expect("dissolves")), 0, "start {start}: the last active tick is not the plain scene");
            if scale == 1.0 {
                let held = (0..).map(|k| i_from + k).find(|&i| state(i) == s_max).unwrap() as f64 * dt;
                assert!(held <= 59.5 + 1e-9, "start {start}: hold at {held:.4}");
            }
        }
    }
}

#[test]
fn glyph_cell_states() {
    let pal = palette();
    let f = text::text_ramp();
    assert_eq!(f.len(), 8);
    assert_eq!(pal.rgb_of(*f.last().unwrap()), hexrgb(0xf3efe6));
    let scenes = [([200.0f32, 190.0, 170.0], [40.0f32, 60.0, 90.0]), ([30.0, 61.0, 78.0], [64.0, 110.0, 136.0]), ([120.0, 118.0, 110.0], [86.0, 84.0, 74.0])];
    for (tier, &t) in TIERS.iter().enumerate() {
        let s_max = t.states();
        let pairs: std::collections::HashSet<(u16, u16)> = text::reachable_pairs(tier).iter().map(|p| (p.0 .0, p.1 .0)).collect();
        assert!(pairs.len() <= t.cost(), "T{tier}: {} pairs > cost {}", pairs.len(), t.cost());
        let g = &text::plate()[..t.g];
        assert!(g.iter().any(|&i| text::luma(pal.rgb_of(i)) <= 0.50), "T{tier}: no capped plate entry");
        for &gi in g {
            for (top, bottom) in scenes {
                assert_eq!(text::cell_state(tier, 0, top, bottom, gi), CellDraw::HalfBlock(pal.nearest(top), pal.nearest(bottom)));
                assert_eq!(text::cell_state(tier, text::HANDOFF_STATES, top, bottom, gi), CellDraw::HalfBlock(gi, gi));
                let mut prev_fg = pal.rgb_of(gi);
                for s in text::HANDOFF_STATES + 1..=s_max {
                    let CellDraw::Glyph(fg, bg) = text::cell_state(tier, s, top, bottom, gi) else { panic!("T{tier} s {s}: not a glyph") };
                    assert_eq!(bg, gi, "T{tier} s {s}: the glyph's bg is the plate");
                    assert!(pairs.contains(&(fg.0, bg.0)), "T{tier} s {s}: pair outside the tier's set");
                    if tier <= 2 {
                        let step = dist(prev_fg, pal.rgb_of(fg));
                        assert!(step <= 48.0, "T{tier} plate {:?} s {s}: fg step {step:.1}", pal.rgb_of(gi));
                    }
                    prev_fg = pal.rgb_of(fg);
                }
                assert_eq!(text::cell_state(tier, s_max, top, bottom, gi), CellDraw::Glyph(*f.last().unwrap(), gi), "T{tier}: hold is #f3efe6");
            }
        }
    }
}

/// The plate stands in for the scene behind the text: its quantisation
/// error on real composited text-box colours.
#[test]
fn plate_quantisation_error() {
    let mut r = run(GRID_1080, 60, 1);
    let pal = palette();
    let mut errs = Vec::new();
    for t in [59.5, 61.0, 62.5, 63.0] {
        r.play_to(t);
        let buf = r.fx.unquantised();
        for (col, row, slot) in r.fx.glyph_cells() {
            let (top, bottom) = (buf.px[2 * row * buf.w + col], buf.px[(2 * row + 1) * buf.w + col]);
            let c = [0, 1, 2].map(|k| (top[k] + bottom[k]) / 2.0);
            let _ = slot;
            let l = text::luma([c[0] as u8, c[1] as u8, c[2] as u8]);
            let k = if l > text::CAP_LUMA { (text::CAP_LUMA / l) as f32 } else { 1.0 };
            let c = c.map(|v| v * k);
            let g = text::nearest_plate(0, c, true);
            errs.push(dist([c[0] as u8, c[1] as u8, c[2] as u8], pal.rgb_of(g)));
        }
        let at: Vec<f64> = errs.iter().rev().take(r.fx.glyph_cells().len()).copied().collect();
        eprintln!("plate error at {t}: mean {:.2} max {:.2}", at.iter().sum::<f64>() / at.len() as f64, at.iter().fold(0.0f64, |m, &x| m.max(x)));
    }
    errs.sort_by(|a, b| a.total_cmp(b));
    let mean = errs.iter().sum::<f64>() / errs.len() as f64;
    let p99 = errs[(errs.len() - 1) * 99 / 100];
    eprintln!("plate error: n={} mean={mean:.2} p99={p99:.2}", errs.len());
    assert!(mean <= 10.0 && p99 <= 28.0, "plate error mean {mean:.2} p99 {p99:.2}");
}

/// Share of the non-space input characters shown with their own symbol and a
/// luma contrast ≥ 0.35.
fn visible_share(r: &Run) -> (f64, usize) {
    let cells = r.engine_cells();
    let inputs = r.input_cells();
    let (mut total, mut shown) = (0usize, 0usize);
    for (c, i) in cells.iter().zip(&inputs) {
        if let Some(sym) = i {
            total += 1;
            if &c.symbol == sym && luma(c.fg) - luma(c.bg) >= 0.35 {
                shown += 1;
            }
        }
    }
    (shown as f64 / total.max(1) as f64, total)
}

#[test]
fn branding_reveal() {
    for fps in [60i64, 120] {
        let dt = 1.0 / fps as f64;
        let mut r = run(GRID_1080, fps, 1);
        r.play_to(58.0 - dt);
        assert!(r.engine_cells().iter().all(|c| c.symbol == HALF_BLOCK), "glyphs before 58.0");
        r.frame().unwrap();
        let world = r.fx.world().expect("world");
        let mut slots: Vec<_> = world.text.clone();
        assert!(!slots.is_empty());
        for s in &slots {
            assert!(s.start >= 58.0 && s.start <= branding::REVEAL_START + branding::REVEAL_SPAN + 1e-9, "start {}", s.start);
        }
        slots.sort_by(|a, b| a.start.total_cmp(&b.start));
        for w in slots.windows(2) {
            assert!(w[0].density >= w[1].density, "a later start has more smoke ({} < {})", w[0].density, w[1].density);
        }
        r.play_to(59.5);
        let cells = r.engine_cells();
        let inputs = r.input_cells();
        for (col, row, _) in r.fx.glyph_cells() {
            let c = &cells[row * r.fx.grid().0 + col];
            assert_eq!(Some(&c.symbol), inputs[row * r.fx.grid().0 + col].as_ref(), "glyph cell ({col}, {row}) at 59.5");
            assert_eq!(c.fg, hexrgb(0xf3efe6), "glyph cell ({col}, {row}) not in hold at 59.5");
        }
        for (c, i) in cells.iter().zip(&inputs) {
            if i.is_none() {
                assert_eq!(c.symbol, HALF_BLOCK, "a fill cell became a glyph");
            }
        }
        r.play_to(61.0);
        let (share, total) = visible_share(&r);
        assert!(total > 0 && share >= 0.95, "{:.1}% of {total} characters visible at 61.0 ({fps} fps)", 100.0 * share);
        let min_contrast = r.engine_cells().iter().filter(|c| c.symbol != HALF_BLOCK).map(|c| luma(c.fg) - luma(c.bg)).fold(1.0, f64::min);
        assert!(min_contrast >= 0.438 - 1e-9, "contrast {min_contrast:.3} below the cap's guarantee");
    }
}

/// Every tick of the reveal and the dissolve, per glyph cell, from what the
/// engine shows. Glyph mode comes from the state index (a `▀` input symbol
/// is still a glyph), and the symbol is checked separately:
/// - a cell in glyph mode shows its own input symbol, and one in `▀` mode
///   shows `▀`;
/// - a glyph switches on or off only next to a flat cell of its plate;
/// - (`smooth`) the `▀` handoff moves pixels by ≤ 48 (plus the scene's own
///   change in the cell), the plate by ≤ 48 at a switch, the fg by ≤ 48 per
///   tick (`smooth_fg`), and the glyph-aware mean step stays under
///   `STEP_LIMIT`.
fn check_glyph_fades(r: &mut Run, smooth: bool, smooth_fg: bool, label: &str) {
    let cols = r.fx.grid().0;
    let slots: Vec<(usize, usize)> = r.fx.glyph_cells().iter().map(|&(c, rw, s)| (rw * cols + c, s)).collect();
    let inputs = r.input_cells();
    let b = text::HANDOFF_STATES;
    for (a, z) in [(58.0, 59.6), (62.4, 64.0)] {
        r.play_to(a - r.fx.dt());
        let mut prev = r.engine_cells();
        let mut prev_px = r.cell_pixels();
        let mut prev_s: Vec<usize> = slots.iter().map(|&(_, s)| r.fx.glyph_state_of(s)).collect();
        let mut prev_scene = pixbuf_rgb(r.fx.unquantised());
        while r.fx.time() < z - 1e-9 {
            if r.frame().is_none() {
                break;
            }
            let t = r.fx.time();
            let cur = r.engine_cells();
            let px = r.cell_pixels();
            let scene = pixbuf_rgb(r.fx.unquantised());
            if smooth {
                let (d, _) = frame_distance(&prev_px, &px);
                assert!(d < STEP_LIMIT, "{label}: glyph-aware step {d:.2} at {t:.3}");
            }
            for (k, &(idx, slot)) in slots.iter().enumerate() {
                let s = r.fx.glyph_state_of(slot);
                let (p, c) = (&prev[idx], &cur[idx]);
                let (pg, cg) = (prev_s[k] > b, s > b);
                let own = inputs[idx].as_deref().expect("glyph cells are input cells");
                assert_eq!(c.symbol, if cg { own } else { HALF_BLOCK }, "{label}: cell {idx} in state {s} shows the wrong symbol at {t:.3}");
                if !pg && cg {
                    assert_eq!(p.fg, p.bg, "{label}: a glyph appeared next to a non-flat cell at {t:.3} (cell {idx}, {} → {s})", prev_s[k]);
                }
                if pg && !cg {
                    assert_eq!(c.fg, c.bg, "{label}: a glyph vanished into a non-flat cell at {t:.3} (cell {idx}, {} → {s})", prev_s[k]);
                }
                if smooth {
                    if pg != cg {
                        assert!(dist(p.bg, c.bg) <= 48.0, "{label}: plate jumps {:.1} at a switch at {t:.3} (cell {idx})", dist(p.bg, c.bg));
                    }
                    let handoff = (1..=b).contains(&s) || (1..=b).contains(&prev_s[k]);
                    if !pg && !cg && handoff {
                        // The blend's own step, allowing for the scene's change in the
                        // cell: the plate follows the cell's mean, and its nearest
                        // plate entry can move twice as far as that mean.
                        let (row, col) = (idx / cols, idx % cols);
                        let moved_cell = (0..2).map(|h| dist(prev_scene[(2 * row + h) * cols + col], scene[(2 * row + h) * cols + col])).fold(0.0, f64::max);
                        for (x, y) in [(p.fg, c.fg), (p.bg, c.bg)] {
                            assert!(
                                dist(x, y) <= 48.0 + 2.0 * moved_cell,
                                "{label}: handoff pixel step {:.1} (scene moved {moved_cell:.1}) at {t:.3} (cell {idx}, state {} → {s})",
                                dist(x, y),
                                prev_s[k]
                            );
                        }
                    }
                    if pg && cg && smooth_fg {
                        assert!(dist(p.fg, c.fg) <= 48.0, "{label}: fg step {:.1} at {t:.3} (cell {idx}, state {} → {s})", dist(p.fg, c.fg), prev_s[k]);
                        assert!(dist(p.bg, c.bg) <= 48.0, "{label}: plate step {:.1} in glyph mode at {t:.3} (cell {idx})", dist(p.bg, c.bg));
                    }
                }
                prev_s[k] = s;
            }
            prev = cur;
            prev_px = px;
            prev_scene = scene;
        }
    }
}

#[test]
fn glyph_fades_are_continuous() {
    for fps in [60i64, 120] {
        let mut r = run(GRID_1080, fps, 1);
        check_glyph_fades(&mut r, true, true, &format!("fixture {fps} fps"));
        // A wide block over contrasting scenery: 11d's sky, horizon, ground
        // and tower, then scene 1's sky, tower and stack in the loop fade.
        let row = |k: usize| -> String { (0..110).map(|i| ["█", "▀", "▄", "o", "m", "a", "r", "c", "h", "y", "─"][(i + k) % 11]).collect() };
        let wide: String = (0..12).map(row).collect::<Vec<_>>().join("\n");
        let mut w = run_input(GRID_1080, fps, 1, config(57.9, 1.0, false), &wide);
        assert_eq!(w.fx.text_plan().tier, 0);
        check_glyph_fades(&mut w, true, true, &format!("wide {fps} fps"));
        // Accelerated playback (4× faster): the timing and the flat-cell
        // switches hold; per-tick steps are naturally 4× larger.
        let mut q = run_with(GRID_1080, fps, 1, config(57.9, 0.25, false));
        check_glyph_fades(&mut q, false, false, &format!("fixture {fps} fps ×0.25"));
        let mut q = run_with(GRID_1080, fps, 1, config(59.4, 0.25, false));
        q.play_to(59.5 + 2.0 * q.fx.dt());
        let (share, total) = visible_share(&q);
        assert_eq!(share, 1.0, "{fps} fps ×0.25: {:.1}% of {total} visible just after 59.5", 100.0 * share);
    }
}

/// A large single-symbol input: one glyph per cell of the whole grid, over
/// every part of the scene (the plate must still be stable).
#[test]
fn glyph_fades_full_grid() {
    for (grid, fps) in [(GRID_1080, 60i64), (GRID_4K, 120)] {
        let full: String = (0..grid.1).map(|_| "A".repeat(grid.0 as usize)).collect::<Vec<_>>().join("\n");
        let mut r = run_input(grid, fps, 1, config(57.9, 1.0, false), &full);
        assert_eq!(r.fx.text_plan().tier, 0);
        assert_eq!(r.fx.glyph_cells().len(), (grid.0 * grid.1) as usize);
        check_glyph_fades(&mut r, true, true, &format!("full grid {grid:?} {fps} fps"));
    }
}

/// `n` distinct single-width Private Use Area symbols, once each.
fn pua_text(n: usize, width: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        if i > 0 && i % width == 0 {
            s.push('\n');
        }
        let cp = if i < 6400 { 0xE000 + i as u32 } else { 0xF0000 + (i - 6400) as u32 };
        s.push(char::from_u32(cp).unwrap());
    }
    s
}

#[test]
fn branding_symbol_tiers() {
    for (n, grid, width, tier) in [(30, GRID_1080, 120, 1), (60, GRID_1080, 120, 2), (72, GRID_1080, 120, 3), (488, GRID_1080, 122, 5), (2048, GRID_4K, 256, 6)] {
        let input = pua_text(n, width);
        let mut r = run_input(grid, 60, 1, config(57.9, 1.0, false), &input);
        let plan = r.fx.text_plan();
        assert_eq!((plan.tier, plan.k_symbols), (tier, n), "n = {n}");
        check_glyph_fades(&mut r, true, tier <= 2, &format!("{n} symbols"));
        let mut r = run_input(grid, 60, 1, config(60.9, 1.0, false), &input);
        r.play_to(61.0);
        let (share, total) = visible_share(&r);
        assert_eq!(total, n);
        assert_eq!(share, 1.0, "n = {n}: {:.1}% visible", 100.0 * share);
        let glyphs: std::collections::HashSet<(String, [u8; 3], [u8; 3])> =
            r.engine_cells().into_iter().filter(|c| c.symbol != HALF_BLOCK).map(|c| (c.symbol, c.fg, c.bg)).collect();
        assert!(glyphs.len() <= text::text_appearance_bound(n + 1, n));
    }
    // Overflow: only the k most frequent symbols become glyphs.
    let n = 5200;
    let mut r = run_input(GRID_4K, 60, 1, config(60.9, 1.0, false), &pua_text(n, 260));
    let k = text::SYMBOL_BUDGET - (n + 1);
    assert_eq!(r.fx.text_plan().k_symbols, k);
    r.play_to(61.0);
    let (share, total) = visible_share(&r);
    assert_eq!(total, n);
    assert_eq!((share * n as f64).round() as usize, k, "exactly the k most frequent symbols are shown");
    assert!(share >= 0.95);
    // A full grid of distinct symbols: the engine's defaults use the budget, no glyphs.
    let full = pua_text(274 * 67, 274);
    let mut r = run_input(GRID_4K, 60, 1, config(60.9, 1.0, false), &full);
    assert_eq!(r.fx.text_plan().k_symbols, 0);
    r.play_to(61.0);
    assert!(r.engine_cells().iter().all(|c| c.symbol == HALF_BLOCK), "no glyphs in the full-grid overflow");
}

#[test]
fn branding_block_glyphs() {
    let input = "▀▀▀ ▄▄▄ ███\n█▄▀ ▀▄█ ▄█▀";
    for fps in [60i64, 120] {
        let mut r = run_input(GRID_1080, fps, 1, config(57.9, 1.0, false), input);
        check_glyph_fades(&mut r, true, true, &format!("blocks {fps} fps"));
        let mut r = run_input(GRID_1080, fps, 1, config(60.9, 1.0, false), input);
        r.play_to(61.0);
        let (share, total) = visible_share(&r);
        assert_eq!((share, total), (1.0, 18));
        let cells = r.engine_cells();
        for (col, row, _) in r.fx.glyph_cells() {
            assert_eq!(cells[row * r.fx.grid().0 + col].fg, hexrgb(0xf3efe6));
        }
    }
}

/// Dev aid (not run by default): effect tick time (step, compose, emit,
/// frame) over the branding for a full 4K grid of glyphs at 120 fps.
/// `cargo test --release --test starship branding_large_input_tick_time -- --ignored --nocapture`
#[test]
#[ignore]
fn branding_large_input_tick_time() {
    let full: String = (0..GRID_4K.1).map(|_| "A".repeat(GRID_4K.0 as usize)).collect::<Vec<_>>().join("\n");
    let mut r = run_input(GRID_4K, 120, 1, config(57.8, 1.0, false), &full);
    let mut ms = Vec::new();
    let mut changed = Vec::new();
    loop {
        let t0 = std::time::Instant::now();
        if r.frame().is_none() {
            break;
        }
        ms.push((t0.elapsed().as_secs_f64() * 1e3, r.fx.time()));
        changed.push(r.fx.changed_cells());
    }
    let mut v: Vec<f64> = ms.iter().map(|m| m.0).collect();
    v.sort_by(|a, b| a.total_cmp(b));
    let worst = ms.iter().fold((0.0, 0.0), |a, &m| if m.0 > a.0 { m } else { a });
    eprintln!("effect tick ms: p50 {:.2} p99 {:.2} max {:.2} at {:.3}s", v[v.len() / 2], v[v.len() * 99 / 100], worst.0, worst.1);
    for (a, b) in [(57.8, 58.0), (58.0, 59.5), (59.5, 62.5), (62.5, 63.7), (63.7, 64.0)] {
        let idx: Vec<usize> = (0..ms.len()).filter(|&i| ms[i].1 >= a && ms[i].1 < b).collect();
        let mut w: Vec<f64> = idx.iter().map(|&i| ms[i].0).collect();
        w.sort_by(|x, y| x.total_cmp(y));
        let ch = idx.iter().map(|&i| changed[i]).sum::<usize>() as f64 / idx.len().max(1) as f64;
        if !w.is_empty() {
            eprintln!("  {a}–{b}: p50 {:.2} max {:.2} changed cells/tick {ch:.0}", w[w.len() / 2], w[w.len() - 1]);
        }
    }
}
