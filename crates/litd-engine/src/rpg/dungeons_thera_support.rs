use crate::rpg::{
    AreaAccessRule, AreaConnection, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile,
    AreaResetProfile, AreaRoom, AreaSpec, AreaVisualContract, ConnectionDirection,
    ConnectionDiscovery, GALADON_AREA_ID, RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind,
    TravelKind,
};

pub(crate) const INVALID_CONFIG_CODE: &str = "LITD-RPG-STATS-INVALID-CONFIG";

// #1657 Graveyard MUST reuse the exact area id Galadon's `conn.galadon.to_graveyard`
// external edge targets (GALADON_GRAVEYARD_AREA_ID in the Galadon fixture), so the
// world-graph edge resolves. The crypts test asserts these strings are identical.
pub(crate) const GRAVEYARD_AREA_ID: &str = "area.thera.galadon.graveyard";

// #1657 crypt/undead dungeon ids.
pub(crate) const MAUSOLEUM_AREA_ID: &str = "area.thera.mausoleum";
pub(crate) const FORGOTTEN_CRYPTS_AREA_ID: &str = "area.thera.forgotten_crypts";
pub(crate) const EIL_SHAERIA_AREA_ID: &str = "area.thera.eil_shaeria";
pub(crate) const RUINS_OF_MAETHIEN_AREA_ID: &str = "area.thera.ruins_of_maethien";

// #1658 keep/citadel dungeon ids.
pub(crate) const KEEP_OF_FHALAUGASH_AREA_ID: &str = "area.thera.keep_of_fhalaugash";
pub(crate) const KEEP_OF_THE_RIGHTEOUS_AREA_ID: &str = "area.thera.keep_of_the_righteous";
pub(crate) const RUINED_KEEP_AREA_ID: &str = "area.thera.ruined_keep";
pub(crate) const LORD_TABERSHAW_KEEP_AREA_ID: &str = "area.thera.lord_tabershaw_keep";
pub(crate) const KEEP_OF_CEAWLIN_AREA_ID: &str = "area.thera.keep_of_ceawlin";
pub(crate) const MOUDRILAR_MONASTERY_AREA_ID: &str = "area.thera.moudrilar_monastery";
pub(crate) const IMPERIAL_LANDS_SEWERS_AREA_ID: &str = "area.thera.imperial_lands_sewers";
pub(crate) const HIGH_LORDS_KEEP_AREA_ID: &str = "area.thera.high_lords_keep";

// #1659 tower/arcane dungeon ids.
pub(crate) const TOWER_OF_HIGH_SORCERY_AREA_ID: &str = "area.thera.tower_of_high_sorcery";
pub(crate) const TOWER_OF_DRAGONS_AREA_ID: &str = "area.thera.tower_of_dragons";
pub(crate) const ELEMENTAL_TEMPLE_AREA_ID: &str = "area.thera.elemental_temple";
pub(crate) const MINES_OF_ZAKIIM_AREA_ID: &str = "area.thera.mines_of_zakiim";
pub(crate) const OCTAGONAL_TOWER_AREA_ID: &str = "area.thera.octagonal_tower";
pub(crate) const TOWER_OF_TROTHON_AREA_ID: &str = "area.thera.tower_of_trothon";
pub(crate) const RUINS_OF_DELAR_TOL_AREA_ID: &str = "area.thera.ruins_of_delar_tol";

