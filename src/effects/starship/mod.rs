//! starship — a 64 s procedural animation of the Starship Flight 15 mission
//! (liftoff to the tower catch, then the branding). Full screen, old engine.
//!
//! The spec is aidocs/starship/storyboard.md. How it draws:
//! 1. Each tick the scene is drawn into a half-block pixel buffer
//!    (`cols × 2·rows`, `raster.rs`): geometry, gradients and particles.
//! 2. Every pixel is mapped to the fixed palette (`palette.rs`), and each
//!    cell's (top, bottom) pair becomes a `▀` with fg = top, bg = bottom.
//! 3. A shadow buffer of the previous pairs limits `set_appearance` to the
//!    cells that changed; the frame is `ctx.frame()`.
//!
//! The fixed palette is what keeps memory bounded: ttfx retains every distinct
//! appearance for the life of the process, and P×P is the most this effect can
//! ever emit (see palette.rs and tests/starship_pool_ceiling.rs).

pub mod palette;
pub mod particles;
pub mod raster;
pub mod scenes;
pub mod timeline;

use clap::Args;

use crate::engine::animation::{ExistingColorHandling, Visual};
use crate::engine::character::CharId;
use crate::engine::ctx::{EffectHooks, EngineCtx};
use crate::engine::effect::Effect;
use crate::engine::error::EngineError;
use crate::engine::events::EffectCallback;
use crate::engine::terminal::CharacterFilter;
use crate::utils::graphics::ColorPair;

use palette::{palette, PalIdx};
use raster::Pixbuf;
use scenes::World;
use timeline::{blend_weight, scene_at, Blend, SceneId, TOTAL_SECONDS};

/// The only symbol the effect draws: upper half block (fg = top pixel).
pub const HALF_BLOCK: &str = "▀";

pub const DURATION_SCALE_MIN: f64 = 0.25;
pub const DURATION_SCALE_MAX: f64 = 4.0;

/// Env var that turns on the CPU stress mode where args cannot reach the
/// effect (the screensaver and its benches parse the effect name only).
pub const STRESS_ENV: &str = "TTFX_STARSHIP_STRESS";

fn parse_duration_scale(s: &str) -> Result<f64, String> {
    let v: f64 = s.parse().map_err(|_| format!("invalid float value: '{s}'"))?;
    if v.is_finite() && (DURATION_SCALE_MIN..=DURATION_SCALE_MAX).contains(&v) {
        Ok(v)
    } else {
        Err(format!(
            "{s} is not a valid value. Argument must be a finite float in [{DURATION_SCALE_MIN}, {DURATION_SCALE_MAX}]."
        ))
    }
}

fn parse_start_at(s: &str) -> Result<f64, String> {
    let v: f64 = s.parse().map_err(|_| format!("invalid float value: '{s}'"))?;
    if v.is_finite() && (0.0..TOTAL_SECONDS).contains(&v) {
        Ok(v)
    } else {
        Err(format!(
            "{s} is not a valid value. Argument must be a finite number of seconds in [0, {TOTAL_SECONDS})."
        ))
    }
}

#[derive(Args, Debug, Clone)]
pub struct StarshipConfig {
    /// Start at this storyboard time in seconds (dev aid). The animation is
    /// simulated up to it, so the frame equals an uninterrupted run's.
    #[arg(long = "start-at", default_value_t = 0.0, value_parser = parse_start_at)]
    pub start_at: f64,

    /// Play the timeline slower (>1) or faster (<1); pacing only.
    #[arg(long = "duration-scale", default_value_t = 1.0, value_parser = parse_duration_scale)]
    pub duration_scale: f64,

    /// CPU stress mode: a plasma that changes every cell every tick plus a
    /// full particle pool (also TTFX_STARSHIP_STRESS=1).
    #[arg(long = "stress", default_value_t = false)]
    pub stress: bool,
}

impl Default for StarshipConfig {
    fn default() -> Self {
        StarshipConfig { start_at: 0.0, duration_scale: 1.0, stress: false }
    }
}

const NO_CHAR: CharId = CharId(u32::MAX);
const NO_KEY: u32 = u32::MAX;

pub struct Starship {
    config: StarshipConfig,
    cols: usize,
    rows: usize,
    /// Cell index (top-down, row-major) → the character drawing it.
    ids: Vec<CharId>,
    /// Last emitted (fg << 16 | bg) palette pair per cell.
    shadow: Vec<u32>,
    cur: Pixbuf,
    alt: Pixbuf,
    world: Option<World>,
    stress: bool,
    /// Next tick to emit, and the tick the simulation state is at.
    tick: u64,
    sim_tick: u64,
    total_ticks: u64,
    /// Storyboard seconds per tick.
    dt: f64,
    changed: usize,
    last_t: f64,
    /// Palette pair (fg·|P| + bg) → the visual `emit_appearance` interned for
    /// it. Only pointers already in the engine's pool are stored, so the cache
    /// adds no appearances; it skips the engine's per-call lookup on a hit.
    visuals: Vec<Option<Visual>>,
    cache_ok: bool,
}

