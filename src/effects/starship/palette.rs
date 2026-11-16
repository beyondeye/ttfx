//! The fixed output palette: every colour the effect can emit.
//!
//! ttfx keeps every distinct cell appearance (symbol + colours) for the rest
//! of the process (`CharacterVisual::interned`), and the screensaver re-seeds
//! the effect on every loop. Emitting free RGB would therefore grow memory
//! without bound. Instead every fg and bg is an entry of this palette `P`:
//! it is built from constants only (no seed, no grid), so the reachable set
//! of `▀` appearances is at most `|P|²` for any seed and any number of loops.
//! The branding's glyph cells (text.rs) use small fixed subsets of P under
//! their own budget. `tests/starship_pool_ceiling.rs` saturates both and
//! measures the bytes they cost.
//!
//! Contents: the storyboard's sampled colours (aidocs/starship/storyboard.md),
//! the gradient ramps each scene declares, the branding text ramp and a grey
//! ramp for blends that fall between them.

use std::sync::OnceLock;

use crate::utils::graphics::Color;

/// Upper bound on `|P|`. The worst case is the screensaver's rebuild: the
/// replacement is built while the ended instance (and the full pool) still
/// exist. Measured: about 320 B per P×P pair plus two engine instances at 4K
/// (about 12 MB each), plus the branding budget (text.rs, ≈ 3.3 MB) or, for
/// a full 4K grid of distinct input symbols, the engine's own per-symbol
/// visuals (≈ 4.5 MB). At |P| = 317 the worst case measures 62.9 MB, under
/// the 64 MB gate, so the cap leaves 3 entries of headroom.
/// `tests/starship_pool_ceiling.rs` measures that lifecycle with the real
/// palette and is the authority; this cap only fails fast at build.
pub const P_CAP: usize = 320;

/// Index into the palette. The only colour type the emitter accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PalIdx(pub u16);

/// Colours sampled for the storyboard, one per line.
const STORYBOARD: &[u32] = &[
    0x000000,
    0x010205,
    0x094287,
    0x0a397b,
    0x0d0e15,
    0x0d0f16,
    0x0d1725,
    0x0e1826,
    0x0e448a,
    0x110d16,
    0x11171b,
    0x121b2d,
    0x191421,
    0x1e3d4e,
    0x1f2636,
    0x222736,
    0x23465a,
    0x255687,
    0x262734,
    0x272929,
    0x2b4e77,
    0x2c507a,
    0x2d2633,
    0x2d2c22,
    0x2f2e2b,
    0x305581,
    0x313a56,
    0x355e8e,
    0x37352b,
    0x392d3a,
    0x3b6187,
    0x3b70a7,
    0x406e88,
    0x414550,
    0x424554,
    0x444348,
    0x44709b,
    0x454448,
    0x477db9,
    0x4887ce,
    0x497197,
    0x4b483d,
    0x4f6271,
    0x524c3e,
    0x56544a,
    0x565b67,
    0x5897d9,
    0x5a8bbf,
    0x5d6570,
    0x60615a,
    0x61758e,
    0x62263c,
    0x635b4d,
    0x694f5c,
    0x6a6054,
    0x6f95ab,
    0x799298,
    0x7b8894,
    0x7d8488,
    0x879299,
    0x8d7581,
    0x8d9da6,
    0x8f785f,
    0x999fa8,
    0x99abb7,
    0x9a8d7c,
    0x9e7f60,
    0x9e9786,
    0x9f9c94,
    0xa3a39b,
    0xa692a0,
    0xa8a69e,
    0xaaa197,
    0xada79e,
    0xafaaa1,
    0xafaca3,
    0xb19e89,
    0xb2a99e,
    0xb3a086,
    0xb8c9d5,
    0xc3b2b9,
    0xc3b8ac,
    0xc4bcb0,
    0xc7bbab,
    0xc7c0b4,
    0xc8bcad,
    0xc9d2d9,
    0xcfdce6,
    0xd0b3bd,
    0xd5e5e9,
    0xd9c9b3,
    0xdacbad,
    0xddedf7,
    0xdf9e54,
    0xe2af7b,
    0xe2ecee,
    0xe3edef,
    0xe6e1db,
    0xe7bb68,
    0xeaebe4,
    0xf1e9d3,
    0xf3d592,
    0xf3efe6,
    0xfafaf7,
    0xfafcfb,
];

/// A gradient a scene draws: `steps` evenly spaced colours from `a` to `b`
/// (both ends included).
#[derive(Debug, Clone, Copy)]
pub struct Ramp {
    pub a: u32,
    pub b: u32,
    pub steps: usize,
}

const fn ramp(a: u32, b: u32, steps: usize) -> Ramp {
    Ramp { a, b, steps }
}

