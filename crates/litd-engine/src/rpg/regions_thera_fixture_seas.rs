// #1655 world/region: seas, islands & underwater, part A (Sea of Despair,
// Waters of the Deep, Aryth Ocean, Dragon Sea). Split from a single file to
// respect the <500-line ratchet (#1466/#800 sec.6) after rustfmt expansion;
// part B (Coral Head, Coral Palace, Kuo-Toa Lair, Crystal Island) lives in
// regions_thera_fixture_seas_b.rs. Mariner country (#1612). Design constants
// cited: swimming/boats/buoyancy (transmuter L42 buoyancy, gill growth L14
// underwater breathing), underwater as full 3D stealth space (mariner
// water-creep #1610, flotsam concealment), felar hydrophobia penalty (#1017),
// storm weather at sea (stormcalling), navigation skill for open-water
// wayfinding.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::regions_thera_fixture_seas_b::{
    CORAL_HEAD_AREA_ID, CRYSTAL_ISLAND_AREA_ID, KUO_TOA_LAIR_AREA_ID,
};
use super::regions_thera_fixture_wetlands::YSIGRATH_SWAMP_AREA_ID;
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const SEA_OF_DESPAIR_AREA_ID: &str = "area.thera.sea_of_despair";
pub(crate) const WATERS_OF_THE_DEEP_AREA_ID: &str = "area.thera.waters_of_the_deep";
pub(crate) const ARYTH_OCEAN_AREA_ID: &str = "area.thera.aryth_ocean";
pub(crate) const DRAGON_SEA_AREA_ID: &str = "area.thera.dragon_sea";

pub(crate) fn seas_a_area_ids() -> [&'static str; 4] {
    [
        SEA_OF_DESPAIR_AREA_ID,
        WATERS_OF_THE_DEEP_AREA_ID,
        ARYTH_OCEAN_AREA_ID,
        DRAGON_SEA_AREA_ID,
    ]
}

pub(crate) fn sea_of_despair_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: SEA_OF_DESPAIR_AREA_ID,
        display_name: "Sea of Despair",
        area_kind: AreaKind::Wilderness,
        source_issue: 1655,
        canon_issues: vec![987, 1598, 1632, 1655, 1612],
        wiki_slug: "sea-of-despair",
        source_history_label: "CF Sea of Despair factual topology and mechanics only",
        summary: "The western sea, with Ruins of the Deep beneath (sahuagin for goods, silver \
            spirits for evils).",
        level_band: (10, 51),
        rooms: vec![
            room(
                "room.sea_of_despair.open_water",
                "Sea of Despair Open Water",
                RoomKind::Wilderness,
                "Storm-prone open water west of the mainland.",
                RoomConcealment::None,
                &["npc.sea_of_despair.storm_sailor"],
            ),
            room(
                "room.sea_of_despair.wreck_shoal",
                "Wreck Shoal",
                RoomKind::Landmark,
                "A shoal littered with old wrecks.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.sea_of_despair.water_to_shoal",
                "room.sea_of_despair.open_water",
                SEA_OF_DESPAIR_AREA_ID,
                Some("room.sea_of_despair.wreck_shoal"),
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.sea_of_despair.to_ysigrath_swamp",
                "room.sea_of_despair.open_water",
                YSIGRATH_SWAMP_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.sea_of_despair.to_waters_of_the_deep",
                "room.sea_of_despair.wreck_shoal",
                WATERS_OF_THE_DEEP_AREA_ID,
                None,
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.sea_of_despair.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.sea_of_despair.population",
        repop_budget: 26,
        repop_minutes: 20,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.sea_of_despair",
        model_id: "model.wilderness.sea_of_despair",
        props: vec![
            "prop.water.storm_swell",
            "prop.water.wreck_hull",
            "prop.water.flotsam_field",
        ],
        silhouette: "storm-prone western sea littered with old wrecks",
        material_notes: "Original Thera-Sea of Despair dressing; no copied iconography.",
        notes: vec![
            "Storm weather at sea (stormcalling); felar hydrophobia penalty applies (#1017); \
            navigation skill needed for open-water wayfinding.",
        ],
    })
}

pub(crate) fn waters_of_the_deep_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: WATERS_OF_THE_DEEP_AREA_ID,
        display_name: "Waters of the Deep",
        area_kind: AreaKind::Wilderness,
        source_issue: 1655,
        canon_issues: vec![987, 1598, 1632, 1655],
        wiki_slug: "waters-of-the-deep",
        source_history_label: "CF Waters of the Deep (and Ruins of the Deep) factual topology and \
            mechanics only",
        summary: "The deep water beneath the Sea of Despair; its deepest chamber is the Ruins of the \
            Deep, where CF ranking places sahuagin for goods and silver spirits for evils at 40-51.",
        level_band: (15, 51),
        rooms: vec![
            room(
                "room.waters_of_the_deep.dark_current",
                "Dark Current",
                RoomKind::Wilderness,
                "A cold dark current far below the surface.",
                RoomConcealment::None,
                &["npc.waters_of_the_deep.sahuagin"],
            ),
            room(
                "room.waters_of_the_deep.ruins_of_the_deep",
                "Ruins of the Deep",
                RoomKind::Chamber,
                "Sunken ruins at 40-51, home to silver spirits and sahuagin.",
                RoomConcealment::None,
                &["npc.waters_of_the_deep.silver_spirit"],
            ),
        ],
        connections: vec![
            conn(
                "conn.waters_of_the_deep.current_to_ruins",
                "room.waters_of_the_deep.dark_current",
                WATERS_OF_THE_DEEP_AREA_ID,
                Some("room.waters_of_the_deep.ruins_of_the_deep"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.waters_of_the_deep.to_sea_of_despair",
                "room.waters_of_the_deep.dark_current",
                SEA_OF_DESPAIR_AREA_ID,
                None,
                ConnectionDirection::Up,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.waters_of_the_deep.to_kuo_toa_lair",
                "room.waters_of_the_deep.ruins_of_the_deep",
                KUO_TOA_LAIR_AREA_ID,
                None,
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.waters_of_the_deep.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.waters_of_the_deep.population",
        repop_budget: 20,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.wilderness.waters_of_the_deep",
        model_id: "model.wilderness.waters_of_the_deep",
        props: vec![
            "prop.water.deep_current",
            "prop.water.sunken_ruin_column",
            "prop.water.bioluminescent_fish",
        ],
        silhouette: "cold deep-water current threading down to sunken ruins",
        material_notes: "Original Thera-Deep-Waters dressing; no copied iconography.",
        notes: vec![
            "Underwater is a full 3D stealth space (mariner water-creep #1610, flotsam \
            concealment); gill growth (L14) or buoyancy (transmuter L42) needed to breathe/move here.",
        ],
    })
}

