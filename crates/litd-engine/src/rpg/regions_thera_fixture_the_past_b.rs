// #1660 world/region: The Past, a time-travel zone, part B (Plains of
// Arendyl, Citadel of Ostalagiah, The Battlefield). Split from
// regions_thera_fixture_the_past.rs (part A: Wagon-Marked Road, Past Grove,
// Maethien, Delar-Tol) to respect the <500-line ratchet (#1466/#800 sec.6)
// after rustfmt expansion.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::regions_thera_fixture_the_past::DELAR_TOL_PAST_AREA_ID;
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const PLAINS_OF_ARENDYL_AREA_ID: &str = "area.thera.past.plains_of_arendyl";
pub(crate) const CITADEL_OF_OSTALAGIAH_AREA_ID: &str = "area.thera.past.citadel_of_ostalagiah";
pub(crate) const THE_BATTLEFIELD_AREA_ID: &str = "area.thera.past.the_battlefield";

pub(crate) fn the_past_b_area_ids() -> [&'static str; 3] {
    [
        PLAINS_OF_ARENDYL_AREA_ID,
        CITADEL_OF_OSTALAGIAH_AREA_ID,
        THE_BATTLEFIELD_AREA_ID,
    ]
}

pub(crate) fn plains_of_arendyl_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: PLAINS_OF_ARENDYL_AREA_ID,
        display_name: "Plains of Arendyl",
        area_kind: AreaKind::Wilderness,
        source_issue: 1660,
        canon_issues: vec![987, 1598, 1632, 1660],
        wiki_slug: "plains-of-arendyl",
        source_history_label: "CF Plains of Arendyl factual topology and mechanics only",
        summary: "CF ranking guide places feasting elves and elven guards here at 25-35 evil ranking.",
        level_band: (25, 35),
        rooms: vec![
            room(
                "room.plains_of_arendyl.feast_ground",
                "Feasting Ground",
                RoomKind::Wilderness,
                "An open plain where feasting elves gather.",
                RoomConcealment::None,
                &["npc.plains_of_arendyl.feasting_elf"],
            ),
            room(
                "room.plains_of_arendyl.guard_line",
                "Elven Guard Line",
                RoomKind::Landmark,
                "A patrol line of elven guards.",
                RoomConcealment::None,
                &["npc.plains_of_arendyl.elven_guard"],
            ),
        ],
        connections: vec![
            conn(
                "conn.plains_of_arendyl.feast_to_guard",
                "room.plains_of_arendyl.feast_ground",
                PLAINS_OF_ARENDYL_AREA_ID,
                Some("room.plains_of_arendyl.guard_line"),
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.plains_of_arendyl.to_delar_tol",
                "room.plains_of_arendyl.feast_ground",
                DELAR_TOL_PAST_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.plains_of_arendyl.to_citadel_of_ostalagiah",
                "room.plains_of_arendyl.guard_line",
                CITADEL_OF_OSTALAGIAH_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.plains_of_arendyl.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.plains_of_arendyl.population",
        repop_budget: 20,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.plains_of_arendyl",
        model_id: "model.wilderness.plains_of_arendyl",
        props: vec![
            "prop.plains.feast_table_ruin_era",
            "prop.past.era_banner",
            "prop.plains.tall_grass_patch",
        ],
        silhouette: "open era-earlier plains with an elven feasting ground and guard line",
        material_notes: "Original Thera-Arendyl Past-era dressing; no copied iconography.",
        notes: vec!["Era-consistent NPCs only per the design brief."],
    })
}