impl Starship {
    pub fn new(config: StarshipConfig) -> Self {
        Starship {
            config,
            cols: 0,
            rows: 0,
            ids: Vec::new(),
            shadow: Vec::new(),
            cur: Pixbuf::new(0, 0),
            alt: Pixbuf::new(0, 0),
            world: None,
            stress: false,
            tick: 0,
            sim_tick: 0,
            total_ticks: 0,
            dt: 0.0,
            changed: 0,
            last_t: 0.0,
            visuals: Vec::new(),
            cache_ok: false,
        }
    }

    fn time_of(&self, tick: u64) -> f64 {
        tick as f64 * self.dt
    }

    fn advance_to(&mut self, ctx: &mut EngineCtx, tick: u64) {
        let world = self.world.as_mut().expect("built");
        while self.sim_tick < tick {
            let t0 = self.sim_tick as f64 * self.dt;
            self.sim_tick += 1;
            let t1 = self.sim_tick as f64 * self.dt;
            world.step(&mut ctx.rng, t0, t1, self.dt);
        }
    }

    fn compose(&mut self, t: f64) {
        let world = self.world.as_ref().expect("built");
        match blend_weight(t) {
            _ if self.stress => world.draw(SceneId::S1, t, &mut self.cur),
            None => world.draw(scene_at(t), t, &mut self.cur),
            Some(Blend::Fade { from, to, w }) => {
                world.draw(from, t, &mut self.cur);
                world.draw(to, t, &mut self.alt);
                self.cur.mix_with(&self.alt, w as f32);
            }
            Some(Blend::Loop { w }) => {
                world.draw(SceneId::B, t, &mut self.cur);
                world.draw_loop_target(&mut self.alt);
                self.cur.mix_with(&self.alt, w as f32);
            }
        }
    }

