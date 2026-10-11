// #1655 world/region: seas, islands & underwater, part B (Coral Head, Coral
// Palace, Kuo-Toa Lair, Crystal Island). Split from
// regions_thera_fixture_seas.rs (part A: Sea of Despair, Waters of the Deep,
// Aryth Ocean, Dragon Sea) to respect the <500-line ratchet (#1466/#800
// sec.6) after rustfmt expansion.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::regions_thera_fixture_seas::{
    ARYTH_OCEAN_AREA_ID, DRAGON_SEA_AREA_ID, WATERS_OF_THE_DEEP_AREA_ID,
};
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const CORAL_HEAD_AREA_ID: &str = "area.thera.coral_head";
pub(crate) const CORAL_PALACE_AREA_ID: &str = "area.thera.coral_palace";
pub(crate) const KUO_TOA_LAIR_AREA_ID: &str = "area.thera.kuo_toa_lair";
pub(crate) const CRYSTAL_ISLAND_AREA_ID: &str = "area.thera.crystal_island";

pub(crate) fn seas_b_area_ids() -> [&'static str; 4] {
    [
        CORAL_HEAD_AREA_ID,
        CORAL_PALACE_AREA_ID,
        KUO_TOA_LAIR_AREA_ID,
        CRYSTAL_ISLAND_AREA_ID,
    ]
}

pub(crate) fn coral_head_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: CORAL_HEAD_AREA_ID,
        display_name: "Coral Head",
        area_kind: AreaKind::Wilderness,
        source_issue: 1655,
        canon_issues: vec![987, 1598, 1632, 1655],
        wiki_slug: "coral-head",
        source_history_label: "CF Coral Head factual topology and mechanics only",
        summary: "The approach to the undersea kingdom, ranked 30-51.",
        level_band: (30, 51),
        rooms: vec![
            room(
                "room.coral_head.reef_approach",
                "Reef Approach",
                RoomKind::Wilderness,
                "A living coral reef guarding the approach to the palace.",
                RoomConcealment::None,
                &["npc.coral_head.reef_guardian"],
            ),
            room(
                "room.coral_head.grand_gate",
                "Coral Head Grand Gate",
                RoomKind::Gate,
                "The carved coral gate into the undersea kingdom.",
                RoomConcealment::GateChokepoint,
                &["npc.coral_head.coral_sentry"],
            ),
        ],
        connections: vec![
            conn(
                "conn.coral_head.approach_to_gate",
                "room.coral_head.reef_approach",
                CORAL_HEAD_AREA_ID,
                Some("room.coral_head.grand_gate"),
                ConnectionDirection::In,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.coral_head.to_dragon_sea",
                "room.coral_head.reef_approach",
                DRAGON_SEA_AREA_ID,
                None,
                ConnectionDirection::Up,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.coral_head.to_coral_palace",
                "room.coral_head.grand_gate",
                CORAL_PALACE_AREA_ID,
                None,
                ConnectionDirection::In,
                TravelKind::Gate,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.coral_head.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.coral_head.population",
        repop_budget: 20,
        repop_minutes: 18,
        pressure_tier: 3,
        tileset_id: "tileset.wilderness.coral_head",
        model_id: "model.wilderness.coral_head",
        props: vec![
            "prop.water.living_coral_reef",
            "prop.water.carved_coral_gate",
            "prop.water.bioluminescent_fish",
        ],
        silhouette: "living coral reef guarding a carved coral gate",
        material_notes: "Original Thera-Coral Head dressing; no copied iconography.",
        notes: vec!["Underwater full-3D stealth applies (mariner water-creep #1610)."],
    })
}

pub(crate) fn coral_palace_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: CORAL_PALACE_AREA_ID,
        display_name: "Coral Palace",
        area_kind: AreaKind::City,
        source_issue: 1655,
        canon_issues: vec![987, 1598, 1632, 1655],
        wiki_slug: "coral-palace",
        source_history_label: "CF Coral Palace factual topology and mechanics only",
        summary: "The undersea kingdom's palace, ranked 35-51.",
        level_band: (35, 51),
        rooms: vec![
            room(
                "room.coral_palace.throne_hall",
                "Coral Throne Hall",
                RoomKind::Landmark,
                "The seat of the undersea kingdom.",
                RoomConcealment::None,
                &["npc.coral_palace.coral_monarch"],
            ),
            room(
                "room.coral_palace.pearl_vault",
                "Pearl Vault",
                RoomKind::Chamber,
                "A vault of pearls and sunken treasure.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.coral_palace.hall_to_vault",
                "room.coral_palace.throne_hall",
                CORAL_PALACE_AREA_ID,
                Some("room.coral_palace.pearl_vault"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.coral_palace.to_coral_head",
                "room.coral_palace.throne_hall",
                CORAL_HEAD_AREA_ID,
                None,
                ConnectionDirection::Out,
                TravelKind::Gate,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.coral_palace.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.coral_palace.population",
        repop_budget: 22,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.city.coral_palace",
        model_id: "model.city.coral_palace",
        props: vec![
            "prop.water.coral_throne",
            "prop.water.pearl_hoard",
            "prop.water.carved_coral_gate",
        ],
        silhouette: "grand coral throne hall over a hidden pearl vault",
        material_notes: "Original Thera-Coral Palace dressing; no copied iconography.",
        notes: vec!["Pearl Vault is a Hidden connection (#944)."],
    })
}

