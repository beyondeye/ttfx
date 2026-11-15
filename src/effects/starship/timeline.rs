//! The storyboard timeline (aidocs/starship/storyboard.md §1.4–1.5).
//!
//! All times are storyboard seconds. A cross-fade of length d occupies the
//! first d seconds of the incoming scene; the outgoing scene keeps running on
//! its own clock underneath. The loop fade at the end of B goes to scene 1's
//! frame at t = 0, so the rebuild after `None` shows no flash.

use crate::utils::easing::Easing;

/// The total length; the effect returns `None` on the tick after it.
pub const TOTAL_SECONDS: f64 = 64.0;

/// The loop fade: B fades to scene 1 at t = 0 over the last 1.5 s.
pub const LOOP_FADE_START: f64 = 62.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SceneId {
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S9,
    S10,
    S11,
    B,
}

impl SceneId {
    /// The particle owner tag of this scene.
    pub fn tag(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Into {
    Cut,
    CrossFade(f64),
    Continuous,
    Loop,
}

#[derive(Debug, Clone, Copy)]
pub struct SceneSpec {
    pub id: SceneId,
    pub start: f64,
    pub dur: f64,
    pub into_next: Into,
}

impl SceneSpec {
    pub fn end(&self) -> f64 {
        self.start + self.dur
    }
}

pub const SCENES: &[SceneSpec] = &[
    SceneSpec { id: SceneId::S1, start: 0.0, dur: 10.0, into_next: Into::CrossFade(0.5) },
    SceneSpec { id: SceneId::S2, start: 10.0, dur: 6.0, into_next: Into::Cut },
    SceneSpec { id: SceneId::S3, start: 16.0, dur: 6.0, into_next: Into::Cut },
    SceneSpec { id: SceneId::S4, start: 22.0, dur: 3.0, into_next: Into::Cut },
    SceneSpec { id: SceneId::S5, start: 25.0, dur: 5.0, into_next: Into::CrossFade(1.0) },
    SceneSpec { id: SceneId::S6, start: 30.0, dur: 5.0, into_next: Into::Cut },
    SceneSpec { id: SceneId::S9, start: 35.0, dur: 4.0, into_next: Into::Cut },
    SceneSpec { id: SceneId::S10, start: 39.0, dur: 6.0, into_next: Into::Cut },
    SceneSpec { id: SceneId::S11, start: 45.0, dur: 13.0, into_next: Into::Continuous },
    SceneSpec { id: SceneId::B, start: 58.0, dur: 6.0, into_next: Into::Loop },
];

/// A sub-shot boundary inside a scene: hard cut unless `continuous`.
#[derive(Debug, Clone, Copy)]
pub struct SubShot {
    pub name: &'static str,
    pub start: f64,
    pub continuous_from_prev: bool,
}

pub const SUBSHOTS: &[SubShot] = &[
    SubShot { name: "1a", start: 0.0, continuous_from_prev: false },
    SubShot { name: "1b", start: 6.0, continuous_from_prev: false },
    SubShot { name: "2a", start: 10.0, continuous_from_prev: false },
    SubShot { name: "2b", start: 12.5, continuous_from_prev: true },
    SubShot { name: "3", start: 16.0, continuous_from_prev: false },
    SubShot { name: "4", start: 22.0, continuous_from_prev: false },
    SubShot { name: "5", start: 25.0, continuous_from_prev: false },
    SubShot { name: "6", start: 30.0, continuous_from_prev: false },
    SubShot { name: "9", start: 35.0, continuous_from_prev: false },
    SubShot { name: "10", start: 39.0, continuous_from_prev: false },
    SubShot { name: "11a", start: 45.0, continuous_from_prev: false },
    SubShot { name: "11b", start: 47.0, continuous_from_prev: false },
    SubShot { name: "11c", start: 51.5, continuous_from_prev: false },
    SubShot { name: "11d", start: 54.0, continuous_from_prev: false },
    SubShot { name: "B", start: 58.0, continuous_from_prev: true },
];

fn spec_index_at(t: f64) -> usize {
    SCENES.iter().rposition(|s| s.start <= t).unwrap_or(0)
}

/// The scene shown at storyboard time t (the incoming one inside a fade).
pub fn scene_at(t: f64) -> SceneId {
    SCENES[spec_index_at(t)].id
}

pub fn spec(id: SceneId) -> &'static SceneSpec {
    SCENES.iter().find(|s| s.id == id).expect("every scene is in the table")
}

/// The sub-shot index shown at storyboard time t.
pub fn subshot_at(t: f64) -> usize {
    SUBSHOTS.iter().rposition(|s| s.start <= t).unwrap_or(0)
}

/// What blends at time t.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Blend {
    /// A cross-fade from scene `from` (on its own clock) into `to`.
    Fade { from: SceneId, to: SceneId, w: f64 },
    /// The loop fade from B to scene 1's frame at t = 0.
    Loop { w: f64 },
}

/// The blend active at t, if any; `w` runs 0 → 1 with `InOutSine`.
pub fn blend_weight(t: f64) -> Option<Blend> {
    if t >= LOOP_FADE_START {
        let w = ((t - LOOP_FADE_START) / (TOTAL_SECONDS - LOOP_FADE_START)).clamp(0.0, 1.0);
        return Some(Blend::Loop { w: Easing::InOutSine.ease(w) });
    }
    let i = spec_index_at(t);
    if i == 0 {
        return None;
    }
    let (prev, cur) = (&SCENES[i - 1], &SCENES[i]);
    match prev.into_next {
        Into::CrossFade(d) if t < cur.start + d => {
            let p = ((t - cur.start) / d).clamp(0.0, 1.0);
            Some(Blend::Fade { from: prev.id, to: cur.id, w: Easing::InOutSine.ease(p) })
        }
        _ => None,
    }
}

/// How the boundary crossed between `t0` (exclusive) and `t1` (inclusive)
/// treats the particle pool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoolEvent {
    None,
    /// Hard cut: clear the pool.
    Clear,
    /// A cross-fade ended: kill the outgoing scene's particles.
    KillOwner(SceneId),
}

pub fn pool_event(t0: f64, t1: f64) -> PoolEvent {
    // Fade ends first: a fade end and a cut never share a tick.
    for (i, s) in SCENES.iter().enumerate().skip(1) {
        if let Into::CrossFade(d) = SCENES[i - 1].into_next {
            let end = s.start + d;
            if t0 < end && end <= t1 {
                return PoolEvent::KillOwner(SCENES[i - 1].id);
            }
        }
    }
    for sub in SUBSHOTS.iter().skip(1) {
        if t0 < sub.start && sub.start <= t1 {
            if sub.continuous_from_prev {
                return PoolEvent::None;
            }
            // A scene boundary entered through a cross-fade is not a cut.
            let scene_idx = spec_index_at(sub.start);
            let is_scene_start = (SCENES[scene_idx].start - sub.start).abs() < 1e-9;
            if is_scene_start && scene_idx > 0 && matches!(SCENES[scene_idx - 1].into_next, Into::CrossFade(_)) {
                return PoolEvent::None;
            }
            return PoolEvent::Clear;
        }
    }
    PoolEvent::None
}

/// Progress through [a, b] with an easing, clamped.
pub fn ease_between(t: f64, a: f64, b: f64, e: Easing) -> f64 {
    if t <= a {
        0.0
    } else if t >= b {
        1.0
    } else {
        e.ease((t - a) / (b - a))
    }
}
