//! stub — a starting point for a new effect (old engine only).
//!
//! Characters appear one group at a time (rows top to bottom by default),
//! each fading from the first gradient color to its final gradient color.
//! Rename it, then replace build() and next_frame() with your own animation.
//!
//! How an effect works:
//! - build() runs once. It gives every character one or more *scenes*: lists
//!   of (symbol, colors, duration) frames.
//! - next_frame() runs once per frame. It activates characters (activate_scene
//!   + set_character_visibility + add to active_characters), calls ctx.update()
//!   to advance every active scene, and returns ctx.frame(). It returns None
//!   when the animation is finished.
//! - A character leaves active_characters by itself once its scene is done.

use clap::Args;

use crate::cli::parse_color;
use crate::effects::common::{
    parse_character_group, parse_gradient_direction, parse_gradient_steps, parse_non_negative_int,
};
use crate::engine::character::CharId;
use crate::engine::ctx::{EffectHooks, EngineCtx};
use crate::engine::effect::Effect;
use crate::engine::error::EngineError;
use crate::engine::events::EffectCallback;
use crate::engine::terminal::{CharacterFilter, CharacterGroup};
use crate::utils::graphics::{Color, Gradient, GradientDirection};

#[derive(Args, Debug, Clone)]
pub struct StubConfig {
    /// Order in which groups of characters appear.
    #[arg(long = "reveal-direction", default_value = "row_top_to_bottom",
          value_parser = parse_character_group)]
    pub reveal_direction: CharacterGroup,

    /// Number of frames to wait before revealing the next group.
    #[arg(long = "reveal-delay", default_value_t = 2, value_parser = parse_non_negative_int)]
    pub reveal_delay: i64,

    /// Space separated, unquoted, list of colors for the final gradient.
    #[arg(long = "final-gradient-stops", num_args = 1.., value_parser = parse_color,
          default_values = ["8A008A", "00D1FF", "FFFFFF"])]
    pub final_gradient_stops: Vec<Color>,

    /// Number of gradient steps to use.
    #[arg(long = "final-gradient-steps", num_args = 1.., value_parser = parse_gradient_steps,
          default_values = ["12"])]
    pub final_gradient_steps: Vec<i64>,

    /// Number of frames to display each gradient step.
    #[arg(long = "final-gradient-frames", default_value_t = 4, value_parser = parse_non_negative_int)]
    pub final_gradient_frames: i64,

    /// Direction of the final gradient.
    #[arg(long = "final-gradient-direction", default_value = "diagonal", value_parser = parse_gradient_direction)]
    pub final_gradient_direction: GradientDirection,
}

pub struct Stub {
    config: StubConfig,
    /// Groups still waiting to appear, in reverse order (pop from the end).
    pending: Vec<Vec<CharId>>,
    countdown: i64,
}

impl Stub {
    pub fn new(config: StubConfig) -> Self {
        Stub { config, pending: Vec::new(), countdown: 0 }
    }
}

impl EffectHooks for Stub {
    fn dispatch_callback(&mut self, _ctx: &mut EngineCtx, _character: CharId, _callback: &EffectCallback) {}
}

impl Effect for Stub {
    fn build(&mut self, ctx: &mut EngineCtx) -> Result<(), EngineError> {
        let config = self.config.clone();
        let final_gradient = Gradient::new(&config.final_gradient_stops, &config.final_gradient_steps, false, false)
            .map_err(EngineError::Other)?;
        let canvas = &ctx.terminal.canvas;
        let color_at = final_gradient
            .build_coordinate_color_mapping(
                canvas.text_bottom,
                canvas.text_top,
                canvas.text_left,
                canvas.text_right,
                config.final_gradient_direction,
            )
            .map_err(EngineError::Other)?;
        let first = final_gradient.spectrum[0].clone();

        let groups = ctx.terminal.get_characters_grouped(CharacterFilter::default(), config.reveal_direction);
        for &id in groups.iter().flatten() {
            let ch = &mut ctx.terminal.arena[id.0 as usize];
            let final_fg = color_at.get(&ch.input_coord).expect("gradient covers the text").clone();
            // Fade in: from the first gradient color to this character's final color.
            let fade = Gradient::new(&[first.clone(), final_fg], &config.final_gradient_steps, false, false)
                .map_err(EngineError::Other)?;
            let symbol = ch.input_symbol.clone();
            let uses_pre = ch.uses_input_preexisting_colors;
            ch.animation.new_scene(false, None, None, "fade_in", uses_pre);
            ch.animation
                .scenes
                .get_mut("fade_in")
                .unwrap()
                .apply_gradient_to_symbols(&[symbol], config.final_gradient_frames.max(1), Some(&fade), None)
                .map_err(EngineError::Other)?;
        }
        self.pending = groups.into_iter().rev().collect();
        Ok(())
    }

    fn next_frame(&mut self, ctx: &mut EngineCtx) -> Option<String> {
        if self.pending.is_empty() && ctx.active_characters.is_empty() {
            return None;
        }
        if self.countdown == 0 {
            if let Some(group) = self.pending.pop() {
                for id in group {
                    ctx.activate_scene(self, id, "fade_in");
                    ctx.terminal.set_character_visibility(id, true);
                    ctx.active_characters.insert(id);
                }
            }
            self.countdown = self.config.reveal_delay;
        } else {
            self.countdown -= 1;
        }
        ctx.update(self);
        Some(ctx.frame())
    }
}