// World-graph approach nodes (#1632). Referenced-only external targets whose full
// AreaSpecs are authored by other batch issues; here they are registry entries so
// each dungeon has a faithful edge back into the continental graph.
// #1704 registry fix: no separate "voralian_corridor" area is authored; the
// mausoleum corridor surfaces into Voralian City itself (authored #1638).
pub(crate) const VORALIAN_CORRIDOR_AREA_ID: &str = "area.thera.voralian";
pub(crate) const ARKHAM_AREA_ID: &str = "area.thera.arkham";
pub(crate) const EASTSTRIDE_REACH_AREA_ID: &str = "area.thera.eaststride_reach";
pub(crate) const NORTHWEST_REACHES_AREA_ID: &str = "area.thera.northwest_reaches";
pub(crate) const BASILICA_AREA_ID: &str = "area.thera.basilica";
pub(crate) const ATURI_AREA_ID: &str = "area.thera.aturi";
pub(crate) const EASTERN_ROAD_AREA_ID: &str = "area.thera.eastern_road";
pub(crate) const SOUTHERN_REACHES_AREA_ID: &str = "area.thera.southern_reaches";
pub(crate) const WESTERN_REACHES_AREA_ID: &str = "area.thera.western_reaches";
pub(crate) const DRAGON_SEA_AREA_ID: &str = "area.thera.dragon_sea";
pub(crate) const AYR_TRINIL_AREA_ID: &str = "area.thera.ayr_trinil";
pub(crate) const TIR_TALATH_AREA_ID: &str = "area.thera.tir_talath";

/// Every area id these dungeons reference, for cross-area referential integrity
/// (`validate_area_connections_against_registry`). Includes each authored dungeon,
/// the Galadon hub, and the world-graph approach nodes (#1632).
pub(crate) fn dungeon_known_area_ids() -> Vec<&'static str> {
    vec![
        GALADON_AREA_ID,
        GRAVEYARD_AREA_ID,
        MAUSOLEUM_AREA_ID,
        FORGOTTEN_CRYPTS_AREA_ID,
        EIL_SHAERIA_AREA_ID,
        RUINS_OF_MAETHIEN_AREA_ID,
        KEEP_OF_FHALAUGASH_AREA_ID,
        KEEP_OF_THE_RIGHTEOUS_AREA_ID,
        RUINED_KEEP_AREA_ID,
        LORD_TABERSHAW_KEEP_AREA_ID,
        KEEP_OF_CEAWLIN_AREA_ID,
        MOUDRILAR_MONASTERY_AREA_ID,
        IMPERIAL_LANDS_SEWERS_AREA_ID,
        HIGH_LORDS_KEEP_AREA_ID,
        TOWER_OF_HIGH_SORCERY_AREA_ID,
        TOWER_OF_DRAGONS_AREA_ID,
        ELEMENTAL_TEMPLE_AREA_ID,
        MINES_OF_ZAKIIM_AREA_ID,
        OCTAGONAL_TOWER_AREA_ID,
        TOWER_OF_TROTHON_AREA_ID,
        RUINS_OF_DELAR_TOL_AREA_ID,
        VORALIAN_CORRIDOR_AREA_ID,
        ARKHAM_AREA_ID,
        EASTSTRIDE_REACH_AREA_ID,
        NORTHWEST_REACHES_AREA_ID,
        BASILICA_AREA_ID,
        ATURI_AREA_ID,
        EASTERN_ROAD_AREA_ID,
        SOUTHERN_REACHES_AREA_ID,
        WESTERN_REACHES_AREA_ID,
        DRAGON_SEA_AREA_ID,
        AYR_TRINIL_AREA_ID,
        TIR_TALATH_AREA_ID,
    ]
}

/// Inputs for a single dungeon AreaSpec. The constant scaffolding (schema, canon,
/// mechanics source, reset/repop/recall, visual) is filled by `dungeon_spec`; the
/// caller supplies the faithful topology (rooms + connections) and metadata.
pub(crate) struct DungeonSpecInput<'a> {
    pub area_id: &'a str,
    pub display_name: &'a str,
    pub source_issue: u32,
    pub extra_canon: &'a [u32],
    pub level: (u8, u8),
    pub summary: &'a str,
    pub slug: &'a str,
    pub silhouette: &'a str,
    pub extra_props: &'a [&'a str],
    pub notes: &'a [&'a str],
    pub rooms: Vec<AreaRoom>,
    pub connections: Vec<AreaConnection>,
}

