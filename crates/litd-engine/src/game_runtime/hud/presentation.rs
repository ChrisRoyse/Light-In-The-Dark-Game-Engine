//! Thin presentation surface: lower deterministic integer HUD state into the #882
//! runtime-text DTOs the produced game renders (`render::runtime_text`).
//!
//! This is the ONLY place floats appear (glam `Vec3`/`Vec4`, seconds). Presentation
//! reads sim state; it never writes back, so nothing here can affect a state hash.
//! Milli-unit integers become metres (`/1000`), tick counts become seconds via the
//! integer ticks-per-second, and 0..=255 colour channels become 0..1 floats.

use crate::render::runtime_text::{DamageNumber, WorldLabel};

use super::HudColor;
use super::floating_text::FloatingTextSet;

fn color_to_vec4(color: HudColor) -> glam::Vec4 {
    glam::Vec4::new(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        f32::from(color.a) / 255.0,
    )
}

fn mm_to_vec3(position_mm: [i64; 3]) -> glam::Vec3 {
    glam::Vec3::new(
        position_mm[0] as f32 / 1000.0,
        position_mm[1] as f32 / 1000.0,
        position_mm[2] as f32 / 1000.0,
    )
}

/// Build the #882 damage-number draw list for every live floating text at `tick`.
/// Age/lifetime seconds come from the integer tick counts and `ticks_per_second`;
/// the fade alpha is taken from the deterministic integer `alpha_at`, so the drawn
/// opacity matches sim state exactly.
pub fn floating_text_damage_numbers(
    set: &FloatingTextSet,
    tick: u64,
    ticks_per_second: u32,
) -> Vec<DamageNumber> {
    let tps = ticks_per_second.max(1) as f32;
    set.texts()
        .iter()
        .map(|text| {
            let elapsed_ticks = tick.saturating_sub(text.spawn_tick);
            let mut color = color_to_vec4(text.color);
            color.w = f32::from(text.alpha_at(tick)) / 255.0;
            DamageNumber {
                id: format!("floating-text-{}", text.id),
                text: text.text.clone(),
                world_position: mm_to_vec3(text.position_at(tick)),
                age_seconds: elapsed_ticks as f32 / tps,
                lifetime_seconds: text.lifespan_ticks as f32 / tps,
                // Velocity z (milli-units/tick) -> px/s rise, a presentation mapping.
                rise_px_per_second: text.velocity_mm_per_tick[2] as f32 / 1000.0 * tps,
                font_px: 18.0,
                color,
            }
        })
        .collect()
}

/// Build a single world-anchored #882 label (e.g. a quest marker or unit nameplate)
/// from integer milli-unit state. Presentation-only.
pub fn world_label(id: &str, text: &str, position_mm: [i64; 3], color: HudColor) -> WorldLabel {
    WorldLabel {
        id: id.to_string(),
        text: text.to_string(),
        world_position: mm_to_vec3(position_mm),
        world_offset: glam::Vec3::new(0.0, 2.0, 0.0),
        font_px: 16.0,
        color: color_to_vec4(color),
    }
}
