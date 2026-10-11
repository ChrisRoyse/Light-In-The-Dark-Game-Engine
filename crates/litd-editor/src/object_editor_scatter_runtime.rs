//! #826 scatter type records -> runtime consumption (#1837 item 3).
//!
//! Lowered destructible/doodad TOML is a **type** artifact (kind, hp, art, scale). Runtime
//! structs need placement/brush knobs (`tile_x/z`, footprint cells, seed) that are out-of-band
//! for the type record. This bridge maps type fields into runtime structs with **explicit
//! synthetic placement** so tests and FSV can prove the lowered data is runtime-consumable.
//!
//! Mapping notes (fail-loud, no invention of art paths):
//! - `DestructibleTypeRecord.death_art` -> `DestructibleSpec.model_ref` when non-empty
//! - `DoodadTypeRecord.art_ref` + `variations` -> weighted `DoodadVariant` list (`{id}_vN`)
//! - Placement defaults are fixed constants documented below (not game-balance knobs)

use litd_engine::{
    EngineError,
    world_destructibles::{DestructibleField, DestructibleKind, DestructibleSpec},
    world_scatter::{DoodadScatterBrush, DoodadVariant, ScatterCell},
};

use super::scatter_fields::{
    DestructibleTypeRecord, DoodadTypeRecord, ObjectEditorLoweredScatterDocument,
};
use super::validation::object_editor_error;

/// Synthetic placement for type->runtime destructible proof (tile origin, no yaw).
pub const SCATTER_RUNTIME_PROOF_TILE_X: i32 = 0;
pub const SCATTER_RUNTIME_PROOF_TILE_Z: i32 = 0;
pub const SCATTER_RUNTIME_PROOF_YAW_DEG: u32 = 0;
/// Fixed seed so doodad scatter is deterministic across platforms.
pub const SCATTER_RUNTIME_PROOF_SEED: u64 = 0x1837_0826;
/// Single-cell footprint for the proof brush.
pub const SCATTER_RUNTIME_PROOF_CELL: ScatterCell = ScatterCell {
    tile_x: 0,
    tile_z: 0,
};
/// Density 100% so the single proof cell always places (empty brush is a runtime error).
pub const SCATTER_RUNTIME_PROOF_DENSITY_PERCENT: u32 = 100;

/// Runtime proof payload after feeding a lowered scatter type into engine structs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScatterRuntimeProof {
    pub object_id: String,
    pub record_kind: String,
    pub destructible: Option<DestructibleSpec>,
    pub doodad_brush: Option<DoodadScatterBrush>,
    pub doodad_placement_count: u32,
}

/// Map a verified lowered scatter document into runtime types and validate them.
///
/// Placement is synthetic (see module constants). Callers that already verified the TOML
/// fingerprint should pass the deserialized document here.
pub fn load_lowered_scatter_into_runtime(
    document: &ObjectEditorLoweredScatterDocument,
) -> Result<ScatterRuntimeProof, EngineError> {
    match document.record_kind.as_str() {
        "destructible" => {
            let record = document.destructible.as_ref().ok_or_else(|| {
                object_editor_error(
                    "scatter.destructible",
                    "record_kind is destructible but destructible table is missing",
                )
            })?;
            let spec = destructible_type_to_runtime_spec(&document.object_id, record)?;
            // Validate via the same field path the runtime authoring surface uses.
            let field = DestructibleField {
                field_id: format!("{}.proof_field", sanitize_id(&document.object_id)),
                display_name: document.display_name.clone(),
                destructibles: vec![spec.clone()],
            };
            field.to_payload().map_err(map_world_error)?;
            Ok(ScatterRuntimeProof {
                object_id: document.object_id.clone(),
                record_kind: document.record_kind.clone(),
                destructible: Some(spec),
                doodad_brush: None,
                doodad_placement_count: 0,
            })
        }
        "doodad" => {
            let record = document.doodad.as_ref().ok_or_else(|| {
                object_editor_error(
                    "scatter.doodad",
                    "record_kind is doodad but doodad table is missing",
                )
            })?;
            let brush =
                doodad_type_to_runtime_brush(&document.object_id, &document.display_name, record)?;
            let placements = brush.scatter().map_err(map_world_error)?;
            Ok(ScatterRuntimeProof {
                object_id: document.object_id.clone(),
                record_kind: document.record_kind.clone(),
                destructible: None,
                doodad_brush: Some(brush),
                doodad_placement_count: placements.len() as u32,
            })
        }
        other => Err(object_editor_error(
            "scatter.record_kind",
            format!("unsupported record_kind {other:?} for runtime consumption"),
        )),
    }
}