pub(crate) fn dungeon_spec(input: DungeonSpecInput) -> AreaSpec {
    let mut canon = vec![987, 1598, 1632, input.source_issue];
    canon.extend_from_slice(input.extra_canon);
    let material = format!(
        "Original Thera {} stonework, warding glyphs, and biers; no copied iconography \
            from the source MUD.",
        input.display_name
    );
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: input.area_id.to_string(),
        display_name: input.display_name.to_string(),
        player_facing_canon_name: input.display_name.to_string(),
        area_kind: AreaKind::Dungeon,
        source_issue: input.source_issue,
        canon_issues: canon,
        mechanics_source_url: "https://wiki.carrionfields.net/game/areas".to_string(),
        source_history_label: format!(
            "CF {} factual level band and denizen roster only",
            input.display_name
        ),
        summary: input.summary.to_string(),
        level_band: AreaLevelBand {
            min_level: input.level.0,
            max_level: input.level.1,
        },
        rooms: input.rooms,
        connections: input.connections,
        resets: dungeon_reset(&format!("reset.{}.cycle", input.slug)),
        repop: dungeon_repop(&format!("repop.{}.denizens", input.slug), 24),
        recall: dungeon_recall(),
        visual: dungeon_visual(input.slug, input.silhouette, &material, input.extra_props),
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: input.notes.iter().map(|note| note.to_string()).collect(),
    }
}

fn dungeon_reset(id: &str) -> AreaResetProfile {
    AreaResetProfile {
        reset_id: id.to_string(),
        interval_minutes: 45,
        clears_unlooted_corpses: true,
        resets_doors_and_locks: true,
    }
}

fn dungeon_repop(id: &str, budget: u16) -> AreaRepopProfile {
    AreaRepopProfile {
        repop_id: id.to_string(),
        baseline_population_budget: budget,
        repop_interval_minutes: 30,
        max_pressure_tier: 2,
    }
}

// Dungeons are not hometowns and grant no recall point (#1634): recall_room_id None,
// has_academy false, hometown_access Any.
fn dungeon_recall() -> AreaRecallProfile {
    AreaRecallProfile {
        is_hometown_option: false,
        recall_room_id: None,
        has_academy: false,
        hometown_access: AreaAccessRule::Any,
    }
}

fn dungeon_visual(
    slug: &str,
    silhouette: &str,
    material_notes: &str,
    extra_props: &[&str],
) -> AreaVisualContract {
    let mut props = vec![
        format!("prop.dungeon.{slug}.entry_arch"),
        "prop.dungeon.torch_sconce".to_string(),
        "prop.dungeon.rubble_pile".to_string(),
    ];
    props.extend(extra_props.iter().map(|prop| prop.to_string()));
    AreaVisualContract {
        renderer: "Rust/wgpu".to_string(),
        tileset_id: format!("tileset.dungeon.{slug}.stone"),
        model_id: format!("model.dungeon.{slug}"),
        required_props: props,
        silhouette: silhouette.to_string(),
        material_notes: material_notes.to_string(),
    }
}

