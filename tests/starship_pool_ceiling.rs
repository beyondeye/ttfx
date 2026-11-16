//! The memory ceiling of the starship effect, measured, not estimated.
//!
//! ttfx keeps every distinct cell appearance for the life of the process. The
//! starship effect emits `▀` with a fg and bg from its fixed palette P (at most
//! P×P), and, in the branding reveal, glyph cells whose (fg, bg) come from the
//! text tier's fixed pairs (`text::reachable_pairs`). The engine itself also
//! interns one default visual per distinct input symbol. `text.rs` keeps the
//! glyph appearances plus those defaults within one budget.
//!
//! Each case runs **in its own process** (this binary re-executes itself), so
//! the thread-local visual pool starts empty, as in a fresh screensaver. A case
//! builds the effect at the 4K grid from its input, emits **every** pair of P×P
//! and every glyph pair of every glyph symbol through `emit_appearance` (the
//! function the frame loop uses), and plays to the end. Then it replays the
//! screensaver's rebuild (`omarchy-screensaver/src/engine.rs` `next_frame`:
//! the replacement is built **before** the old instance is dropped), plays it,
//! re-emits everything, and reports the peak heap footprint over a baseline
//! taken before any ttfx code ran.
//!
//! Footprint counts each allocation as glibc malloc lays it out (8-byte
//! header, 16-byte granules, 32-byte minimum chunk), so it tracks real heap
//! use rather than requested bytes. A case fails if its ceiling exceeds
//! `CEILING_MB`, or if the measurement exceeds the analytic model by more than
//! 25 %.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ttfx::effects::starship::palette::{palette, PalIdx, P_CAP};
use ttfx::effects::starship::text::{self, TIERS};
use ttfx::effects::starship::{emit_appearance, Glyph, Starship, StarshipConfig};
use ttfx::engine::animation::CharacterVisual;
use ttfx::engine::canvas::Anchor;
use ttfx::engine::character::CharId;
use ttfx::engine::ctx::{Clock, EngineCtx};
use ttfx::engine::effect::Effect;
use ttfx::engine::terminal::{CharacterFilter, TerminalConfig};
use ttfx::utils::rng::Rng;

const CEILING_MB: f64 = 64.0;
const GRID: (i64, i64) = (274, 67);
const CASE_ENV: &str = "STARSHIP_CEILING_CASE";

struct Footprint;

static COUNTING: AtomicBool = AtomicBool::new(true);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);

fn chunk(size: usize) -> u64 {
    ((size + 8 + 15) & !15).max(32) as u64
}

unsafe impl GlobalAlloc for Footprint {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() && COUNTING.load(Ordering::Relaxed) {
            let live = LIVE.fetch_add(chunk(layout.size()), Ordering::Relaxed) + chunk(layout.size());
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        if COUNTING.load(Ordering::Relaxed) {
            LIVE.fetch_sub(chunk(layout.size()), Ordering::Relaxed);
        }
        unsafe { System.dealloc(p, layout) };
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, new_size) };
        if !q.is_null() && COUNTING.load(Ordering::Relaxed) {
            // Old and new blocks briefly coexist when realloc moves.
            let live = LIVE.fetch_add(chunk(new_size), Ordering::Relaxed) + chunk(new_size);
            PEAK.fetch_max(live, Ordering::Relaxed);
            LIVE.fetch_sub(chunk(layout.size()), Ordering::Relaxed);
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Footprint = Footprint;

fn mb(b: u64) -> f64 {
    b as f64 / (1024.0 * 1024.0)
}

/// Run `f` with the counter off (the measurement's own bookkeeping).
fn uncounted<T>(f: impl FnOnce() -> T) -> T {
    COUNTING.store(false, Ordering::Relaxed);
    let out = f();
    COUNTING.store(true, Ordering::Relaxed);
    out
}

fn screensaver_ctx(input: &str, seed: u64) -> EngineCtx {
    let config = TerminalConfig {
        frame_rate: 120,
        canvas_width: GRID.0,
        canvas_height: GRID.1,
        ignore_terminal_dimensions: true,
        reuse_canvas: true,
        anchor_canvas: Anchor::C,
        anchor_text: Anchor::C,
        ..TerminalConfig::default()
    };
    EngineCtx::new(input, config, Rng::seeded(seed), Clock::virtual_with_frame_rate(120)).expect("engine ctx")
}

/// The i-th distinct single-width symbol: the Private Use Area, then PUA-A.
fn pua(i: usize) -> char {
    let cp = if i < 6400 { 0xE000 + i as u32 } else { 0xF0000 + (i - 6400) as u32 };
    char::from_u32(cp).expect("PUA code point")
}

/// `n` distinct symbols, once each, in lines of at most `width`.
fn generated(n: usize, width: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        if i > 0 && i % width == 0 {
            s.push('\n');
        }
        s.push(pua(i));
    }
    s
}

