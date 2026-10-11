use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::{
    WorldMemoryConstellation, WorldMemoryIndex, WorldMemoryLensKind, WorldMemorySlotKind,
    WorldMemorySlotState, world_memory_slots::validate_typed_slots,
};

pub fn validate_world_memory_index(index: &WorldMemoryIndex) -> Result<(), EngineError> {
    if index.schema != "litd.authoring.world_memory.v2.no_flatten_slots" {
        return invalid_validation(
            "index.schema",
            format!("unsupported schema {}", index.schema),
        );
    }
    if index.world_count == 0 || index.constellation_count != index.constellations.len() {
        return invalid_validation(
            "index.counts",
            "world_count must be non-zero and constellation_count must match rows",
        );
    }
    let mut seen = BTreeSet::new();
    for constellation in &index.constellations {
        if !seen.insert(constellation.constellation_id.clone()) {
            return invalid_validation(
                "index.duplicate",
                format!("duplicate constellation {}", constellation.constellation_id),
            );
        }
        validate_world_memory_constellation(constellation)?;
    }
    Ok(())
}

pub fn validate_world_memory_constellation(
    constellation: &WorldMemoryConstellation,
) -> Result<(), EngineError> {
    if constellation.constellation_id.trim().is_empty()
        || constellation.world_id.trim().is_empty()
        || constellation.source_id.trim().is_empty()
        || constellation.title.trim().is_empty()
        || constellation.source_fingerprint.trim().is_empty()
    {
        return invalid_validation(
            "constellation.identity",
            "identity fields must be non-empty",
        );
    }
    validate_typed_slots(constellation.source_kind, &constellation.slots)?;
    for slot in &constellation.slots {
        match slot.kind {
            WorldMemorySlotKind::Text => {
                if !constellation
                    .lenses
                    .contains_key(&WorldMemoryLensKind::Text)
                {
                    return invalid_validation("slot.text", "text slot missing text lens");
                }
            }
            WorldMemorySlotKind::Structured => {
                if !constellation
                    .lenses
                    .contains_key(&WorldMemoryLensKind::Structured)
                {
                    return invalid_validation(
                        "slot.structured",
                        "structured slot missing structured lens",
                    );
                }
            }
            WorldMemorySlotKind::Code if slot.state == WorldMemorySlotState::Populated => {
                if !constellation
                    .lenses
                    .contains_key(&WorldMemoryLensKind::Code)
                {
                    return invalid_validation(
                        "slot.code",
                        "populated code slot missing code lens",
                    );
                }
            }
            WorldMemorySlotKind::Code | WorldMemorySlotKind::VectorSet => {}
        }
    }
    Ok(())
}

fn invalid_validation<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.world_memory.{}", field.into()),
        reason: reason.into(),
    })
}