pub(crate) fn room(
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

pub(crate) fn internal(
    connection_id: &str,
    area_id: &str,
    from_room_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
) -> AreaConnection {
    conn(
        connection_id,
        from_room_id,
        area_id,
        Some(to_room_id),
        direction,
        travel_kind,
        ConnectionDiscovery::Visible,
        None,
    )
}

pub(crate) fn hidden_internal(
    connection_id: &str,
    area_id: &str,
    from_room_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    conn(
        connection_id,
        from_room_id,
        area_id,
        Some(to_room_id),
        direction,
        travel_kind,
        discovery,
        None,
    )
}

pub(crate) fn locked_internal(
    connection_id: &str,
    area_id: &str,
    from_room_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    locked_by_ref: &str,
) -> AreaConnection {
    conn(
        connection_id,
        from_room_id,
        area_id,
        Some(to_room_id),
        direction,
        travel_kind,
        ConnectionDiscovery::Visible,
        Some(locked_by_ref),
    )
}

pub(crate) fn external(
    connection_id: &str,
    from_room_id: &str,
    to_area_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    conn(
        connection_id,
        from_room_id,
        to_area_id,
        None,
        direction,
        travel_kind,
        discovery,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn conn(
    connection_id: &str,
    from_room_id: &str,
    to_area_id: &str,
    to_room_id: Option<&str>,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
    locked_by_ref: Option<&str>,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: to_area_id.to_string(),
        to_room_id: to_room_id.map(str::to_string),
        direction,
        travel_kind,
        discovery,
        locked_by_ref: locked_by_ref.map(str::to_string),
    }
}

// Shared keep interior (#1658): gate -> courtyard -> guard hall -> inner ward ->
// lord's hall, plus a locked cell block off the ward and a hidden sally port (#944).
// `mook`/`elite`/`boss` are the patrol/officer/lord npc population ids.
pub(crate) fn keep_rooms(slug: &str, mook: &str, elite: &str, boss: &str) -> Vec<AreaRoom> {
    vec![
        room(
            &room_id(slug, "gate"),
            "Keep Gate",
            RoomKind::Gate,
            "The gate-tower and portcullis where sentries challenge all who approach.",
            RoomConcealment::GateChokepoint,
            &[mook],
        ),
        room(
            &room_id(slug, "courtyard"),
            "Inner Courtyard",
            RoomKind::Intersection,
            "A patrolled courtyard overlooked by the wall-walks; open ground, little cover.",
            RoomConcealment::None,
            &[mook, elite],
        ),
        room(
            &room_id(slug, "guard_hall"),
            "Guard Hall",
            RoomKind::Chamber,
            "The garrison hall where off-duty guards watch the doors.",
            RoomConcealment::None,
            &[mook, elite],
        ),
        room(
            &room_id(slug, "inner_ward"),
            "Inner Ward",
            RoomKind::Chamber,
            "The keep's inner ward, climbing by stair toward the lord's hall.",
            RoomConcealment::None,
            &[elite],
        ),
        room(
            &room_id(slug, "lords_hall"),
            "Lord's Hall",
            RoomKind::Chamber,
            "The great hall where the keep's lord holds court under guard.",
            RoomConcealment::None,
            &[boss],
        ),
        room(
            &room_id(slug, "cells"),
            "Cell Block",
            RoomKind::Landmark,
            "A locked block of holding cells; captives wait behind iron doors.",
            RoomConcealment::HiddenLayer,
            &[],
        ),
    ]
}

pub(crate) fn keep_connections(
    slug: &str,
    area_id: &str,
    approach: &str,
    approach_dir: ConnectionDirection,
) -> Vec<AreaConnection> {
    vec![
        external(
            &conn_id(slug, "approach"),
            &room_id(slug, "gate"),
            approach,
            approach_dir,
            TravelKind::Road,
            ConnectionDiscovery::Visible,
        ),
        internal(
            &conn_id(slug, "enter"),
            area_id,
            &room_id(slug, "gate"),
            &room_id(slug, "courtyard"),
            ConnectionDirection::In,
            TravelKind::Gate,
        ),
        internal(
            &conn_id(slug, "to_hall"),
            area_id,
            &room_id(slug, "courtyard"),
            &room_id(slug, "guard_hall"),
            ConnectionDirection::North,
            TravelKind::Path,
        ),
        internal(
            &conn_id(slug, "to_ward"),
            area_id,
            &room_id(slug, "guard_hall"),
            &room_id(slug, "inner_ward"),
            ConnectionDirection::Up,
            TravelKind::Stair,
        ),
        internal(
            &conn_id(slug, "to_lord"),
            area_id,
            &room_id(slug, "inner_ward"),
            &room_id(slug, "lords_hall"),
            ConnectionDirection::In,
            TravelKind::Path,
        ),
        locked_internal(
            &conn_id(slug, "cell_door"),
            area_id,
            &room_id(slug, "inner_ward"),
            &room_id(slug, "cells"),
            ConnectionDirection::Down,
            TravelKind::Stair,
            &format!("lock.{slug}.cells"),
        ),
        hidden_internal(
            &conn_id(slug, "sally_port"),
            area_id,
            &room_id(slug, "courtyard"),
            &room_id(slug, "cells"),
            ConnectionDirection::Down,
            TravelKind::Descend,
            ConnectionDiscovery::Hidden,
        ),
    ]
}

pub(crate) fn room_id(slug: &str, name: &str) -> String {
    format!("room.{slug}.{name}")
}

pub(crate) fn conn_id(slug: &str, name: &str) -> String {
    format!("conn.{slug}.{name}")
}
