//! The memory ceiling of the starship effect, measured, not estimated.
//!
//! ttfx keeps every distinct cell appearance for the life of the process. The
//! starship effect only ever emits `▀` with a fg and bg from its fixed
//! palette P, so P×P is the complete set it can make the engine retain, for
//! any seed and any number of loops. This binary (its own process, no libtest
//! threads, so the thread-local pool starts empty) builds the effect at the 4K
//! grid, then emits **every** pair of P×P through `emit_appearance` (the same
//! function the effect's frame loop uses) and plays it to its end. Then it
//! replays the screensaver's rebuild (`omarchy-screensaver/src/engine.rs`
//! `next_frame`: the replacement is built **before** the old instance is
//! dropped), plays the new instance, and reports the peak heap footprint over
//! a baseline taken before any ttfx code ran. That overlap is the worst case.
//!
//! Footprint counts each allocation as glibc malloc lays it out (8-byte header,
//! 16-byte granules, 32-byte minimum chunk), so it tracks real heap use rather
//! than requested bytes. It fails if the ceiling exceeds `CEILING_MB`, or if the
//! measurement exceeds the analytic model by more than 25 %.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ttfx::effects::starship::palette::{palette, PalIdx, P_CAP};
use ttfx::effects::starship::{emit_appearance, Starship, StarshipConfig};
use ttfx::engine::animation::CharacterVisual;
use ttfx::engine::canvas::Anchor;
use ttfx::engine::ctx::{Clock, EngineCtx};
use ttfx::engine::effect::Effect;
use ttfx::engine::terminal::{CharacterFilter, TerminalConfig};
use ttfx::utils::rng::Rng;

const CEILING_MB: f64 = 64.0;

struct Footprint;

static COUNTING: AtomicBool = AtomicBool::new(true);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);
static ALLOCS: AtomicU64 = AtomicU64::new(0);

fn chunk(size: usize) -> u64 {
    ((size + 8 + 15) & !15).max(32) as u64
}

unsafe impl GlobalAlloc for Footprint {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() && COUNTING.load(Ordering::Relaxed) {
            let live = LIVE.fetch_add(chunk(layout.size()), Ordering::Relaxed) + chunk(layout.size());
            PEAK.fetch_max(live, Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
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
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Footprint = Footprint;

fn mb(b: u64) -> f64 {
    b as f64 / (1024.0 * 1024.0)
}

fn screensaver_ctx(cols: i64, rows: i64, seed: u64) -> EngineCtx {
    let config = TerminalConfig {
        frame_rate: 120,
        canvas_width: cols,
        canvas_height: rows,
        ignore_terminal_dimensions: true,
        reuse_canvas: true,
        anchor_canvas: Anchor::C,
        anchor_text: Anchor::C,
        ..TerminalConfig::default()
    };
    EngineCtx::new(include_str!("fixtures/starship_input.txt"), config, Rng::seeded(seed), Clock::virtual_with_frame_rate(120))
        .expect("engine ctx")
}

fn main() {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);

    let (cols, rows) = (274, 67);
    // The old instance: started near its end so it reaches `None` quickly.
    let mut ctx = screensaver_ctx(cols, rows, 1);
    let mut fx = Starship::new(StarshipConfig { start_at: 63.9, ..StarshipConfig::default() });
    fx.build(&mut ctx).expect("build starship");
    let build_one = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);

    let pal = palette();
    let n = pal.len();
    assert!(n <= P_CAP, "palette has {n} entries, cap {P_CAP}");
    let all = CharacterFilter { input_chars: true, inner_fill_chars: true, outer_fill_chars: true, added_chars: false };
    let id = ctx.terminal.collect_characters(all)[0];

    // Pointer bookkeeping is the measurement's own; keep it out of the count.
    COUNTING.store(false, Ordering::Relaxed);
    let mut ptrs: Vec<usize> = Vec::with_capacity(n * n);
    COUNTING.store(true, Ordering::Relaxed);
    let before_pairs = LIVE.load(Ordering::Relaxed);
    for fg in 0..n {
        for bg in 0..n {
            emit_appearance(&mut ctx, id, PalIdx(fg as u16), PalIdx(bg as u16));
            let v = ctx.terminal.arena[id.0 as usize].animation.current_character_visual;
            COUNTING.store(false, Ordering::Relaxed);
            ptrs.push(v as *const CharacterVisual as usize);
            COUNTING.store(true, Ordering::Relaxed);
        }
    }
    let after_pairs = LIVE.load(Ordering::Relaxed);