/// Scene 1 (liftoff) gradients: sky, haze, smoke, flame and the aerial ground.
pub const RAMPS_S1: &[Ramp] = &[
    ramp(0x23465a, 0x406e88, 16),
    ramp(0x406e88, 0x799298, 8),
    ramp(0x1e3d4e, 0x23465a, 6),
    ramp(0xdacbad, 0xb3a086, 8),
    ramp(0xb3a086, 0x8f785f, 8),
    ramp(0xf3d592, 0xe2af7b, 6),
    ramp(0xe2af7b, 0x9e7f60, 6),
    ramp(0x272929, 0x11171b, 6),
    ramp(0xd9c9b3, 0xb19e89, 6),
];

/// Scene 2 (ascent) gradients: the sky, the horizon haze and the plume.
pub const RAMPS_S2: &[Ramp] = &[
    ramp(0x2c507a, 0x305581, 6),
    ramp(0x305581, 0x3b6187, 6),
    ramp(0x3b6187, 0x4f6271, 6),
    ramp(0xd0b3bd, 0xa692a0, 4),
    ramp(0xc3b2b9, 0x7d8488, 6),
    ramp(0x1f3f66, 0x2c507a, 6),
    ramp(0x3b6187, 0x7b8894, 6),
    ramp(0x7b8894, 0x4f6271, 4),
    ramp(0x4f6271, 0x1e3550, 6),
    ramp(0x1e3550, 0x272929, 4),
];

/// Scene 3 (onboard) gradients: sea, land, hull and the engine fire.
pub const RAMPS_S3: &[Ramp] = &[
    ramp(0x255687, 0x44709b, 6),
    ramp(0x44709b, 0x3b70a7, 4),
    ramp(0x355e8e, 0x44709b, 4),
    ramp(0x5d6570, 0x44709b, 6),
    ramp(0x262734, 0x454448, 6),
    ramp(0x454448, 0x5d6570, 6),
    ramp(0x525f6e, 0x3a3a42, 3),
    ramp(0x255687, 0x497197, 6),
    ramp(0xf1e9d3, 0xdf9e54, 6),
    ramp(0xdf9e54, 0x999fa8, 6),
];

/// Scene 4 (high altitude): the navy sky into the plume glow, the plume, the cone.
pub const RAMPS_S4: &[Ramp] = &[
    ramp(0x0d1725, 0x392d3a, 6),
    ramp(0x392d3a, 0x694f5c, 4),
    ramp(0x694f5c, 0xe6e1db, 6),
    ramp(0x0d1725, 0x414550, 4),
];

/// Scene 5 (hot staging): sky, atmosphere band, vapour, hull shadow, booster glow.
pub const RAMPS_S5: &[Ramp] = &[
    ramp(0x313a56, 0x094287, 4),
    ramp(0x094287, 0x5a8bbf, 6),
    ramp(0x5a8bbf, 0xc9d2d9, 4),
    ramp(0x1f2636, 0x0d0e15, 4),
    ramp(0xe7bb68, 0x8d7581, 3),
];

/// Scene 6 (coast): sky, limb band, glare, hull shadow.
pub const RAMPS_S6: &[Ramp] = &[
    ramp(0x0a397b, 0x0e448a, 3),
    ramp(0x0e448a, 0x477db9, 5),
    ramp(0x477db9, 0x5897d9, 3),
    ramp(0x5897d9, 0xb8c9d5, 4),
    ramp(0x222736, 0x0d0f16, 4),
];

/// Scene 9 (ship over Earth): plum hull, ocean, clouds, the sun's glare into space.
pub const RAMPS_S9: &[Ramp] = &[
    ramp(0x2d2633, 0x110d16, 4),
    ramp(0x191421, 0x61758e, 4),
    ramp(0x61758e, 0x99abb7, 4),
    ramp(0xddedf7, 0x191421, 6),
    ramp(0x2d2633, 0x3d3446, 3),
];

/// Scene 10 (SECO): the haze from black space to its bright core, the grey hull.
pub const RAMPS_S10: &[Ramp] = &[
    ramp(0x010205, 0x121b2d, 4),
    ramp(0x121b2d, 0x565b67, 5),
    ramp(0x565b67, 0xd5e5e9, 6),
    ramp(0x444348, 0x010205, 4),
];

/// Scene 11 (catch): overcast sky, ground, tower lattice, ship steel, steam.
pub const RAMPS_S11: &[Ramp] = &[
    ramp(0xaaa197, 0xc3b8ac, 3),
    ramp(0xa8a69e, 0x9f9c94, 3),
    ramp(0x74695c, 0x463f33, 3),
    ramp(0x2d2c22, 0x37352b, 3),
    ramp(0x9a8d7c, 0xc7bbab, 4),
    ramp(0xc4bcb0, 0xafaaa1, 3),
    ramp(0xc8bcad, 0x9e9786, 4),
    ramp(0x56544a, 0xa3a39b, 4),
];

/// The branding text ramp F (`text.rs`): dim → smoke → text colour. Every
/// step is an exact palette entry.
pub const RAMPS_TEXT: &[Ramp] = &[ramp(0x6e6c66, 0xafaca3, 4), ramp(0xafaca3, 0xf3efe6, 5)];

