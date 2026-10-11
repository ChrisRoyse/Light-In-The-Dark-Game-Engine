use std::collections::BTreeSet;

use litd_engine::{EngineError, render::vfx::VFX_MAX_PARTICLES_PER_EMITTER};

use super::{
    VfxParticleBudget, VfxTextElement, VfxTextPattern, VfxTextRegion, VfxTextTaxonomy,
    invalid_text_generation,
};

pub(super) fn classify_taxonomy(tokens: &BTreeSet<String>) -> Result<VfxTextTaxonomy, EngineError> {
    let element = classify_element(tokens)?;
    let pattern = classify_pattern(tokens)?;
    let region = classify_region(tokens, &pattern);
    let mut features = Vec::new();
    if has_any(
        tokens,
        &["rising", "rise", "upward", "embers", "spark", "sparks"],
    ) {
        features.push("rising_embers".to_string());
    }
    if has_any(tokens, &["shockwave", "ring", "wave"]) {
        features.push("shockwave_ring".to_string());
    }
    Ok(VfxTextTaxonomy {
        element,
        pattern,
        region,
        features,
        source_of_truth: "prompt tokens classified into element x pattern x region".to_string(),
    })
}

pub(super) fn particle_budget(
    tokens: &BTreeSet<String>,
    taxonomy: &VfxTextTaxonomy,
) -> VfxParticleBudget {
    let requested = explicit_particle_count(tokens).unwrap_or(match taxonomy.pattern {
        VfxTextPattern::Explosion => 65_536,
        VfxTextPattern::Aura => 32_768,
        VfxTextPattern::Bolt | VfxTextPattern::Trail => 16_384,
    });
    let emitted = requested.clamp(1_024, VFX_MAX_PARTICLES_PER_EMITTER);
    let capped = requested != emitted;
    VfxParticleBudget {
        requested_max_particles: requested,
        emitted_max_particles: emitted,
        max_allowed_particles: VFX_MAX_PARTICLES_PER_EMITTER,
        capped,
        cap_reason: if capped {
            format!(
                "requested {requested} particles capped to per-emitter limit {}",
                VFX_MAX_PARTICLES_PER_EMITTER
            )
        } else {
            "within budget".to_string()
        },
    }
}

pub(super) fn prompt_tokens(prompt: &str) -> BTreeSet<String> {
    prompt
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn classify_element(tokens: &BTreeSet<String>) -> Result<VfxTextElement, EngineError> {
    if has_any(
        tokens,
        &[
            "fire", "fireball", "fiery", "flame", "ember", "embers", "cinder",
        ],
    ) {
        Ok(VfxTextElement::Fire)
    } else if has_any(tokens, &["lightning", "bolt", "electric", "storm"]) {
        Ok(VfxTextElement::Lightning)
    } else if has_any(tokens, &["frost", "ice", "snow", "cold"]) {
        Ok(VfxTextElement::Frost)
    } else if has_any(tokens, &["arcane", "magic", "mana", "void"]) {
        Ok(VfxTextElement::Arcane)
    } else if has_any(tokens, &["smoke", "ash", "dust"]) {
        Ok(VfxTextElement::Smoke)
    } else {
        invalid_text_generation(
            "taxonomy.element",
            "prompt must name a supported VFX element",
        )
    }
}

fn classify_pattern(tokens: &BTreeSet<String>) -> Result<VfxTextPattern, EngineError> {
    if has_any(
        tokens,
        &["explosion", "explode", "burst", "detonation", "impact"],
    ) {
        Ok(VfxTextPattern::Explosion)
    } else if has_any(tokens, &["bolt", "beam", "strike", "zap"]) {
        Ok(VfxTextPattern::Bolt)
    } else if has_any(tokens, &["aura", "field", "halo"]) {
        Ok(VfxTextPattern::Aura)
    } else if has_any(tokens, &["trail", "wake", "streak"]) {
        Ok(VfxTextPattern::Trail)
    } else {
        invalid_text_generation(
            "taxonomy.pattern",
            "prompt must name a supported VFX pattern",
        )
    }
}

fn classify_region(tokens: &BTreeSet<String>, pattern: &VfxTextPattern) -> VfxTextRegion {
    if has_any(tokens, &["cone", "spray"]) {
        VfxTextRegion::Cone
    } else if has_any(tokens, &["line", "beam", "bolt"]) || matches!(pattern, VfxTextPattern::Bolt)
    {
        VfxTextRegion::Line
    } else if has_any(tokens, &["sphere", "orb", "area", "ring"]) {
        VfxTextRegion::Sphere
    } else {
        match pattern {
            VfxTextPattern::Explosion | VfxTextPattern::Aura => VfxTextRegion::Sphere,
            VfxTextPattern::Bolt | VfxTextPattern::Trail => VfxTextRegion::Line,
        }
    }
}

fn explicit_particle_count(tokens: &BTreeSet<String>) -> Option<u32> {
    if tokens.contains("million") {
        return Some(1_000_000);
    }
    tokens
        .iter()
        .filter_map(|token| token.parse::<u32>().ok())
        .max()
}

fn has_any(tokens: &BTreeSet<String>, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| tokens.contains(*candidate))
}