pub(crate) fn kuo_toa_lair_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: KUO_TOA_LAIR_AREA_ID,
        display_name: "Kuo-Toa Lair",
        area_kind: AreaKind::Lair,
        source_issue: 1655,
        canon_issues: vec![987, 1598, 1632, 1655],
        wiki_slug: "kuo-toa-lair",
        source_history_label: "CF Kuo-Toa Lair factual topology and mechanics only",
        summary: "A deep-trench horror, ranked 40-51.",
        level_band: (40, 51),
        rooms: vec![
            room(
                "room.kuo_toa_lair.trench_mouth",
                "Trench Mouth",
                RoomKind::Wilderness,
                "The mouth of a deep undersea trench.",
                RoomConcealment::None,
                &["npc.kuo_toa_lair.kuo_toa"],
            ),
            room(
                "room.kuo_toa_lair.priest_grotto",
                "Kuo-Toa Priest Grotto",
                RoomKind::Chamber,
                "A bioluminescent grotto where kuo-toa priests hold rites.",
                RoomConcealment::None,
                &["npc.kuo_toa_lair.kuo_toa_priest"],
            ),
        ],
        connections: vec![
            conn(
                "conn.kuo_toa_lair.mouth_to_grotto",
                "room.kuo_toa_lair.trench_mouth",
                KUO_TOA_LAIR_AREA_ID,
                Some("room.kuo_toa_lair.priest_grotto"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.kuo_toa_lair.to_waters_of_the_deep",
                "room.kuo_toa_lair.trench_mouth",
                WATERS_OF_THE_DEEP_AREA_ID,
                None,
                ConnectionDirection::Up,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.kuo_toa_lair.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.kuo_toa_lair.population",
        repop_budget: 18,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.lair.kuo_toa_lair",
        model_id: "model.lair.kuo_toa_lair",
        props: vec![
            "prop.water.deep_trench_wall",
            "prop.water.bioluminescent_fish",
            "prop.water.kuo_toa_totem",
        ],
        silhouette: "deep trench mouth opening onto a bioluminescent priest grotto",
        material_notes: "Original Thera-Kuo-Toa dressing; no copied iconography.",
        notes: vec![
            "Deep-trench horror per source; gill growth (L14) or buoyancy (transmuter L42) \
            required.",
        ],
    })
}

pub(crate) fn crystal_island_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: CRYSTAL_ISLAND_AREA_ID,
        display_name: "Crystal Island",
        area_kind: AreaKind::Wilderness,
        source_issue: 1655,
        canon_issues: vec![987, 1598, 1632, 1655],
        wiki_slug: "crystal-island",
        source_history_label: "CF Crystal Island factual topology and mechanics only",
        summary: "CF ranking guide places sahuagin/kuo-toa/guards here at 30-35 good ranking.",
        level_band: (30, 35),
        rooms: vec![
            room(
                "room.crystal_island.crystal_beach",
                "Crystal Beach",
                RoomKind::Wilderness,
                "A beach glittering with embedded crystal formations.",
                RoomConcealment::None,
                &["npc.crystal_island.island_guard"],
            ),
            room(
                "room.crystal_island.crystal_grove",
                "Crystal Grove",
                RoomKind::Landmark,
                "A grove of crystalline growths at the island's heart.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.crystal_island.beach_to_grove",
                "room.crystal_island.crystal_beach",
                CRYSTAL_ISLAND_AREA_ID,
                Some("room.crystal_island.crystal_grove"),
                ConnectionDirection::In,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.crystal_island.to_aryth_ocean",
                "room.crystal_island.crystal_beach",
                ARYTH_OCEAN_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.crystal_island.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.crystal_island.population",
        repop_budget: 18,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.crystal_island",
        model_id: "model.wilderness.crystal_island",
        props: vec![
            "prop.water.crystal_formation",
            "prop.water.island_beach_sand",
            "prop.water.gull_flock",
        ],
        silhouette: "crystal-studded island beach opening onto a crystalline grove",
        material_notes: "Original Thera-Crystal Island dressing; no copied iconography.",
        notes: vec![
            "Part of the western isles web alongside Island Trails, the Garden, and Isle of \
            Corte (#1646, out of this batch's scope).",
        ],
    })
}

pub(crate) fn all_seas_b_specs() -> Vec<AreaSpec> {
    vec![
        coral_head_area_spec(),
        coral_palace_area_spec(),
        kuo_toa_lair_area_spec(),
        crystal_island_area_spec(),
    ]
}