/// Every scene's declared ramps, plus the branding text ramp.
pub const ALL_RAMPS: &[&[Ramp]] =
    &[RAMPS_S1, RAMPS_S2, RAMPS_S3, RAMPS_S4, RAMPS_S5, RAMPS_S6, RAMPS_S9, RAMPS_S10, RAMPS_S11, RAMPS_TEXT];

const GREY_STEPS: usize = 16;

/// The colours a ramp contributes to the palette, in order (both ends included).
pub fn ramp_colors(r: &Ramp) -> Vec<[u8; 3]> {
    let (a, b) = (rgb_of(r.a), rgb_of(r.b));
    let n = r.steps.max(2);
    (0..n)
        .map(|i| {
            let w = i as f64 / (n - 1) as f64;
            [lerp_u8(a[0], b[0], w), lerp_u8(a[1], b[1], w), lerp_u8(a[2], b[2], w)]
        })
        .collect()
}

/// RGB bytes of a `0xrrggbb` constant.
pub fn rgb_of(hex: u32) -> [u8; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

fn lerp_u8(a: u8, b: u8, w: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * w).round().clamp(0.0, 255.0) as u8
}

/// The palette, its RGB555 lookup table and the emitted `Color` per entry.
pub struct Palette {
    pub rgb: Vec<[u8; 3]>,
    pub colors: Vec<Color>,
    /// RGB555 (r5 << 10 | g5 << 5 | b5) → nearest palette index.
    lut: Vec<u16>,
}

impl Palette {
    fn build() -> Palette {
        let mut rgb: Vec<[u8; 3]> = Vec::new();
        rgb.extend(STORYBOARD.iter().map(|&h| rgb_of(h)));
        for ramps in ALL_RAMPS {
            for r in ramps.iter() {
                rgb.extend(ramp_colors(r));
            }
        }
        for i in 0..GREY_STEPS {
            let g = lerp_u8(0, 255, i as f64 / (GREY_STEPS - 1) as f64);
            rgb.push([g, g, g]);
        }
        rgb.sort_unstable();
        rgb.dedup();
        assert!(rgb.len() <= P_CAP, "starship palette has {} entries, cap is {P_CAP}", rgb.len());

        let colors = rgb.iter().map(|c| Color::from_rgb(c[0], c[1], c[2])).collect();
        let mut lut = vec![0u16; 1 << 15];
        for (key, slot) in lut.iter_mut().enumerate() {
            let q = expand555(key as u16);
            let mut best = (u32::MAX, 0usize);
            for (i, c) in rgb.iter().enumerate() {
                let d = dist2(q, *c);
                if d < best.0 {
                    best = (d, i);
                }
            }
            *slot = best.1 as u16;
        }
        Palette { rgb, colors, lut }
    }

    pub fn len(&self) -> usize {
        self.rgb.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rgb.is_empty()
    }

    /// Nearest palette entry for an RGB colour (channels 0–255, clamped).
    #[inline]
    pub fn nearest(&self, c: [f32; 3]) -> PalIdx {
        let q = |v: f32| ((v.clamp(0.0, 255.0) * (31.0 / 255.0)) + 0.5) as u16;
        let key = (q(c[0]) << 10) | (q(c[1]) << 5) | q(c[2]);
        PalIdx(self.lut[key as usize])
    }

    pub fn rgb_of(&self, idx: PalIdx) -> [u8; 3] {
        self.rgb[idx.0 as usize]
    }

    /// The palette index of an exact member colour, if it is one.
    pub fn index_of(&self, c: [u8; 3]) -> Option<PalIdx> {
        self.rgb.binary_search(&c).ok().map(|i| PalIdx(i as u16))
    }

    /// FNV-1a over the entries, to check the palette is a pure constant.
    pub fn checksum(&self) -> u64 {
        self.rgb.iter().flatten().fold(0xcbf29ce484222325u64, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        })
    }

    /// A fresh build that bypasses the process-wide instance (tests only).
    pub fn build_uncached() -> Palette {
        Palette::build()
    }
}

fn expand555(key: u16) -> [u8; 3] {
    let e = |v: u16| ((v as u32 * 255 + 15) / 31) as u8;
    [e((key >> 10) & 31), e((key >> 5) & 31), e(key & 31)]
}

fn dist2(a: [u8; 3], b: [u8; 3]) -> u32 {
    let d = |x: u8, y: u8| {
        let v = x as i32 - y as i32;
        (v * v) as u32
    };
    d(a[0], b[0]) + d(a[1], b[1]) + d(a[2], b[2])
}

static PALETTE: OnceLock<Palette> = OnceLock::new();

/// The process-wide palette (built on first use, shared across rebuilds).
pub fn palette() -> &'static Palette {
    PALETTE.get_or_init(Palette::build)
}

/// RGB from a `0xrrggbb` constant, as the f32 channels the pixbuf uses.
pub const fn hex(h: u32) -> [f32; 3] {
    [((h >> 16) & 0xff) as f32, ((h >> 8) & 0xff) as f32, (h & 0xff) as f32]
}
