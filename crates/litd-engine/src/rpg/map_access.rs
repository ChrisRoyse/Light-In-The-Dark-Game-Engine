use std::collections::BTreeSet;

use litd_hash::sha256_hex;

use crate::{
    EngineError,
    math::{Vec3, Vec4},
    render::ui::{
        MapOverlayMarkerKind, MapOverlayMarkerSource, MapOverlayPolicy, MapOverlayPolicyMode,
        MinimapMarker, validate_map_overlay_policy,
    },
};

use super::{
    RPG_MAP_ACCESS_SCHEMA, RpgMapAccessReadback, RpgMapAccessState, RpgMapFeature,
    RpgMapFeatureKind, RpgMapOpenRequest, RpgMapRenderPlan, RpgMapRuntimeState, RpgMapSource,
    RpgMapSourceKind, RpgMapSourceStatus,
};

pub fn resolve_rpg_map_access(
    state: &RpgMapRuntimeState,
    request: &RpgMapOpenRequest,
) -> Result<RpgMapRenderPlan, EngineError> {
    validate_rpg_map_state(state)?;
    let state_sha256 = state_hash(state)?;
    let available_source_count = state
        .sources
        .iter()
        .filter(|source| source.status == RpgMapSourceStatus::Available)
        .count();
    let Some(source_id) = request.requested_source_id.as_deref() else {
        return no_map_plan(
            state,
            request,
            state_sha256,
            available_source_count,
            None,
            None,
        );
    };
    validate_token("request.requested_source_id", source_id)?;
    let Some(source) = state
        .sources
        .iter()
        .find(|source| source.source_id == source_id)
    else {
        return invalid_map_access(
            "request.requested_source_id",
            format!("unknown map source {source_id}"),
        );
    };
    if source.status != RpgMapSourceStatus::Available {
        return no_map_plan(
            state,
            request,
            state_sha256,
            available_source_count,
            Some(source),
            Some(match source.status {
                RpgMapSourceStatus::Removed => "map_source_removed",
                RpgMapSourceStatus::Lost => "map_source_lost",
                RpgMapSourceStatus::Available => unreachable!(),
            }),
        );
    }
    acquired_map_plan(state, request, source, state_sha256, available_source_count)
}

pub fn validate_rpg_map_state(state: &RpgMapRuntimeState) -> Result<(), EngineError> {
    if state.schema != RPG_MAP_ACCESS_SCHEMA {
        return invalid_map_access("schema", format!("expected {RPG_MAP_ACCESS_SCHEMA}"));
    }
    validate_token("player_id", &state.player_id)?;
    if state.sources.len() > 256 {
        return invalid_map_access("sources", "at most 256 map sources");
    }
    let mut seen = BTreeSet::new();
    for source in &state.sources {
        validate_map_source(source)?;
        if !seen.insert(source.source_id.as_str()) {
            return invalid_map_access(
                "source.source_id",
                format!("duplicate {}", source.source_id),
            );
        }
    }
    Ok(())
}

pub fn remove_rpg_map_source(
    state: &mut RpgMapRuntimeState,
    source_id: &str,
    status: RpgMapSourceStatus,
) -> Result<(), EngineError> {
    validate_token("source_id", source_id)?;
    if status == RpgMapSourceStatus::Available {
        return invalid_map_access("status", "removal status cannot be available");
    }
    let Some(source) = state
        .sources
        .iter_mut()
        .find(|source| source.source_id == source_id)
    else {
        return invalid_map_access("source_id", format!("unknown map source {source_id}"));
    };
    source.status = status;
    validate_rpg_map_state(state)
}

pub fn state_hash(state: &RpgMapRuntimeState) -> Result<String, EngineError> {
    serde_json::to_vec(state)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize RPG map access state: {error}"),
        })
}

fn acquired_map_plan(
    state: &RpgMapRuntimeState,
    request: &RpgMapOpenRequest,
    source: &RpgMapSource,
    state_sha256: String,
    available_source_count: usize,
) -> Result<RpgMapRenderPlan, EngineError> {
    let policy =
        MapOverlayPolicy::runtime_rpg_diegetic_acquired(&source.source_id, &source.scope_id);
    let markers = source
        .features
        .iter()
        .map(|feature| marker_for_feature(source, feature))
        .collect::<Vec<_>>();
    let policy_readback = validate_map_overlay_policy(&policy, &markers)?;
    let gps_indicators_disabled = !policy_readback.player_position_indicator
        && !policy_readback.viewport_indicator
        && !policy_readback.quest_markers
        && !policy_readback.route_lines;
    Ok(RpgMapRenderPlan {
        overlay_policy: policy,
        markers,
        readback: RpgMapAccessReadback {
            player_id: state.player_id.clone(),
            access: RpgMapAccessState::DiegeticMapVisible,
            requested_source_id: request.requested_source_id.clone(),
            active_source_id: Some(source.source_id.clone()),
            scope_id: Some(source.scope_id.clone()),
            source_kind: Some(source.kind),
            source_status: Some(source.status),
            fidelity: Some(source.fidelity),
            map_surface_visible: policy_readback.surface_visible,
            degraded_reason: None,
            gps_indicators_disabled,
            source_count: state.sources.len(),
            available_source_count,
            policy: policy_readback,
            state_sha256,
        },
    })
}

