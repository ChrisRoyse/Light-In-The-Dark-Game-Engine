// #1652 world/region: mountains & cold north, part B (Jade Mountains, Loch
// Terradian, Mount Kiadana-Rah). Split from
// regions_thera_fixture_mountains.rs (part A: Northern Mountains, Mount
// Calandaryl, The Pass, Frigid Wasteland) to respect the <500-line ratchet
// (#1466/#800 sec.6) after rustfmt expansion.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::regions_thera_fixture_mountains::FRIGID_WASTELAND_AREA_ID;
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const JADE_MOUNTAINS_AREA_ID: &str = "area.thera.jade_mountains";
pub(crate) const LOCH_TERRADIAN_AREA_ID: &str = "area.thera.loch_terradian";
pub(crate) const MOUNT_KIADANA_RAH_AREA_ID: &str = "area.thera.mount_kiadana_rah";

pub(crate) fn mountains_b_area_ids() -> [&'static str; 3] {
    [
        JADE_MOUNTAINS_AREA_ID,
        LOCH_TERRADIAN_AREA_ID,
        MOUNT_KIADANA_RAH_AREA_ID,
    ]
}

pub(crate) fn jade_mountains_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: JADE_MOUNTAINS_AREA_ID,
        display_name: "Jade Mountains",
        area_kind: AreaKind::Wilderness,
        source_issue: 1652,
        canon_issues: vec![987, 1598, 1632, 1652],
        wiki_slug: "jade-mountains",
        source_history_label: "CF Jade Mountains factual topology and mechanics only",
        summary: "CF ranking guide places jade elves at 25-30 evil ranking.",
        level_band: (15, 35),
        rooms: vec![
            room(
                "room.jade_mountains.green_slope",
                "Jade-Veined Slope",
                RoomKind::Wilderness,
                "A slope streaked with jade mineral veins.",
                RoomConcealment::None,
                &["npc.jade_mountains.jade_elf"],
            ),
            room(
                "room.jade_mountains.elf_enclave",
                "Jade Elf Enclave",
                RoomKind::Landmark,
                "A hidden enclave carved into the jade-veined stone.",
                RoomConcealment::HiddenLayer,
                &["npc.jade_mountains.jade_elf_warden"],
            ),
        ],
        connections: vec![
            conn(
                "conn.jade_mountains.slope_to_enclave",
                "room.jade_mountains.green_slope",
                JADE_MOUNTAINS_AREA_ID,
                Some("room.jade_mountains.elf_enclave"),
                ConnectionDirection::Up,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.jade_mountains.to_frigid_wasteland",
                "room.jade_mountains.green_slope",
                FRIGID_WASTELAND_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.jade_mountains.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.jade_mountains.population",
        repop_budget: 20,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.jade_mountains",
        model_id: "model.wilderness.jade_mountains",
        props: vec![
            "prop.mountain.jade_vein",
            "prop.mountain.hidden_enclave_gate",
            "prop.mountain.rocky_outcrop",
        ],
        silhouette: "jade-veined mountain slope hiding a carved elf enclave",
        material_notes: "Original Thera-Jade Mountains dressing; no copied iconography.",
        notes: vec!["Jade Elf Enclave is a Hidden connection (#944)."],
    })
}