pub(crate) fn aryth_ocean_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: ARYTH_OCEAN_AREA_ID,
        display_name: "Aryth Ocean",
        area_kind: AreaKind::Wilderness,
        source_issue: 1655,
        canon_issues: vec![987, 1598, 1632, 1655],
        wiki_slug: "aryth-ocean",
        source_history_label: "CF Aryth Ocean factual topology and mechanics only",
        summary: "The south/east ocean.",
        level_band: (10, 35),
        rooms: vec![
            room(
                "room.aryth_ocean.open_water",
                "Aryth Open Water",
                RoomKind::Wilderness,
                "Broad open ocean southeast of the mainland.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.aryth_ocean.trade_lane",
                "Aryth Trade Lane",
                RoomKind::Street,
                "A charted lane used by trading vessels.",
                RoomConcealment::None,
                &["npc.aryth_ocean.merchant_sailor"],
            ),
        ],
        connections: vec![
            conn(
                "conn.aryth_ocean.water_to_lane",
                "room.aryth_ocean.open_water",
                ARYTH_OCEAN_AREA_ID,
                Some("room.aryth_ocean.trade_lane"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.aryth_ocean.to_dragon_sea",
                "room.aryth_ocean.trade_lane",
                DRAGON_SEA_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.aryth_ocean.to_crystal_island",
                "room.aryth_ocean.open_water",
                CRYSTAL_ISLAND_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.aryth_ocean.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.aryth_ocean.population",
        repop_budget: 22,
        repop_minutes: 20,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.aryth_ocean",
        model_id: "model.wilderness.aryth_ocean",
        props: vec![
            "prop.water.open_ocean_swell",
            "prop.water.trade_lane_buoy",
            "prop.water.gull_flock",
        ],
        silhouette: "broad open southeast ocean crossed by a charted trade lane",
        material_notes: "Original Thera-Aryth Ocean dressing; no copied iconography.",
        notes: vec!["Navigation skill needed for open-water wayfinding; storm weather at sea."],
    })
}

pub(crate) fn dragon_sea_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: DRAGON_SEA_AREA_ID,
        display_name: "Dragon Sea",
        area_kind: AreaKind::Wilderness,
        source_issue: 1655,
        canon_issues: vec![987, 1598, 1632, 1655],
        wiki_slug: "dragon-sea",
        source_history_label: "CF Dragon Sea factual topology and mechanics only",
        summary: "The eastern sea carrying the Port-Dragon-Arkham sea lane.",
        level_band: (10, 35),
        rooms: vec![
            room(
                "room.dragon_sea.sea_lane",
                "Port-Dragon-Arkham Sea Lane",
                RoomKind::Street,
                "The charted lane linking the Dragon port to Arkham.",
                RoomConcealment::None,
                &["npc.dragon_sea.merchant_sailor"],
            ),
            room(
                "room.dragon_sea.open_water",
                "Dragon Sea Open Water",
                RoomKind::Wilderness,
                "Open water east of the sea lane.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.dragon_sea.lane_to_water",
                "room.dragon_sea.sea_lane",
                DRAGON_SEA_AREA_ID,
                Some("room.dragon_sea.open_water"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.dragon_sea.to_aryth_ocean",
                "room.dragon_sea.sea_lane",
                ARYTH_OCEAN_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.dragon_sea.to_coral_head",
                "room.dragon_sea.open_water",
                CORAL_HEAD_AREA_ID,
                None,
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.dragon_sea.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.dragon_sea.population",
        repop_budget: 22,
        repop_minutes: 20,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.dragon_sea",
        model_id: "model.wilderness.dragon_sea",
        props: vec![
            "prop.water.sea_lane_marker",
            "prop.water.open_ocean_swell",
            "prop.water.gull_flock",
        ],
        silhouette: "eastern sea carrying the Port-Dragon-Arkham trade lane",
        material_notes: "Original Thera-Dragon Sea dressing; no copied iconography.",
        notes: vec!["The undersea descent to Coral Head is Hidden (#944)."],
    })
}

pub(crate) fn all_seas_a_specs() -> Vec<AreaSpec> {
    vec![
        sea_of_despair_area_spec(),
        waters_of_the_deep_area_spec(),
        aryth_ocean_area_spec(),
        dragon_sea_area_spec(),
    ]
}