struct Case {
    name: &'static str,
    input: fn() -> String,
    /// The expected tier, and whether every glyph symbol is drawn.
    tier: usize,
    full: bool,
}

const CASES: &[Case] = &[
    Case { name: "fixture", input: || include_str!("fixtures/starship_input.txt").to_string(), tier: 0, full: true },
    Case { name: "t0_max", input: || generated(17, 17), tier: 0, full: true },
    Case { name: "t4_saturate", input: || generated(487, 260), tier: 4, full: true },
    Case { name: "max_supported", input: || generated(5119, 270), tier: 6, full: true },
    Case { name: "overflow_full_grid", input: || generated(GRID.0 as usize * GRID.1 as usize, GRID.0 as usize), tier: 6, full: false },
];

/// One id per glyph symbol (an input character showing it).
fn ids_by_symbol(ctx: &EngineCtx, symbols: &[String]) -> Vec<CharId> {
    let input = CharacterFilter { input_chars: true, inner_fill_chars: false, outer_fill_chars: false, added_chars: false };
    let mut first: HashMap<&str, CharId> = HashMap::new();
    for id in ctx.terminal.collect_characters(input) {
        first.entry(ctx.terminal.arena[id.0 as usize].input_symbol.as_str()).or_insert(id);
    }
    symbols.iter().map(|s| first[s.as_str()]).collect()
}

/// Emit P×P on `pp_id` and every glyph pair on each glyph symbol's id;
/// return the visual pointers (when `collect`).
fn emit_all(ctx: &mut EngineCtx, pp_id: CharId, glyph_ids: &[CharId], tier: usize, collect: bool) -> Vec<usize> {
    let n = palette().len();
    let pairs = uncounted(|| text::reachable_pairs(tier));
    let mut ptrs: Vec<usize> = uncounted(|| Vec::with_capacity(if collect { n * n + glyph_ids.len() * pairs.len() } else { 0 }));
    let record = |ctx: &EngineCtx, id: CharId, ptrs: &mut Vec<usize>| {
        if collect {
            let v = ctx.terminal.arena[id.0 as usize].animation.current_character_visual;
            uncounted(|| ptrs.push(v as *const CharacterVisual as usize));
        }
    };
    for fg in 0..n {
        for bg in 0..n {
            emit_appearance(ctx, pp_id, Glyph::HalfBlock, PalIdx(fg as u16), PalIdx(bg as u16));
            record(ctx, pp_id, &mut ptrs);
        }
    }
    for &id in glyph_ids {
        for &(fg, bg) in &pairs {
            emit_appearance(ctx, id, Glyph::Input, fg, bg);
            record(ctx, id, &mut ptrs);
        }
    }
    uncounted(|| drop(pairs));
    ptrs
}

