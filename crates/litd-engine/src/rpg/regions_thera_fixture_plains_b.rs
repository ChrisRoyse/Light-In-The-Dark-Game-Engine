// #1651 world/region: plains, hills & fields, part B (Drogran Hills, Northern
// Foothills, Fields of Balatorn). Split from
// regions_thera_fixture_plains.rs (part A: Udgaardian Plains, Nanthor, Open
// Plains) to respect the <500-line ratchet (#1466/#800 sec.6) after rustfmt
// expansion.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::regions_thera_fixture_plains::OPEN_PLAINS_AREA_ID;
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const DROGRAN_HILLS_AREA_ID: &str = "area.thera.drogran_hills";
pub(crate) const NORTHERN_FOOTHILLS_AREA_ID: &str = "area.thera.northern_foothills";
pub(crate) const FIELDS_OF_BALATORN_AREA_ID: &str = "area.thera.fields_of_balatorn";

pub(crate) fn plains_b_area_ids() -> [&'static str; 3] {
    [
        DROGRAN_HILLS_AREA_ID,
        NORTHERN_FOOTHILLS_AREA_ID,
        FIELDS_OF_BALATORN_AREA_ID,
    ]
}

pub(crate) fn drogran_hills_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: DROGRAN_HILLS_AREA_ID,
        display_name: "Drogran Hills",
        area_kind: AreaKind::Wilderness,
        source_issue: 1651,
        canon_issues: vec![987, 1598, 1632, 1651],
        wiki_slug: "drogran-hills",
        source_history_label: "CF Drogran Hills factual topology and mechanics only",
        summary: "CF ranking guide places ugruks at 25-35 for good-aligned characters.",
        level_band: (25, 35),
        rooms: vec![
            room(
                "room.drogran_hills.rise",
                "Drogran Rise",
                RoomKind::Wilderness,
                "Rolling hills patrolled by ugruks.",
                RoomConcealment::None,
                &["npc.drogran_hills.ugruk"],
            ),
            room(
                "room.drogran_hills.ugruk_camp",
                "Ugruk Camp",
                RoomKind::Landmark,
                "A fortified ugruk hilltop camp.",
                RoomConcealment::None,
                &["npc.drogran_hills.ugruk_chief"],
            ),
        ],
        connections: vec![
            conn(
                "conn.drogran_hills.rise_to_camp",
                "room.drogran_hills.rise",
                DROGRAN_HILLS_AREA_ID,
                Some("room.drogran_hills.ugruk_camp"),
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.drogran_hills.to_open_plains",
                "room.drogran_hills.rise",
                OPEN_PLAINS_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.drogran_hills.to_northern_foothills",
                "room.drogran_hills.ugruk_camp",
                NORTHERN_FOOTHILLS_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.drogran_hills.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.drogran_hills.population",
        repop_budget: 22,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.drogran_hills",
        model_id: "model.wilderness.drogran_hills",
        props: vec![
            "prop.hills.ugruk_palisade",
            "prop.hills.rocky_outcrop",
            "prop.hills.worn_path",
        ],
        silhouette: "rolling hill country crowned by a fortified ugruk camp",
        material_notes: "Original Thera-Drogran hill-camp dressing; no copied iconography.",
        notes: vec![
            "Sightlines remain long across the hill crests; tall-grass patches are the only \
            legal concealment (helpfile 467).",
        ],
    })
}

pub(crate) fn northern_foothills_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: NORTHERN_FOOTHILLS_AREA_ID,
        display_name: "Northern Foothills",
        area_kind: AreaKind::Region,
        source_issue: 1651,
        canon_issues: vec![987, 1598, 1632, 1651],
        wiki_slug: "northern-foothills",
        source_history_label: "CF Northern Foothills factual topology and mechanics only",
        summary: "1-51 traverse country into the mountains, the plains-to-peaks transition zone.",
        level_band: (1, 51),
        rooms: vec![
            room(
                "room.northern_foothills.rise",
                "Foothill Rise",
                RoomKind::Wilderness,
                "Grass gives way to stone as the land climbs.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.northern_foothills.mountain_gate",
                "Mountain Gate Trail",
                RoomKind::Landmark,
                "The trail where the foothills meet the true mountains.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.northern_foothills.rise_to_gate",
                "room.northern_foothills.rise",
                NORTHERN_FOOTHILLS_AREA_ID,
                Some("room.northern_foothills.mountain_gate"),
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.northern_foothills.to_drogran_hills",
                "room.northern_foothills.rise",
                DROGRAN_HILLS_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.northern_foothills.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.northern_foothills.population",
        repop_budget: 20,
        repop_minutes: 15,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.northern_foothills",
        model_id: "model.wilderness.northern_foothills",
        props: vec![
            "prop.hills.scree_slope",
            "prop.hills.rocky_outcrop",
            "prop.hills.worn_path",
        ],
        silhouette: "transitional foothill country climbing from grass into stone",
        material_notes: "Original Thera-Northern Foothills dressing; no copied iconography.",
        notes: vec![
            "Traverse country per source; connects the plains cluster to #1652's mountain \
            cluster (not authored as a cross-issue edge here to avoid a forward reference).",
        ],
    })
}

pub(crate) fn fields_of_balatorn_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: FIELDS_OF_BALATORN_AREA_ID,
        display_name: "Fields of Balatorn",
        area_kind: AreaKind::Wilderness,
        source_issue: 1651,
        canon_issues: vec![987, 1598, 1632, 1651],
        wiki_slug: "fields-of-balatorn",
        source_history_label: "CF Fields of Balatorn factual topology and mechanics only",
        summary: "Farmland east of the spine near Balator, with the Bramblefield patch near Voralian; \
            source gives no explicit CF level range, so the band below is estimated, not a cited number.",
        level_band: (10, 30),
        rooms: vec![
            room(
                "room.fields_of_balatorn.grain_field",
                "Grain Field",
                RoomKind::Wilderness,
                "Open farmland stretching toward Balator.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.fields_of_balatorn.bramblefield",
                "Bramblefield",
                RoomKind::Landmark,
                "A tangled bramble patch near Voralian.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.fields_of_balatorn.field_to_bramble",
                "room.fields_of_balatorn.grain_field",
                FIELDS_OF_BALATORN_AREA_ID,
                Some("room.fields_of_balatorn.bramblefield"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.fields_of_balatorn.to_open_plains",
                "room.fields_of_balatorn.grain_field",
                OPEN_PLAINS_AREA_ID,
                None,
                ConnectionDirection::Northwest,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.fields_of_balatorn.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.fields_of_balatorn.population",
        repop_budget: 18,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.fields_of_balatorn",
        model_id: "model.wilderness.fields_of_balatorn",
        props: vec![
            "prop.plains.grain_stook",
            "prop.plains.bramble_patch",
            "prop.plains.worn_path",
        ],
        silhouette: "eastern farmland fringed by a tangled bramblefield",
        material_notes: "Original Thera-Balatorn field dressing; no copied iconography.",
        notes: vec![
            "Field terrain is camouflage-legal only via tall-grass/bramble patches per \
            helpfile 467.",
        ],
    })
}

pub(crate) fn all_plains_b_specs() -> Vec<AreaSpec> {
    vec![
        drogran_hills_area_spec(),
        northern_foothills_area_spec(),
        fields_of_balatorn_area_spec(),
    ]
}
