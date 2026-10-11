use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    render::vfx::{
        VfxEffectDescriptor, VfxEffectStoreReadback, invalid_vfx, read_effect_from_content_store,
        validate_effect_descriptor,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfxRenderCueSourceKind {
    Ability,
    Trigger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxRenderCue {
    pub cue_id: String,
    pub source_kind: VfxRenderCueSourceKind,
    pub source_id: String,
    pub effect: VfxEffectDescriptor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxResolvedRenderCue {
    pub cue: VfxRenderCue,
    pub effect_readback: VfxEffectStoreReadback,
    pub source_of_truth: String,
}

impl VfxRenderCue {
    pub fn for_ability(
        cue_id: impl Into<String>,
        ability_id: impl Into<String>,
        effect: VfxEffectDescriptor,
    ) -> Result<Self, EngineError> {
        Self::new(cue_id, VfxRenderCueSourceKind::Ability, ability_id, effect)
    }

    pub fn for_trigger(
        cue_id: impl Into<String>,
        trigger_id: impl Into<String>,
        effect: VfxEffectDescriptor,
    ) -> Result<Self, EngineError> {
        Self::new(cue_id, VfxRenderCueSourceKind::Trigger, trigger_id, effect)
    }

    pub fn new(
        cue_id: impl Into<String>,
        source_kind: VfxRenderCueSourceKind,
        source_id: impl Into<String>,
        effect: VfxEffectDescriptor,
    ) -> Result<Self, EngineError> {
        let cue = Self {
            cue_id: cue_id.into(),
            source_kind,
            source_id: source_id.into(),
            effect,
        };
        validate_render_cue(&cue)?;
        Ok(cue)
    }
}

pub fn resolve_render_cue_effect(
    root: impl AsRef<Path>,
    cue: &VfxRenderCue,
) -> Result<VfxResolvedRenderCue, EngineError> {
    validate_render_cue(cue)?;
    let readback = read_effect_from_content_store(root, &cue.effect)?;
    Ok(VfxResolvedRenderCue {
        cue: cue.clone(),
        effect_readback: readback,
        source_of_truth: "render cue descriptor + content-addressed VFX object bytes".to_string(),
    })
}

pub fn validate_render_cue(cue: &VfxRenderCue) -> Result<(), EngineError> {
    validate_id("render_cue.cue_id", &cue.cue_id)?;
    match cue.source_kind {
        VfxRenderCueSourceKind::Ability => {
            validate_id("render_cue.ability_id", &cue.source_id)?;
        }
        VfxRenderCueSourceKind::Trigger => {
            validate_id("render_cue.trigger_id", &cue.source_id)?;
        }
    }
    validate_effect_descriptor(&cue.effect)?;
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_vfx(field, "id must be non-empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return invalid_vfx(
            field,
            "id must contain only ASCII letters, digits, '.', '_', '-', or ':'",
        );
    }
    Ok(())
}
