use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::MinimapMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapOverlayPolicyMode {
    AuthoringPreview,
    RuntimeRpgNoMapAccess,
    RuntimeRpgDiegeticAcquired,
    RuntimeAssisted,
    RuntimeStrategyMinimap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapOverlayMarkerKind {
    PlayerPosition,
    ViewportRect,
    QuestObjective,
    RouteLine,
    Landmark,
    Danger,
    Placement,
    TacticalUnit,
    Resource,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapOverlayMarkerSource {
    EditorAuthoring,
    InventoryMap,
    FoundMap,
    QuestReward,
    ArchiveCopy,
    ExplorationKnowledge,
    NpcKnowledge,
    AssistedOptIn,
    StrategyRuntime,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapOverlayPolicy {
    pub mode: MapOverlayPolicyMode,
    pub source_id: Option<String>,
    pub scope_id: Option<String>,
    pub assisted_label: Option<String>,
    pub player_position_indicator: bool,
    pub viewport_indicator: bool,
    pub quest_markers: bool,
    pub route_lines: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapOverlayPolicyReadback {
    pub mode: MapOverlayPolicyMode,
    pub source_id: Option<String>,
    pub scope_id: Option<String>,
    pub assisted_label: Option<String>,
    pub surface_visible: bool,
    pub player_position_indicator: bool,
    pub viewport_indicator: bool,
    pub quest_markers: bool,
    pub route_lines: bool,
    pub marker_count: usize,
    pub marker_ids: Vec<String>,
}

impl MapOverlayPolicy {
    pub fn authoring_preview() -> Self {
        Self {
            mode: MapOverlayPolicyMode::AuthoringPreview,
            source_id: Some("editor.authoring_preview".to_string()),
            scope_id: Some("editor.current_world".to_string()),
            assisted_label: None,
            player_position_indicator: false,
            viewport_indicator: true,
            quest_markers: true,
            route_lines: true,
        }
    }

    pub fn runtime_rpg_no_map_access() -> Self {
        Self {
            mode: MapOverlayPolicyMode::RuntimeRpgNoMapAccess,
            source_id: None,
            scope_id: None,
            assisted_label: None,
            player_position_indicator: false,
            viewport_indicator: false,
            quest_markers: false,
            route_lines: false,
        }
    }

    pub fn runtime_rpg_diegetic_acquired(source_id: &str, scope_id: &str) -> Self {
        Self {
            mode: MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired,
            source_id: Some(source_id.to_string()),
            scope_id: Some(scope_id.to_string()),
            assisted_label: None,
            player_position_indicator: false,
            viewport_indicator: false,
            quest_markers: false,
            route_lines: false,
        }
    }

    pub fn runtime_assisted(label: &str) -> Self {
        Self {
            mode: MapOverlayPolicyMode::RuntimeAssisted,
            source_id: Some("runtime.assisted_navigation".to_string()),
            scope_id: Some("runtime.current_world".to_string()),
            assisted_label: Some(label.to_string()),
            player_position_indicator: true,
            viewport_indicator: true,
            quest_markers: true,
            route_lines: true,
        }
    }

    pub fn runtime_strategy_minimap() -> Self {
        Self {
            mode: MapOverlayPolicyMode::RuntimeStrategyMinimap,
            source_id: Some("runtime.strategy_minimap".to_string()),
            scope_id: Some("runtime.tactical_world".to_string()),
            assisted_label: None,
            player_position_indicator: true,
            viewport_indicator: true,
            quest_markers: true,
            route_lines: true,
        }
    }
}

pub fn validate_map_overlay_policy(
    policy: &MapOverlayPolicy,
    markers: &[MinimapMarker],
) -> Result<MapOverlayPolicyReadback, EngineError> {
    validate_optional_id("source_id", policy.source_id.as_deref())?;
    validate_optional_id("scope_id", policy.scope_id.as_deref())?;
    validate_optional_id("assisted_label", policy.assisted_label.as_deref())?;
    let surface_visible = match policy.mode {
        MapOverlayPolicyMode::RuntimeRpgNoMapAccess => {
            validate_no_free_rpg_map(policy, markers)?;
            false
        }
        MapOverlayPolicyMode::RuntimeRpgDiegeticAcquired => {
            validate_acquired_rpg_map(policy, markers)?;
            true
        }
        MapOverlayPolicyMode::AuthoringPreview => {
            validate_marker_set(policy, markers, marker_allowed_for_authoring)?;
            true
        }
        MapOverlayPolicyMode::RuntimeAssisted => {
            validate_assisted_policy(policy)?;
            validate_marker_set(policy, markers, marker_allowed_for_assisted)?;
            true
        }
        MapOverlayPolicyMode::RuntimeStrategyMinimap => {
            validate_marker_set(policy, markers, marker_allowed_for_strategy)?;
            true
        }
    };
    Ok(MapOverlayPolicyReadback {
        mode: policy.mode,
        source_id: policy.source_id.clone(),
        scope_id: policy.scope_id.clone(),
        assisted_label: policy.assisted_label.clone(),
        surface_visible,
        player_position_indicator: policy.player_position_indicator,
        viewport_indicator: policy.viewport_indicator,
        quest_markers: policy.quest_markers,
        route_lines: policy.route_lines,
        marker_count: markers.len(),
        marker_ids: markers.iter().map(|marker| marker.id.clone()).collect(),
    })
}

fn validate_no_free_rpg_map(
    policy: &MapOverlayPolicy,
    markers: &[MinimapMarker],
) -> Result<(), EngineError> {
    if policy.source_id.is_some() || policy.scope_id.is_some() || policy.assisted_label.is_some() {
        return invalid_policy(
            "runtime_rpg_no_map_access",
            "fresh RPG map access has no source",
        );
    }
    if policy.player_position_indicator
        || policy.viewport_indicator
        || policy.quest_markers
        || policy.route_lines
    {
        return invalid_policy(
            "runtime_rpg_no_map_access",
            "fresh RPG runtime cannot enable map, minimap, GPS, quest, or route indicators",
        );
    }
    if !markers.is_empty() {
        return invalid_policy(
            "runtime_rpg_no_map_access.markers",
            "fresh RPG runtime cannot contain minimap markers",
        );
    }
    Ok(())
}

fn validate_acquired_rpg_map(
    policy: &MapOverlayPolicy,
    markers: &[MinimapMarker],
) -> Result<(), EngineError> {
    require_id("source_id", policy.source_id.as_deref())?;
    require_id("scope_id", policy.scope_id.as_deref())?;
    if policy.assisted_label.is_some() {
        return invalid_policy(
            "runtime_rpg_diegetic_acquired.assisted_label",
            "diegetic RPG maps are not assisted navigation",
        );
    }
    if policy.player_position_indicator
        || policy.viewport_indicator
        || policy.quest_markers
        || policy.route_lines
    {
        return invalid_policy(
            "runtime_rpg_diegetic_acquired",
            "diegetic RPG maps cannot show player dots, viewport rectangles, quest pins, or route lines",
        );
    }
    validate_marker_set(policy, markers, marker_allowed_for_acquired_rpg)
}

fn validate_assisted_policy(policy: &MapOverlayPolicy) -> Result<(), EngineError> {
    require_id("source_id", policy.source_id.as_deref())?;
    require_id("scope_id", policy.scope_id.as_deref())?;
    require_id("assisted_label", policy.assisted_label.as_deref())
}

fn validate_marker_set(
    policy: &MapOverlayPolicy,
    markers: &[MinimapMarker],
    allowed: fn(&MinimapMarker) -> bool,
) -> Result<(), EngineError> {
    for marker in markers {
        validate_required_id("marker.id", &marker.id)?;
        if marker.allowed_policy != policy.mode {
            return invalid_policy(
                "marker.allowed_policy",
                format!(
                    "{} is allowed for {:?}, not {:?}",
                    marker.id, marker.allowed_policy, policy.mode
                ),
            );
        }
        if !allowed(marker) {
            return invalid_policy(
                "marker.kind",
                format!(
                    "{} marker kind/source {:?}/{:?} is not allowed for {:?}",
                    marker.id, marker.kind, marker.source, policy.mode
                ),
            );
        }
        if matches!(marker.kind, MapOverlayMarkerKind::PlayerPosition)
            && !policy.player_position_indicator
        {
            return invalid_policy("marker.player_position", "policy disables player position");
        }
        if matches!(marker.kind, MapOverlayMarkerKind::ViewportRect) && !policy.viewport_indicator {
            return invalid_policy("marker.viewport", "policy disables viewport indicators");
        }
        if matches!(marker.kind, MapOverlayMarkerKind::QuestObjective) && !policy.quest_markers {
            return invalid_policy("marker.quest", "policy disables quest markers");
        }
        if matches!(marker.kind, MapOverlayMarkerKind::RouteLine) && !policy.route_lines {
            return invalid_policy("marker.route", "policy disables route lines");
        }
    }
    Ok(())
}

fn marker_allowed_for_authoring(marker: &MinimapMarker) -> bool {
    matches!(
        marker.source,
        MapOverlayMarkerSource::EditorAuthoring | MapOverlayMarkerSource::Debug
    ) && matches!(
        marker.kind,
        MapOverlayMarkerKind::Placement
            | MapOverlayMarkerKind::Landmark
            | MapOverlayMarkerKind::Danger
            | MapOverlayMarkerKind::ViewportRect
            | MapOverlayMarkerKind::Debug
    )
}

fn marker_allowed_for_acquired_rpg(marker: &MinimapMarker) -> bool {
    matches!(
        marker.source,
        MapOverlayMarkerSource::InventoryMap
            | MapOverlayMarkerSource::FoundMap
            | MapOverlayMarkerSource::QuestReward
            | MapOverlayMarkerSource::ArchiveCopy
            | MapOverlayMarkerSource::ExplorationKnowledge
            | MapOverlayMarkerSource::NpcKnowledge
    ) && matches!(
        marker.kind,
        MapOverlayMarkerKind::Landmark
            | MapOverlayMarkerKind::Danger
            | MapOverlayMarkerKind::Resource
    )
}

fn marker_allowed_for_assisted(marker: &MinimapMarker) -> bool {
    marker.source == MapOverlayMarkerSource::AssistedOptIn
}

fn marker_allowed_for_strategy(marker: &MinimapMarker) -> bool {
    marker.source == MapOverlayMarkerSource::StrategyRuntime
        && matches!(
            marker.kind,
            MapOverlayMarkerKind::PlayerPosition
                | MapOverlayMarkerKind::ViewportRect
                | MapOverlayMarkerKind::QuestObjective
                | MapOverlayMarkerKind::RouteLine
                | MapOverlayMarkerKind::TacticalUnit
                | MapOverlayMarkerKind::Resource
                | MapOverlayMarkerKind::Danger
        )
}

fn require_id(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    match value {
        Some(value) => validate_required_id(field, value),
        None => invalid_policy(field, "value is required"),
    }
}

fn validate_optional_id(field: &str, value: Option<&str>) -> Result<(), EngineError> {
    match value {
        Some(value) => validate_required_id(field, value),
        None => Ok(()),
    }
}

fn validate_required_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_policy(field, "value must be a non-empty single line");
    }
    Ok(())
}

fn invalid_policy<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("map_overlay_policy.{}", field.into()),
        reason: reason.into(),
    })
}