pub(crate) fn citadel_of_ostalagiah_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: CITADEL_OF_OSTALAGIAH_AREA_ID,
        display_name: "Citadel of Ostalagiah",
        area_kind: AreaKind::Dungeon,
        source_issue: 1660,
        canon_issues: vec![987, 1598, 1632, 1660],
        wiki_slug: "citadel-of-ostalagiah",
        source_history_label: "CF Citadel of Ostalagiah factual topology and mechanics only",
        summary: "A fortified citadel on the Past map; source gives no explicit CF level range for \
            this node, so the band below is estimated from regional context, not a cited number.",
        level_band: (30, 51),
        rooms: vec![
            room(
                "room.citadel_of_ostalagiah.outer_wall",
                "Outer Wall",
                RoomKind::Gate,
                "The citadel's fortified outer wall.",
                RoomConcealment::GateChokepoint,
                &["npc.citadel_of_ostalagiah.era_consistent_sentry"],
            ),
            room(
                "room.citadel_of_ostalagiah.inner_keep",
                "Inner Keep",
                RoomKind::Landmark,
                "The citadel's inner keep.",
                RoomConcealment::None,
                &["npc.citadel_of_ostalagiah.era_consistent_lord"],
            ),
        ],
        connections: vec![
            conn(
                "conn.citadel_of_ostalagiah.wall_to_keep",
                "room.citadel_of_ostalagiah.outer_wall",
                CITADEL_OF_OSTALAGIAH_AREA_ID,
                Some("room.citadel_of_ostalagiah.inner_keep"),
                ConnectionDirection::In,
                TravelKind::Gate,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.citadel_of_ostalagiah.to_plains_of_arendyl",
                "room.citadel_of_ostalagiah.outer_wall",
                PLAINS_OF_ARENDYL_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.citadel_of_ostalagiah.to_the_battlefield",
                "room.citadel_of_ostalagiah.inner_keep",
                THE_BATTLEFIELD_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.citadel_of_ostalagiah.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.citadel_of_ostalagiah.population",
        repop_budget: 20,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.dungeon.citadel_of_ostalagiah",
        model_id: "model.dungeon.citadel_of_ostalagiah",
        props: vec![
            "prop.past.fortified_wall",
            "prop.past.era_banner",
            "prop.past.keep_throne",
        ],
        silhouette: "fortified era-earlier citadel with an outer wall and inner keep",
        material_notes: "Original Thera-Ostalagiah dressing; no copied iconography.",
        notes: vec!["Doors and locks reset here (fortress seals)."],
    })
}

pub(crate) fn the_battlefield_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: THE_BATTLEFIELD_AREA_ID,
        display_name: "The Battlefield",
        area_kind: AreaKind::Wilderness,
        source_issue: 1660,
        canon_issues: vec![987, 1598, 1632, 1660],
        wiki_slug: "the-battlefield",
        source_history_label: "CF The Battlefield factual topology and mechanics only",
        summary: "The past war set-piece; CF ranking guide places feared swordsmen, large guards, and \
            dwarves here at 30-40 evil ranking, spanning 20-51 overall.",
        level_band: (20, 51),
        rooms: vec![
            room(
                "room.the_battlefield.front_line",
                "Front Line",
                RoomKind::Wilderness,
                "The still-raging front line of the past war.",
                RoomConcealment::None,
                &["npc.the_battlefield.feared_swordsman"],
            ),
            room(
                "room.the_battlefield.siege_camp",
                "Siege Camp",
                RoomKind::Landmark,
                "A dwarven siege camp behind the line.",
                RoomConcealment::None,
                &["npc.the_battlefield.dwarf_soldier"],
            ),
        ],
        connections: vec![
            conn(
                "conn.the_battlefield.line_to_camp",
                "room.the_battlefield.front_line",
                THE_BATTLEFIELD_AREA_ID,
                Some("room.the_battlefield.siege_camp"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.the_battlefield.to_citadel_of_ostalagiah",
                "room.the_battlefield.front_line",
                CITADEL_OF_OSTALAGIAH_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.the_battlefield.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.the_battlefield.population",
        repop_budget: 26,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.wilderness.the_battlefield",
        model_id: "model.wilderness.the_battlefield",
        props: vec![
            "prop.past.war_banner",
            "prop.past.siege_engine",
            "prop.past.battlefield_debris",
        ],
        silhouette: "past-war battlefield with a dwarven siege camp behind the front line",
        material_notes: "Original Thera-Battlefield dressing; no copied iconography.",
        notes: vec!["The past war set-piece per the design brief; era-consistent NPCs only."],
    })
}

pub(crate) fn all_the_past_b_specs() -> Vec<AreaSpec> {
    vec![
        plains_of_arendyl_area_spec(),
        citadel_of_ostalagiah_area_spec(),
        the_battlefield_area_spec(),
    ]
}
