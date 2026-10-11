// Minimal factual AreaSpecs for the two Galadon satellite areas the #1636 hub
// fixture connects out to but no batch had authored: the Lumber Yard and the
// Kobold Warrens (#1704 caught both as real dangling cross-area edges). Both are
// canonical CF newbie (level 1-5) areas; their factual placement is transcribed
// verbatim from the CF `NEWBIE AREAS` helpfile (help id 2523):
//   Lumber Yard:    "Two south from the Large Intersection in Galadon, then east."
//   Kobold Warrens: "Proceed out the northwest gate of Galadon ... Go west into
//                    the forest and take the first north."
// Only factual topology/placement is transcribed; all room dressing/props are
// original Thera material (no copied CF prose or iconography).

use litd_engine::rpg::{
    AreaAccessRule, AreaConnection, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile,
    AreaResetProfile, AreaRoom, AreaSpec, AreaVisualContract, ConnectionDirection,
    ConnectionDiscovery, GALADON_AREA_ID, RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind,
    TravelKind,
};

pub const GALADON_LUMBER_YARD_AREA_ID: &str = "area.thera.galadon.lumber_yard";
pub const GALADON_KOBOLD_WARRENS_AREA_ID: &str = "area.thera.galadon.kobold_warrens";

/// The two satellite specs, for aggregation into the full authored corpus.
pub fn galadon_satellite_area_specs() -> Vec<AreaSpec> {
    vec![lumber_yard_area_spec(), kobold_warrens_area_spec()]
}

/// Lumber Yard: a low-level working logging camp two south then east of the
/// Large Intersection (help id 2523). Reached from Galadon's south gate.
pub fn lumber_yard_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: GALADON_LUMBER_YARD_AREA_ID.to_string(),
        display_name: "The Lumber Yard".to_string(),
        player_facing_canon_name: "The Lumber Yard".to_string(),
        area_kind: AreaKind::Wilderness,
        source_issue: 1704,
        canon_issues: vec![987, 1598, 1632, 1636, 1704],
        mechanics_source_url: "https://wiki.carrionfields.net/game/newbie".to_string(),
        source_history_label: "CF Lumber Yard newbie-area factual placement only (help id 2523)"
            .to_string(),
        summary: "A working logging camp on Galadon's southern skirts: felled timber, a sawpit, \
            and easy work for beginning adventurers."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 5,
        },
        rooms: vec![
            room(
                "room.galadon.lumber_yard.entrance",
                "Lumber Yard Gate",
                RoomKind::Landmark,
                "A rutted cart track opens onto the yard; the road back north climbs toward the \
                city's south gate.",
                RoomConcealment::None,
                &["npc.galadon.lumber_yard.foreman"],
            ),
            room(
                "room.galadon.lumber_yard.sawpit",
                "The Sawpit",
                RoomKind::Wilderness,
                "Stacked logs, a pit saw, and sawdust underfoot; woodcutters work here through \
                the day.",
                RoomConcealment::None,
                &["npc.galadon.lumber_yard.woodcutter"],
            ),
        ],
        connections: vec![
            internal(
                "conn.galadon.lumber_yard.entrance_to_sawpit",
                GALADON_LUMBER_YARD_AREA_ID,
                "room.galadon.lumber_yard.entrance",
                "room.galadon.lumber_yard.sawpit",
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            // Back-edge to the hub city (the #1636 Galadon fixture wires the
            // forward edge out of its south gate).
            external(
                "conn.galadon.lumber_yard.to_galadon",
                "room.galadon.lumber_yard.entrance",
                GALADON_AREA_ID,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets: reset("reset.galadon.lumber_yard.cycle"),
        repop: repop("repop.galadon.lumber_yard.population"),
        recall: no_recall(),
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.wilderness.galadon.lumber_yard".to_string(),
            model_id: "model.wilderness.galadon.lumber_yard".to_string(),
            required_props: vec![
                "prop.wilderness.log_stack".to_string(),
                "prop.wilderness.pit_saw".to_string(),
                "prop.wilderness.sawdust_pile".to_string(),
            ],
            silhouette: "a cleared logging camp of stacked timber and a sawpit south of the city"
                .to_string(),
            material_notes: "Original Thera-Galadon lumber-camp dressing; no copied iconography."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Factual placement per CF help id 2523: two south from the Large Intersection, then \
                east. Authored by #1704 to resolve the Galadon->lumber_yard dangling edge."
                .to_string(),
        ],
    }
}