    fn emit(&mut self, ctx: &mut EngineCtx) {
        let pal = palette();
        let half = (pal.len() / 2) as u16;
        let flip = (self.tick & 1) as u16 * half;
        let w = self.cur.w;
        self.changed = 0;
        for row in 0..self.rows {
            for col in 0..self.cols {
                let idx = row * self.cols + col;
                let id = self.ids[idx];
                if id == NO_CHAR {
                    continue;
                }
                let mut fg = pal.nearest(self.cur.px[2 * row * w + col]);
                let mut bg = pal.nearest(self.cur.px[(2 * row + 1) * w + col]);
                if self.stress {
                    // Consecutive ticks use disjoint palette halves: every cell changes.
                    fg = PalIdx(fg.0 % half + flip);
                    bg = PalIdx(bg.0 % half + flip);
                }
                let key = (u32::from(fg.0) << 16) | u32::from(bg.0);
                if self.shadow[idx] != key {
                    self.shadow[idx] = key;
                    self.changed += 1;
                    let slot = fg.0 as usize * pal.len() + bg.0 as usize;
                    match self.visuals.get(slot).copied().flatten() {
                        Some(visual) => {
                            ctx.terminal.arena[id.0 as usize].animation.current_character_visual = visual;
                        }
                        _ => {
                            emit_appearance(ctx, id, fg, bg);
                            if self.cache_ok {
                                self.visuals[slot] =
                                    Some(ctx.terminal.arena[id.0 as usize].animation.current_character_visual);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Free the effect's buffers once the animation is over.
    fn release(&mut self) {
        self.world = None;
        self.visuals = Vec::new();
        self.shadow = Vec::new();
        self.ids = Vec::new();
        self.cur = Pixbuf::new(0, 0);
        self.alt = Pixbuf::new(0, 0);
    }

    // ---- test hooks -------------------------------------------------------

    /// Grid size in cells.
    pub fn grid(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// Storyboard time of the last emitted frame.
    pub fn time(&self) -> f64 {
        self.last_t
    }

    /// The total tick count, and the next tick to emit.
    pub fn ticks(&self) -> (u64, u64) {
        (self.total_ticks, self.tick)
    }

    /// The composed (pre-palette) pixel buffer of the last frame.
    pub fn unquantised(&self) -> &Pixbuf {
        &self.cur
    }

    /// The emitted pixel colours of the last frame (row-major, 2 per cell).
    pub fn pixels(&self) -> Vec<[u8; 3]> {
        let pal = palette();
        if self.shadow.is_empty() {
            return Vec::new();
        }
        let mut out = vec![[0u8; 3]; self.cols * self.rows * 2];
        for row in 0..self.rows {
            for col in 0..self.cols {
                let key = self.shadow[row * self.cols + col];
                if key == NO_KEY {
                    continue;
                }
                out[2 * row * self.cols + col] = pal.rgb_of(PalIdx((key >> 16) as u16));
                out[(2 * row + 1) * self.cols + col] = pal.rgb_of(PalIdx(key as u16));
            }
        }
        out
    }

    /// Cells whose appearance changed in the last frame.
    pub fn changed_cells(&self) -> usize {
        self.changed
    }

    pub fn live_particles(&self) -> usize {
        self.world.as_ref().map_or(0, |w| w.pool.len())
    }

    /// The live particles (empty once the effect has ended).
    pub fn particles(&self) -> &[particles::Particle] {
        self.world.as_ref().map_or(&[], |w| &w.pool.items)
    }

    /// Whether the effect still holds its frame buffers.
    pub fn holds_buffers(&self) -> bool {
        self.world.is_some() || !self.visuals.is_empty() || !self.cur.px.is_empty()
    }

    /// The simulation state (read-only; `None` once the effect has ended).
    pub fn world(&self) -> Option<&World> {
        self.world.as_ref()
    }

    /// The last frame's state drawn with one scene's layer only (no mix):
    /// the endpoints a cross-fade must match.
    pub fn render_only(&mut self, scene: SceneId) -> &Pixbuf {
        let world = self.world.as_ref().expect("built");
        world.draw(scene, self.last_t, &mut self.alt);
        &self.alt
    }
}

/// The single place the effect hands a cell appearance to the engine. Every
/// colour is a palette entry, so P×P bounds what the visual pool can hold.
pub fn emit_appearance(ctx: &mut EngineCtx, id: CharId, fg: PalIdx, bg: PalIdx) {
    let pal = palette();
    let colors = ColorPair::new(Some(pal.colors[fg.0 as usize]), Some(pal.colors[bg.0 as usize]));
    let ch = &mut ctx.terminal.arena[id.0 as usize];
    let uses_pre = ch.uses_input_preexisting_colors;
    ch.animation.set_appearance(&ch.input_symbol, uses_pre, Some(HALF_BLOCK), Some(colors));
}

impl EffectHooks for Starship {
    fn dispatch_callback(&mut self, _ctx: &mut EngineCtx, _character: CharId, _callback: &EffectCallback) {}
}

impl Effect for Starship {
    fn build(&mut self, ctx: &mut EngineCtx) -> Result<(), EngineError> {
        let canvas = &ctx.terminal.canvas;
        let (left, top) = (canvas.left, canvas.top);
        self.cols = canvas.width.max(1) as usize;
        self.rows = canvas.height.max(1) as usize;
        let cells = self.cols * self.rows;

        let all = CharacterFilter {
            input_chars: true,
            inner_fill_chars: true,
            outer_fill_chars: true,
            added_chars: false,
        };
        self.ids = vec![NO_CHAR; cells];
        for id in ctx.terminal.collect_characters(all) {
            let c = ctx.terminal.arena[id.0 as usize].motion.current_coord;
            let (col, row) = (c.column - left, top - c.row);
            if col >= 0 && row >= 0 && (col as usize) < self.cols && (row as usize) < self.rows {
                let slot = &mut self.ids[row as usize * self.cols + col as usize];
                debug_assert!(*slot == NO_CHAR, "two characters on one cell");
                *slot = id;
            }
        }
        for &id in self.ids.iter().filter(|&&id| id != NO_CHAR) {
            ctx.terminal.set_character_visibility(id, true);
        }
        self.shadow = vec![NO_KEY; cells];
        // The cached visual is valid for every cell only if set_appearance
        // would build it identically for each: same flags, no input colours.
        let flags = |id: CharId| {
            let a = &ctx.terminal.arena[id.0 as usize].animation;
            let pre = a.existing_color_handling == ExistingColorHandling::Always
                && ctx.terminal.arena[id.0 as usize].uses_input_preexisting_colors;
            (a.no_color, a.use_xterm_colors, pre)
        };
        let mut live = self.ids.iter().copied().filter(|&id| id != NO_CHAR);
        let first = live.next().map(flags);
        self.cache_ok = matches!(first, Some((_, _, false))) && live.all(|id| Some(flags(id)) == first);
        let n = palette().len();
        self.visuals = vec![None; if self.cache_ok { n * n } else { 0 }];
        self.cur = Pixbuf::new(self.cols, self.rows * 2);
        self.alt = Pixbuf::new(self.cols, self.rows * 2);

        let fps = match ctx.terminal.config.frame_rate {
            f if f > 0 => f as f64,
            _ => 60.0,
        };
        let scale = self.config.duration_scale.clamp(DURATION_SCALE_MIN, DURATION_SCALE_MAX);
        self.dt = 1.0 / (fps * scale);
        self.total_ticks = ((TOTAL_SECONDS * fps * scale).round() as u64).max(1);
        let start = ((self.config.start_at.max(0.0) * fps * scale).round() as u64).min(self.total_ticks - 1);

        self.stress = self.config.stress || std::env::var(STRESS_ENV).is_ok_and(|v| v == "1");
        self.world = Some(World::new(&mut ctx.rng, self.stress, self.cur.aspect()));
        self.tick = start;
        self.sim_tick = 0;
        palette();
        self.advance_to(ctx, start);
        Ok(())
    }

    fn next_frame(&mut self, ctx: &mut EngineCtx) -> Option<String> {
        if self.tick >= self.total_ticks {
            // The screensaver builds the next instance before it drops this
            // one, so give back everything the effect owns now: only the
            // engine's own state overlaps the rebuild.
            self.release();
            return None;
        }
        self.advance_to(ctx, self.tick);
        let t = self.time_of(self.tick);
        self.last_t = t;
        self.compose(t);
        self.emit(ctx);
        self.tick += 1;
        Some(ctx.frame())
    }
}
