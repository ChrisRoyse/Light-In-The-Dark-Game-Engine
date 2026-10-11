// #1660 world/region: The Past, a time-travel zone, part A (Wagon-Marked
// Road, Past Grove, Maethien, Delar-Tol). Split from a single file to respect
// the <500-line ratchet (#1466/#800 sec.6) after rustfmt expansion; part B
// (Plains of Arendyl, Citadel of Ostalagiah, The Battlefield) lives in
// regions_thera_fixture_the_past_b.rs. CF-factual: the map carries a separate
// THE PAST section (TO PAST portal) - the same places, an age earlier. The
// engine feature this calls for is REGION VARIANTS (same geography, different
// era/state). #1725 added `variant_of_area_id`/`variant_label` to the AreaSpec
// schema: Maethien and Delar-Tol below now carry a STRUCTURAL variant link to
// their present-day Ruins counterparts (authored by the #1657 dungeons batch),
// resolved and dangling-checked by the #1704 corpus registry. The remaining
// Past places whose present-day twin is not yet authored keep the era in prose
// until that twin lands.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::regions_thera_fixture_the_past_b::PLAINS_OF_ARENDYL_AREA_ID;
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const WAGON_MARKED_ROAD_AREA_ID: &str = "area.thera.past.wagon_marked_road";
pub(crate) const PAST_GROVE_AREA_ID: &str = "area.thera.past.past_grove";
pub(crate) const MAETHIEN_PAST_AREA_ID: &str = "area.thera.past.maethien";
pub(crate) const DELAR_TOL_PAST_AREA_ID: &str = "area.thera.past.delar_tol";

// #1725 region-variant base areas: the present-day counterparts of the two
// Past-era towns above, authored by the #1657-#1659 dungeons batch
// (`dungeons_thera_support::{RUINS_OF_MAETHIEN_AREA_ID, RUINS_OF_DELAR_TOL_AREA_ID}`).
// That batch is a sibling `#[cfg(test)]` module, so its consts are not importable
// here; the ids are mirrored as literals and the #1704 corpus registry resolves
// the `variant_of_area_id` link, failing loud on any drift between the two.
const RUINS_OF_MAETHIEN_PRESENT_AREA_ID: &str = "area.thera.ruins_of_maethien";
const RUINS_OF_DELAR_TOL_PRESENT_AREA_ID: &str = "area.thera.ruins_of_delar_tol";

pub(crate) fn the_past_a_area_ids() -> [&'static str; 4] {
    [
        WAGON_MARKED_ROAD_AREA_ID,
        PAST_GROVE_AREA_ID,
        MAETHIEN_PAST_AREA_ID,
        DELAR_TOL_PAST_AREA_ID,
    ]
}