fn run_case(case: &Case) {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let input = uncounted(|| (case.input)());

    // The old instance: started near its end so it reaches `None` quickly.
    let mut ctx = screensaver_ctx(&input, 1);
    let mut fx = Starship::new(StarshipConfig { start_at: 63.9, ..StarshipConfig::default() });
    fx.build(&mut ctx).expect("build starship");
    let build_one = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);

    let pal = palette();
    let n = pal.len();
    assert!(n <= P_CAP, "palette has {n} entries, cap {P_CAP}");
    let plan = fx.text_plan();
    let n_distinct = uncounted(|| text::count_symbols(ctx.terminal.arena.iter().map(|c| c.input_symbol.as_str())).len());
    let n_input = uncounted(|| {
        let input = CharacterFilter { input_chars: true, inner_fill_chars: false, outer_fill_chars: false, added_chars: false };
        let ids = ctx.terminal.collect_characters(input);
        text::count_symbols(ids.iter().map(|id| ctx.terminal.arena[id.0 as usize].input_symbol.as_str())).len()
    });
    let symbols = uncounted(|| fx.glyph_symbols(&ctx));
    let glyph_ids = uncounted(|| ids_by_symbol(&ctx, &symbols));
    assert_eq!(plan.tier, case.tier, "{}: tier", case.name);
    if case.full {
        assert_eq!(symbols.len(), n_input, "{}: every symbol is a glyph within the scope", case.name);
    } else {
        assert_eq!(symbols.len(), plan.k_symbols, "{}: overflow draws exactly k symbols", case.name);
    }
    let all = CharacterFilter { input_chars: true, inner_fill_chars: true, outer_fill_chars: true, added_chars: false };
    let pp_id = ctx.terminal.collect_characters(all)[0];

    let before = LIVE.load(Ordering::Relaxed);
    let mut ptrs = emit_all(&mut ctx, pp_id, &glyph_ids, plan.tier, true);
    let emitted = LIVE.load(Ordering::Relaxed).saturating_sub(before);

    // Play the old instance to its end; it releases its own buffers.
    let mut frames = 0;
    while fx.next_frame(&mut ctx).is_some() {
        frames += 1;
    }
    assert!(frames >= 1, "the old instance played");
    assert!(!fx.holds_buffers(), "the ended instance must release its buffers before the rebuild");

    // The rebuild: built while the old one still exists, then plays 2 s.
    let before_new = LIVE.load(Ordering::Relaxed);
    let mut ctx2 = screensaver_ctx(&input, 1);
    let mut fx2 = Starship::new(StarshipConfig::default());
    fx2.build(&mut ctx2).expect("rebuild starship");
    let build_two = LIVE.load(Ordering::Relaxed).saturating_sub(before_new);
    for _ in 0..240 {
        fx2.next_frame(&mut ctx2).expect("frame");
    }
    // Everything again, on the new instance: all must already be pooled.
    let pp_id2 = ctx2.terminal.collect_characters(all)[0];
    let glyph_ids2 = uncounted(|| ids_by_symbol(&ctx2, &symbols));
    let before_again = LIVE.load(Ordering::Relaxed);
    emit_all(&mut ctx2, pp_id2, &glyph_ids2, plan.tier, false);
    let regrowth = LIVE.load(Ordering::Relaxed).saturating_sub(before_again);
    drop(fx);
    drop(ctx);
    let peak = PEAK.load(Ordering::Relaxed);
    COUNTING.store(false, Ordering::Relaxed);

    ptrs.sort_unstable();
    ptrs.dedup();
    let distinct = ptrs.len();
    let glyph_pairs = text::reachable_pairs(plan.tier).len();
    // A `▀` glyph symbol's pairs are already P×P pairs.
    let glyph_new = symbols.iter().filter(|s| s.as_str() != "▀").count() * glyph_pairs;

    // Analytic model: P×P and the glyph pairs (visual + symbol + two colour
    // codes), the engine's default visual per distinct symbol (both builds
    // include it), the visual set table, and both instances' engine state.
    let visual = chunk(std::mem::size_of::<CharacterVisual>());
    let symbol = chunk(4);
    let codes = 2 * chunk(6);
    let appearances = (n * n + glyph_new) as u64;
    let set_slots = ((appearances as usize + n_distinct) * 8 / 7).next_power_of_two() as u64;
    let set_table = chunk(set_slots as usize * 8 + set_slots as usize + 16);
    let model = appearances * (visual + symbol + codes) + set_table + set_table / 2 + build_one + build_two;
    let ceiling = peak.saturating_sub(baseline);
    println!(
        "starship-pool-ceiling case={} palette={n} n_distinct={n_distinct} tier=T{} glyph_symbols={} glyph_pairs={} \
         text_appearances={} budget={} build_mb={:.2} rebuild_mb={:.2} emitted_mb={:.2} ceiling_mb={:.2} model_mb={:.2} \
         ratio={:.3} regrowth_bytes={regrowth} limit_mb={CEILING_MB}",
        case.name,
        plan.tier,
        symbols.len(),
        glyph_pairs,
        symbols.len() * glyph_pairs,
        text::SYMBOL_BUDGET,
        mb(build_one),
        mb(build_two),
        mb(emitted),
        mb(ceiling),
        mb(model),
        ceiling as f64 / model as f64,
    );

    assert!(glyph_pairs <= TIERS[plan.tier].cost(), "{}: tier pairs over the tier cost", case.name);
    assert!(
        n_distinct + symbols.len() * glyph_pairs <= text::SYMBOL_BUDGET.max(n_distinct),
        "{}: engine defaults + glyph appearances over the budget",
        case.name
    );
    assert_eq!(distinct, n * n + glyph_new, "{}: every pair must intern a distinct visual", case.name);
    assert_eq!(regrowth, 0, "{}: re-emitting must not grow the heap", case.name);
    assert!(mb(ceiling) <= CEILING_MB, "{}: ceiling {:.2} MB exceeds {CEILING_MB} MB", case.name, mb(ceiling));
    assert!(
        (ceiling as f64) <= model as f64 * 1.25,
        "{}: measured ceiling {:.2} MB is over 1.25x the model {:.2} MB: a cost the model misses",
        case.name,
        mb(ceiling),
        mb(model)
    );
    drop(fx2);
    drop(ctx2);
}

fn main() {
    if let Ok(name) = std::env::var(CASE_ENV) {
        let case = CASES.iter().find(|c| c.name == name).expect("known case");
        run_case(case);
        println!("ok {name}");
        return;
    }
    // Each case in a fresh process, so its visual pool starts empty.
    let exe = std::env::current_exe().expect("own path");
    let mut failed = Vec::new();
    for case in CASES {
        let out = std::process::Command::new(&exe).env(CASE_ENV, case.name).output().expect("run case");
        print!("{}", String::from_utf8_lossy(&out.stdout));
        eprint!("{}", String::from_utf8_lossy(&out.stderr));
        if !out.status.success() {
            failed.push(case.name);
        }
    }
    assert!(failed.is_empty(), "ceiling cases failed: {failed:?}");
    println!("ok");
}