/// Map type fields into a placed `DestructibleSpec` (synthetic origin tile).
pub fn destructible_type_to_runtime_spec(
    object_id: &str,
    record: &DestructibleTypeRecord,
) -> Result<DestructibleSpec, EngineError> {
    let model_ref = record.death_art.trim();
    if model_ref.is_empty() {
        return Err(object_editor_error(
            "destructible.death_art",
            "death_art must be non-empty to bind DestructibleSpec.model_ref (type TOML has no separate model_ref)",
        ));
    }
    Ok(DestructibleSpec {
        id: sanitize_id(object_id),
        kind: parse_destructible_kind(&record.kind)?,
        tile_x: SCATTER_RUNTIME_PROOF_TILE_X,
        tile_z: SCATTER_RUNTIME_PROOF_TILE_Z,
        yaw_deg: SCATTER_RUNTIME_PROOF_YAW_DEG,
        max_hp: record.max_hp,
        armor: record.armor,
        model_ref: model_ref.to_string(),
    })
}

/// Map type fields into a `DoodadScatterBrush` with a synthetic one-cell footprint.
pub fn doodad_type_to_runtime_brush(
    object_id: &str,
    display_name: &str,
    record: &DoodadTypeRecord,
) -> Result<DoodadScatterBrush, EngineError> {
    if record.art_ref.trim().is_empty() {
        return Err(object_editor_error(
            "doodad.art_ref",
            "art_ref must be non-empty to seed runtime doodad variants",
        ));
    }
    let base = sanitize_id(object_id);
    let mut variants = Vec::with_capacity(record.variations as usize);
    for index in 0..record.variations {
        variants.push(DoodadVariant {
            variant_id: format!("{base}_v{index}"),
            weight: 1,
        });
    }
    let brush = DoodadScatterBrush {
        brush_id: format!("{base}.proof_brush"),
        display_name: if display_name.trim().is_empty() {
            base.clone()
        } else {
            display_name.to_string()
        },
        variants,
        density_percent: SCATTER_RUNTIME_PROOF_DENSITY_PERCENT,
        min_scale_milli: record.min_scale_milli,
        max_scale_milli: record.max_scale_milli,
        min_yaw_deg: 0,
        max_yaw_deg: 0,
        cells: vec![SCATTER_RUNTIME_PROOF_CELL],
        seed: SCATTER_RUNTIME_PROOF_SEED,
    };
    brush.validate().map_err(map_world_error)?;
    Ok(brush)
}

fn parse_destructible_kind(kind: &str) -> Result<DestructibleKind, EngineError> {
    match kind {
        "tree" => Ok(DestructibleKind::Tree),
        "gate" => Ok(DestructibleKind::Gate),
        "wall" => Ok(DestructibleKind::Wall),
        "breakable" => Ok(DestructibleKind::Breakable),
        other => Err(object_editor_error(
            "destructible.kind",
            format!("unknown destructible kind {other:?} for runtime mapping"),
        )),
    }
}

fn sanitize_id(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "scatter_runtime".to_string()
    } else {
        cleaned
    }
}

fn map_world_error(error: litd_engine::world_authoring::WorldAuthoringError) -> EngineError {
    object_editor_error(
        "scatter.runtime",
        format!("runtime consumption rejected lowered scatter type: {error}"),
    )
}