fn no_map_plan(
    state: &RpgMapRuntimeState,
    request: &RpgMapOpenRequest,
    state_sha256: String,
    available_source_count: usize,
    source: Option<&RpgMapSource>,
    degraded_reason: Option<&str>,
) -> Result<RpgMapRenderPlan, EngineError> {
    let policy = MapOverlayPolicy::runtime_rpg_no_map_access();
    let policy_readback = validate_map_overlay_policy(&policy, &[])?;
    Ok(RpgMapRenderPlan {
        overlay_policy: policy,
        markers: Vec::new(),
        readback: RpgMapAccessReadback {
            player_id: state.player_id.clone(),
            access: if source.is_some() {
                RpgMapAccessState::SourceUnavailable
            } else {
                RpgMapAccessState::NoMapAccess
            },
            requested_source_id: request.requested_source_id.clone(),
            active_source_id: None,
            scope_id: source.map(|source| source.scope_id.clone()),
            source_kind: source.map(|source| source.kind),
            source_status: source.map(|source| source.status),
            fidelity: source.map(|source| source.fidelity),
            map_surface_visible: policy_readback.surface_visible,
            degraded_reason: degraded_reason.map(ToOwned::to_owned),
            gps_indicators_disabled: true,
            source_count: state.sources.len(),
            available_source_count,
            policy: policy_readback,
            state_sha256,
        },
    })
}

fn marker_for_feature(source: &RpgMapSource, feature: &RpgMapFeature) -> MinimapMarker {
    MinimapMarker {
        id: feature.feature_id.clone(),
        kind: match feature.kind {
            RpgMapFeatureKind::Landmark => MapOverlayMarkerKind::Landmark,
            RpgMapFeatureKind::Danger => MapOverlayMarkerKind::Danger,
            RpgMapFeatureKind::Resource => MapOverlayMarkerKind::Resource,
        },
        source: source.kind.marker_source(),
        allowed_policy: MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
        world_position: Vec3::new(
            feature.world_position[0],
            feature.world_position[1],
            feature.world_position[2],
        ),
        color: feature.kind.marker_color(),
        radius_px: 4.0,
    }
}

fn validate_map_source(source: &RpgMapSource) -> Result<(), EngineError> {
    validate_token("source.source_id", &source.source_id)?;
    validate_token("source.scope_id", &source.scope_id)?;
    validate_text("source.display_name", &source.display_name, 96)?;
    validate_text("source.acquired_from", &source.acquired_from, 160)?;
    if !(1..=5).contains(&source.quality_tier) {
        return invalid_map_access("source.quality_tier", "must be inside 1..=5");
    }
    if source.features.is_empty() {
        return invalid_map_access("source.features", "map source requires authored content");
    }
    if source.features.len() > 128 {
        return invalid_map_access("source.features", "at most 128 features");
    }
    let mut seen = BTreeSet::new();
    for feature in &source.features {
        validate_feature(feature)?;
        if !seen.insert(feature.feature_id.as_str()) {
            return invalid_map_access(
                "feature.feature_id",
                format!("duplicate {}", feature.feature_id),
            );
        }
    }
    Ok(())
}

fn validate_feature(feature: &RpgMapFeature) -> Result<(), EngineError> {
    validate_token("feature.feature_id", &feature.feature_id)?;
    validate_text("feature.label", &feature.label, 96)?;
    if !feature.world_position.iter().all(|value| value.is_finite()) {
        return invalid_map_access("feature.world_position", "coordinates must be finite");
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid_map_access(field, "must be non-empty and <= 96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid_map_access(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

fn validate_text(field: &str, value: &str, max_len: usize) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_map_access(field, "must be non-empty");
    }
    if value.len() > max_len {
        return invalid_map_access(field, format!("must be <= {max_len} bytes"));
    }
    Ok(())
}

fn invalid_map_access<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: format!("map_access.{}", field.into()),
        reason: reason.to_string(),
    })
}

impl RpgMapSourceKind {
    fn marker_source(self) -> MapOverlayMarkerSource {
        match self {
            Self::CartographerPurchase => MapOverlayMarkerSource::InventoryMap,
            Self::FoundOrLooted => MapOverlayMarkerSource::FoundMap,
            Self::QuestReward => MapOverlayMarkerSource::QuestReward,
            Self::ArchiveCopy => MapOverlayMarkerSource::ArchiveCopy,
            Self::ExplorationKnowledge => MapOverlayMarkerSource::ExplorationKnowledge,
            Self::NpcKnowledge => MapOverlayMarkerSource::NpcKnowledge,
        }
    }
}

impl RpgMapFeatureKind {
    fn marker_color(self) -> Vec4 {
        match self {
            Self::Landmark => Vec4::new(0.76, 0.58, 0.32, 1.0),
            Self::Danger => Vec4::new(0.70, 0.18, 0.12, 1.0),
            Self::Resource => Vec4::new(0.23, 0.52, 0.36, 1.0),
        }
    }
}