    // Play the old instance to its end; it releases its own buffers.
    let mut frames = 0;
    while fx.next_frame(&mut ctx).is_some() {
        frames += 1;
    }
    assert!(frames >= 1, "the old instance played");
    assert!(!fx.holds_buffers(), "the ended instance must release its buffers before the rebuild");

    // The rebuild: the replacement is built while the old one still exists,
    // exactly as the screensaver does, then plays two seconds of frames.
    let before_new = LIVE.load(Ordering::Relaxed);
    let mut ctx2 = screensaver_ctx(cols, rows, 1);
    let mut fx2 = Starship::new(StarshipConfig::default());
    fx2.build(&mut ctx2).expect("rebuild starship");
    let build_two = LIVE.load(Ordering::Relaxed).saturating_sub(before_new);
    for _ in 0..240 {
        fx2.next_frame(&mut ctx2).expect("frame");
    }
    // Every pair again, on the new instance: all must already be pooled.
    let id2 = ctx2.terminal.collect_characters(all)[0];
    let before_again = LIVE.load(Ordering::Relaxed);
    for fg in 0..n {
        for bg in 0..n {
            emit_appearance(&mut ctx2, id2, PalIdx(fg as u16), PalIdx(bg as u16));
        }
    }
    let regrowth = LIVE.load(Ordering::Relaxed).saturating_sub(before_again);
    let overlap = LIVE.load(Ordering::Relaxed);
    drop(fx);
    drop(ctx);
    let end = LIVE.load(Ordering::Relaxed);
    let peak = PEAK.load(Ordering::Relaxed);
    COUNTING.store(false, Ordering::Relaxed);

    ptrs.sort_unstable();
    ptrs.dedup();
    let distinct = ptrs.len();

    // Analytic model of the worst case: P×P, plus the old instance's engine
    // state (its effect buffers are released) and a full new instance.
    let pairs = (n * n) as u64;
    let visual = chunk(std::mem::size_of::<CharacterVisual>());
    let symbol = chunk("▀".len());
    // fg and bg colour codes: `ColorCode::Rgb(String)` of 6 hex digits each.
    let codes = 2 * chunk(6);
    let set_slots = ((n * n) * 8 / 7).next_power_of_two() as u64;
    let set_table = chunk(set_slots as usize * 8 + set_slots as usize + 16);
    // Peak while the table doubles: the new table plus the old (half) one.
    let model_pairs = pairs * (visual + symbol + codes) + set_table + set_table / 2;
    let model = model_pairs + build_one + build_two;

    let ceiling = peak.saturating_sub(baseline);
    println!(
        "starship-pool-ceiling palette={n} pairs={pairs} distinct_visuals={distinct} size_of_visual={} \
         build_mb={:.2} rebuild_mb={:.2} pairs_mb={:.2} overlap_mb={:.2} final_mb={:.2} ceiling_mb={:.2} model_mb={:.2} ratio={:.3} regrowth_bytes={regrowth} limit_mb={CEILING_MB}",
        std::mem::size_of::<CharacterVisual>(),
        mb(build_one),
        mb(build_two),
        mb(after_pairs.saturating_sub(before_pairs)),
        mb(overlap.saturating_sub(baseline)),
        mb(end.saturating_sub(baseline)),
        mb(ceiling),
        mb(model),
        ceiling as f64 / model as f64,
    );

    assert_eq!(distinct, n * n, "every palette pair must intern a distinct visual");
    assert_eq!(regrowth, 0, "re-emitting P×P must not grow the heap");
    assert!(mb(ceiling) <= CEILING_MB, "ceiling {:.2} MB exceeds {CEILING_MB} MB", mb(ceiling));
    assert!(
        (ceiling as f64) <= model as f64 * 1.25,
        "measured ceiling {:.2} MB is over 1.25x the model {:.2} MB: a cost the model misses",
        mb(ceiling),
        mb(model)
    );
    drop(fx2);
    drop(ctx2);
    println!("ok");
}