pub(crate) fn wagon_marked_road_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: WAGON_MARKED_ROAD_AREA_ID,
        display_name: "The Galadonian Wagon-Marked Road (Past)",
        area_kind: AreaKind::Road,
        source_issue: 1660,
        canon_issues: vec![987, 1598, 1632, 1660],
        wiki_slug: "wagon-marked-road-past",
        source_history_label: "CF Wagon-Marked Road (The Past section) factual topology and \
            mechanics only",
        summary: "The portal landing point for TO PAST travel: the same Galadonian road, an age \
            earlier, still rutted with wagon tracks.",
        level_band: (1, 30),
        rooms: vec![
            room(
                "room.wagon_marked_road.portal_landing",
                "Portal Landing",
                RoomKind::Landmark,
                "Where the TO PAST portal deposits travelers.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
            room(
                "room.wagon_marked_road.rutted_track",
                "Rutted Wagon Track",
                RoomKind::Street,
                "A road worn deep by generations of wagon wheels.",
                RoomConcealment::None,
                &["npc.wagon_marked_road.era_consistent_wagoner"],
            ),
        ],
        connections: vec![
            conn(
                "conn.wagon_marked_road.landing_to_track",
                "room.wagon_marked_road.portal_landing",
                WAGON_MARKED_ROAD_AREA_ID,
                Some("room.wagon_marked_road.rutted_track"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.wagon_marked_road.to_galadon",
                "room.wagon_marked_road.portal_landing",
                GALADON_AREA_ID,
                None,
                ConnectionDirection::Out,
                TravelKind::Portal,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.wagon_marked_road.to_past_grove",
                "room.wagon_marked_road.rutted_track",
                PAST_GROVE_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.wagon_marked_road.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.wagon_marked_road.population",
        repop_budget: 16,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.road.wagon_marked_road_past",
        model_id: "model.road.wagon_marked_road_past",
        props: vec![
            "prop.past.wagon_rut",
            "prop.past.portal_shimmer",
            "prop.past.era_lantern_post",
        ],
        silhouette: "deep-rutted wagon road under a shimmering portal landing",
        material_notes: "Original Thera-Past road dressing, deliberately era-flavored older than the \
            present-day Galadon roads; no copied iconography.",
        notes: vec![
            "Travel via TO PAST portal (#944: Hidden discovery, Portal travel kind); NPCs on \
            this side are era-consistent, not present-day duplicates.",
        ],
    })
}

pub(crate) fn past_grove_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: PAST_GROVE_AREA_ID,
        display_name: "Past Grove",
        area_kind: AreaKind::Wilderness,
        source_issue: 1660,
        canon_issues: vec![987, 1598, 1632, 1660],
        wiki_slug: "past-grove",
        source_history_label: "CF Past Grove (paired with the present-day Present Grove) factual \
            topology and mechanics only",
        summary: "The era-earlier twin of the present-day Present Grove; part of the Shadow/Hidden \
            Grove-Forest pair on the Past map.",
        level_band: (10, 30),
        rooms: vec![
            room(
                "room.past_grove.younger_canopy",
                "Younger Canopy",
                RoomKind::Wilderness,
                "A grove whose trees are visibly younger than their present-day counterpart.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.past_grove.shadow_hollow",
                "Shadow Hollow",
                RoomKind::Landmark,
                "A shadowed hollow, half of the Shadow/Hidden Grove-Forest pair.",
                RoomConcealment::AlleyShadow,
                &["npc.past_grove.era_consistent_druid"],
            ),
        ],
        connections: vec![
            conn(
                "conn.past_grove.canopy_to_hollow",
                "room.past_grove.younger_canopy",
                PAST_GROVE_AREA_ID,
                Some("room.past_grove.shadow_hollow"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.past_grove.to_wagon_marked_road",
                "room.past_grove.younger_canopy",
                WAGON_MARKED_ROAD_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.past_grove.to_maethien",
                "room.past_grove.shadow_hollow",
                MAETHIEN_PAST_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.past_grove.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.past_grove.population",
        repop_budget: 16,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.past_grove",
        model_id: "model.wilderness.past_grove",
        props: vec![
            "prop.past.younger_sapling",
            "prop.past.shadow_hollow_stone",
            "prop.forest.game_trail",
        ],
        silhouette: "grove of visibly younger trees paired with a shadowed hollow",
        material_notes: "Original Thera-Past-Grove dressing, era-flavored distinct from the present-day \
            Present Grove; no copied iconography.",
        notes: vec!["Era-consistent NPCs only; not a duplicate of the present-day Present Grove."],
    })
}

pub(crate) fn maethien_past_area_spec() -> AreaSpec {
    let mut spec = build_wild_area(AreaBlueprint {
        area_id: MAETHIEN_PAST_AREA_ID,
        display_name: "Maethien",
        area_kind: AreaKind::Town,
        source_issue: 1660,
        canon_issues: vec![987, 1598, 1632, 1660],
        wiki_slug: "maethien-the-past",
        source_history_label: "CF Maethien, whole (The Past section) factual topology and mechanics \
            only. Distinct from the present-day Ruins of Maethien authored under #1657 (a sibling \
            dungeons batch); this is the intact Past-era town, not a shared or renamed area.",
        summary: "Maethien whole and intact, an age before it fell to ruin.",
        level_band: (20, 40),
        rooms: vec![
            room(
                "room.maethien_past.town_square",
                "Maethien Town Square",
                RoomKind::Intersection,
                "The still-standing town square, before the fall.",
                RoomConcealment::CrowdCover,
                &["npc.maethien_past.era_consistent_townsfolk"],
            ),
            room(
                "room.maethien_past.hall",
                "Maethien Hall",
                RoomKind::Landmark,
                "The intact hall at the town's heart.",
                RoomConcealment::None,
                &["npc.maethien_past.era_consistent_elder"],
            ),
        ],
        connections: vec![
            conn(
                "conn.maethien_past.square_to_hall",
                "room.maethien_past.town_square",
                MAETHIEN_PAST_AREA_ID,
                Some("room.maethien_past.hall"),
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.maethien_past.to_past_grove",
                "room.maethien_past.town_square",
                PAST_GROVE_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.maethien_past.to_delar_tol",
                "room.maethien_past.hall",
                DELAR_TOL_PAST_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.maethien_past.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.maethien_past.population",
        repop_budget: 22,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.town.maethien_past",
        model_id: "model.town.maethien_past",
        props: vec![
            "prop.past.intact_town_hall",
            "prop.past.era_banner",
            "prop.past.cobblestone_square",
        ],
        silhouette: "intact era-earlier town, whole and unruined",
        material_notes: "Original Thera-Maethien Past-era dressing, deliberately distinct from any \
            present-day ruin state; no copied iconography.",
        notes: vec![
            "Region-variant of the present-day Ruins of Maethien (#1657, a sibling batch). The \
            #1725 structural variant_of_area_id link (below) makes the pairing queryable; the \
            #1704 corpus registry resolves it and fails loud on a dangling base.",
        ],
    });
    // #1725: structural region-variant link to the present-day counterpart.
    spec.variant_of_area_id = Some(RUINS_OF_MAETHIEN_PRESENT_AREA_ID.to_string());
    spec.variant_label = Some("past".to_string());
    spec
}

pub(crate) fn delar_tol_past_area_spec() -> AreaSpec {
    let mut spec = build_wild_area(AreaBlueprint {
        area_id: DELAR_TOL_PAST_AREA_ID,
        display_name: "Delar-Tol",
        area_kind: AreaKind::Town,
        source_issue: 1660,
        canon_issues: vec![987, 1598, 1632, 1660],
        wiki_slug: "delar-tol-the-past",
        source_history_label: "CF Delar-Tol intact (The Past section) factual topology and mechanics \
            only, distinct from the present-day Ruins of Delar-Tol.",
        summary: "Delar-Tol intact, an age before it fell to ruin.",
        level_band: (20, 40),
        rooms: vec![
            room(
                "room.delar_tol_past.gate",
                "Delar-Tol Gate",
                RoomKind::Gate,
                "The still-standing gate of Delar-Tol.",
                RoomConcealment::GateChokepoint,
                &["npc.delar_tol_past.era_consistent_guard"],
            ),
            room(
                "room.delar_tol_past.market",
                "Delar-Tol Market",
                RoomKind::Market,
                "A bustling market, before the fall.",
                RoomConcealment::CrowdCover,
                &["npc.delar_tol_past.era_consistent_trader"],
            ),
        ],
        connections: vec![
            conn(
                "conn.delar_tol_past.gate_to_market",
                "room.delar_tol_past.gate",
                DELAR_TOL_PAST_AREA_ID,
                Some("room.delar_tol_past.market"),
                ConnectionDirection::In,
                TravelKind::Gate,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.delar_tol_past.to_maethien",
                "room.delar_tol_past.gate",
                MAETHIEN_PAST_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.delar_tol_past.to_plains_of_arendyl",
                "room.delar_tol_past.market",
                PLAINS_OF_ARENDYL_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.delar_tol_past.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.delar_tol_past.population",
        repop_budget: 22,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.town.delar_tol_past",
        model_id: "model.town.delar_tol_past",
        props: vec![
            "prop.past.intact_gatehouse",
            "prop.past.market_stall_era",
            "prop.past.era_banner",
        ],
        silhouette: "intact era-earlier walled town with a bustling market",
        material_notes: "Original Thera-Delar-Tol Past-era dressing; no copied iconography.",
        notes: vec![
            "Region-variant of the present-day Ruins of Delar-Tol (#1657, a sibling batch), linked \
            structurally via the #1725 variant_of_area_id field (below) and resolved by the #1704 \
            corpus registry.",
        ],
    });
    // #1725: structural region-variant link to the present-day counterpart.
    spec.variant_of_area_id = Some(RUINS_OF_DELAR_TOL_PRESENT_AREA_ID.to_string());
    spec.variant_label = Some("past".to_string());
    spec
}

pub(crate) fn all_the_past_a_specs() -> Vec<AreaSpec> {
    vec![
        wagon_marked_road_area_spec(),
        past_grove_area_spec(),
        maethien_past_area_spec(),
        delar_tol_past_area_spec(),
    ]
}