/// Kobold Warrens: a low-level forest warren reached out the northwest gate,
/// west into the forest, first north (help id 2523).
pub fn kobold_warrens_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: GALADON_KOBOLD_WARRENS_AREA_ID.to_string(),
        display_name: "The Kobold Warrens".to_string(),
        player_facing_canon_name: "The Kobold Warrens".to_string(),
        area_kind: AreaKind::Lair,
        source_issue: 1704,
        canon_issues: vec![987, 1598, 1632, 1636, 1704],
        mechanics_source_url: "https://wiki.carrionfields.net/game/newbie".to_string(),
        source_history_label: "CF Kobold Warrens newbie-area factual placement only (help id 2523)"
            .to_string(),
        summary: "A cramped forest warren of kobolds northwest of Galadon: burrow mouths in the \
            treeline and tunnels beyond, a first proving-ground for beginners."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 5,
        },
        rooms: vec![
            room(
                "room.galadon.kobold_warrens.forest_edge",
                "Forest Edge",
                RoomKind::Wilderness,
                "The treeline west of the northwest gate; a trodden path turns north toward burrow \
                mouths in the bank.",
                RoomConcealment::None,
                &["npc.galadon.kobold_warrens.kobold_sentry"],
            ),
            room(
                "room.galadon.kobold_warrens.burrow",
                "Kobold Burrow",
                RoomKind::Chamber,
                "A low earthen tunnel reeking of kobold; the ceiling forces a stoop.",
                RoomConcealment::HiddenLayer,
                &["npc.galadon.kobold_warrens.kobold_digger"],
            ),
        ],
        connections: vec![
            internal(
                "conn.galadon.kobold_warrens.edge_to_burrow",
                GALADON_KOBOLD_WARRENS_AREA_ID,
                "room.galadon.kobold_warrens.forest_edge",
                "room.galadon.kobold_warrens.burrow",
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            // Back-edge to the hub city (the #1636 Galadon fixture wires the
            // forward edge out of its northwest gate).
            external(
                "conn.galadon.kobold_warrens.to_galadon",
                "room.galadon.kobold_warrens.forest_edge",
                GALADON_AREA_ID,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets: reset("reset.galadon.kobold_warrens.cycle"),
        repop: repop("repop.galadon.kobold_warrens.population"),
        recall: no_recall(),
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.lair.galadon.kobold_warrens".to_string(),
            model_id: "model.lair.galadon.kobold_warrens".to_string(),
            required_props: vec![
                "prop.lair.burrow_mouth".to_string(),
                "prop.lair.bone_midden".to_string(),
                "prop.lair.dug_earth".to_string(),
            ],
            silhouette: "burrow mouths in a wooded bank opening onto cramped kobold tunnels"
                .to_string(),
            material_notes: "Original Thera-Galadon kobold-warren dressing; no copied iconography."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Factual placement per CF help id 2523: out the northwest gate, west into the forest, \
                first north. Authored by #1704 to resolve the Galadon->kobold_warrens dangling edge."
                .to_string(),
        ],
    }
}

fn room(
    room_id: &str,
    display_name: &str,
    room_kind: RoomKind,
    summary: &str,
    concealment: RoomConcealment,
    npcs: &[&str],
) -> AreaRoom {
    AreaRoom {
        room_id: room_id.to_string(),
        display_name: display_name.to_string(),
        room_kind,
        summary: summary.to_string(),
        concealment,
        npc_population_ids: npcs.iter().map(|id| id.to_string()).collect(),
    }
}

#[allow(clippy::too_many_arguments)]
fn internal(
    connection_id: &str,
    area_id: &str,
    from_room_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: area_id.to_string(),
        to_room_id: Some(to_room_id.to_string()),
        direction,
        travel_kind,
        discovery,
        locked_by_ref: None,
    }
}

fn external(
    connection_id: &str,
    from_room_id: &str,
    to_area_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: to_area_id.to_string(),
        to_room_id: None,
        direction,
        travel_kind,
        discovery,
        locked_by_ref: None,
    }
}

fn reset(reset_id: &str) -> AreaResetProfile {
    AreaResetProfile {
        reset_id: reset_id.to_string(),
        interval_minutes: 15,
        clears_unlooted_corpses: true,
        resets_doors_and_locks: false,
    }
}

fn repop(repop_id: &str) -> AreaRepopProfile {
    AreaRepopProfile {
        repop_id: repop_id.to_string(),
        baseline_population_budget: 12,
        repop_interval_minutes: 10,
        max_pressure_tier: 1,
    }
}

fn no_recall() -> AreaRecallProfile {
    AreaRecallProfile {
        is_hometown_option: false,
        recall_room_id: None,
        has_academy: false,
        hometown_access: AreaAccessRule::Any,
    }
}