pub(crate) fn loch_terradian_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: LOCH_TERRADIAN_AREA_ID,
        display_name: "Loch Terradian",
        area_kind: AreaKind::Region,
        source_issue: 1652,
        canon_issues: vec![987, 1598, 1632, 1652, 1656],
        wiki_slug: "loch-terradian",
        source_history_label: "CF Loch Terradian factual topology and mechanics only",
        summary: "The giant-war loch; CF ranking guide places war-weary giants here on the evil side. \
            Its open-water body is claimed by this issue per #1656's own cross-reference.",
        level_band: (40, 45),
        rooms: vec![
            room(
                "room.loch_terradian.shore",
                "War-Scarred Shore",
                RoomKind::Wilderness,
                "A shoreline still scarred from a war between giants.",
                RoomConcealment::None,
                &["npc.loch_terradian.war_weary_giant"],
            ),
            room(
                "room.loch_terradian.open_water",
                "Loch Terradian Open Water",
                RoomKind::Wilderness,
                "The loch's cold open water.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.loch_terradian.shore_to_water",
                "room.loch_terradian.shore",
                LOCH_TERRADIAN_AREA_ID,
                Some("room.loch_terradian.open_water"),
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.loch_terradian.to_frigid_wasteland",
                "room.loch_terradian.shore",
                FRIGID_WASTELAND_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.loch_terradian.to_mount_kiadana_rah",
                "room.loch_terradian.open_water",
                MOUNT_KIADANA_RAH_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.loch_terradian.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.loch_terradian.population",
        repop_budget: 18,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.loch_terradian",
        model_id: "model.wilderness.loch_terradian",
        props: vec![
            "prop.water.cold_loch_surface",
            "prop.mountain.war_scarred_shore",
            "prop.mountain.rocky_outcrop",
        ],
        silhouette: "cold mountain loch scarred by a past giant war",
        material_notes: "Original Thera-Loch Terradian dressing; no copied iconography.",
        notes: vec![
            "The water crossing to Mount Kiadana-Rah is a Hidden connection (#944): it is not \
            plainly visible where the safe crossing lies.",
        ],
    })
}

pub(crate) fn mount_kiadana_rah_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: MOUNT_KIADANA_RAH_AREA_ID,
        display_name: "Mount Kiadana-Rah",
        area_kind: AreaKind::Wilderness,
        source_issue: 1652,
        canon_issues: vec![987, 1598, 1632, 1652, 1014],
        wiki_slug: "mount-kiadana-rah",
        source_history_label: "CF Mount Kiadana-Rah factual topology and mechanics only",
        summary: "Storm giant island-mountain across the water; CF ranking guide places storm giants \
            at 40-51 evil ranking.",
        level_band: (25, 51),
        rooms: vec![
            room(
                "room.mount_kiadana_rah.island_shore",
                "Island Shore",
                RoomKind::Wilderness,
                "The rocky shore of the storm giant's island-mountain.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.mount_kiadana_rah.storm_hold",
                "Storm Giant Hold",
                RoomKind::Landmark,
                "A cloud-wreathed hold at the mountain's peak.",
                RoomConcealment::None,
                &["npc.mount_kiadana_rah.storm_giant"],
            ),
        ],
        connections: vec![
            conn(
                "conn.mount_kiadana_rah.shore_to_hold",
                "room.mount_kiadana_rah.island_shore",
                MOUNT_KIADANA_RAH_AREA_ID,
                Some("room.mount_kiadana_rah.storm_hold"),
                ConnectionDirection::Up,
                TravelKind::Stair,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.mount_kiadana_rah.to_loch_terradian",
                "room.mount_kiadana_rah.island_shore",
                LOCH_TERRADIAN_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.mount_kiadana_rah.cycle",
        reset_minutes: 45,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.mount_kiadana_rah.population",
        repop_budget: 16,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.wilderness.mount_kiadana_rah",
        model_id: "model.wilderness.mount_kiadana_rah",
        props: vec![
            "prop.mountain.cloud_wreathed_peak",
            "prop.mountain.storm_giant_hold",
            "prop.water.rocky_island_shore",
        ],
        silhouette: "storm-wreathed island-mountain rising from cold open water",
        material_notes: "Original Thera-Kiadana-Rah dressing; no copied iconography.",
        notes: vec![
            "Storm weather at altitude; the water crossing from Loch Terradian is Hidden (#944).",
        ],
    })
}

pub(crate) fn all_mountains_b_specs() -> Vec<AreaSpec> {
    vec![
        jade_mountains_area_spec(),
        loch_terradian_area_spec(),
        mount_kiadana_rah_area_spec(),
    ]
}
