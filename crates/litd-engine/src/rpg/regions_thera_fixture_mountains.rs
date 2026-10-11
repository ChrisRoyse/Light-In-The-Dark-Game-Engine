// #1652 world/region: mountains & cold north, part A (Northern Mountains,
// Mount Calandaryl, The Pass, Frigid Wasteland). Split from a single file to
// respect the <500-line ratchet (#1466/#800 sec.6) after rustfmt expansion;
// part B (Jade Mountains, Loch Terradian, Mount Kiadana-Rah) lives in
// regions_thera_fixture_mountains_b.rs. Mountaineer country (#1612) + giant
// homelands (#1012-#1015). Design constants cited: altitude/cold survival
// (#953 temperature), sure-footing inclines, rockslide warcries, snow
// footprints EXTRA visible for track (#1620 - cold counters stealth).

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::regions_thera_fixture_mountains_b::{JADE_MOUNTAINS_AREA_ID, LOCH_TERRADIAN_AREA_ID};
use super::regions_thera_fixture_plains_b::NORTHERN_FOOTHILLS_AREA_ID;
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const NORTHERN_MOUNTAINS_AREA_ID: &str = "area.thera.northern_mountains";
pub(crate) const MOUNT_CALANDARYL_AREA_ID: &str = "area.thera.mount_calandaryl";
pub(crate) const THE_PASS_AREA_ID: &str = "area.thera.the_pass";
pub(crate) const FRIGID_WASTELAND_AREA_ID: &str = "area.thera.frigid_wasteland";

pub(crate) fn mountains_a_area_ids() -> [&'static str; 4] {
    [
        NORTHERN_MOUNTAINS_AREA_ID,
        MOUNT_CALANDARYL_AREA_ID,
        THE_PASS_AREA_ID,
        FRIGID_WASTELAND_AREA_ID,
    ]
}

pub(crate) fn northern_mountains_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: NORTHERN_MOUNTAINS_AREA_ID,
        display_name: "Northern Mountains",
        area_kind: AreaKind::Wilderness,
        source_issue: 1652,
        canon_issues: vec![987, 1598, 1632, 1652, 1612],
        wiki_slug: "northern-mountains",
        source_history_label: "CF Northern Mountains factual topology and mechanics only",
        summary: "Patrolled by Udgaard patrollers, far northwest beyond the foothills.",
        level_band: (35, 45),
        rooms: vec![
            room(
                "room.northern_mountains.switchback",
                "Mountain Switchback",
                RoomKind::Wilderness,
                "A narrow climbing trail requiring sure footing on the inclines.",
                RoomConcealment::None,
                &["npc.northern_mountains.udgaard_patroller"],
            ),
            room(
                "room.northern_mountains.ridge_camp",
                "Ridge Camp",
                RoomKind::Landmark,
                "A windswept camp along the ridge line.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.northern_mountains.switchback_to_ridge",
                "room.northern_mountains.switchback",
                NORTHERN_MOUNTAINS_AREA_ID,
                Some("room.northern_mountains.ridge_camp"),
                ConnectionDirection::Up,
                TravelKind::Stair,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.northern_mountains.to_northern_foothills",
                "room.northern_mountains.switchback",
                NORTHERN_FOOTHILLS_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.northern_mountains.to_mount_calandaryl",
                "room.northern_mountains.ridge_camp",
                MOUNT_CALANDARYL_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Stair,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.northern_mountains.to_the_pass",
                "room.northern_mountains.ridge_camp",
                THE_PASS_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.northern_mountains.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.northern_mountains.population",
        repop_budget: 24,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.northern_mountains",
        model_id: "model.wilderness.northern_mountains",
        props: vec![
            "prop.mountain.snow_ridge",
            "prop.mountain.switchback_trail",
            "prop.mountain.wind_marker",
        ],
        silhouette: "steep switchback trail climbing a windswept northern ridge",
        material_notes: "Original Thera-Northern Mountains dressing; no copied iconography.",
        notes: vec![
            "Sure-footing inclines and rockslide warcries; snow footprints are extra visible \
            for tracking, countering stealth (#1620).",
        ],
    })
}

pub(crate) fn mount_calandaryl_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: MOUNT_CALANDARYL_AREA_ID,
        display_name: "Mount Calandaryl",
        area_kind: AreaKind::Wilderness,
        source_issue: 1652,
        canon_issues: vec![987, 1598, 1632, 1652],
        wiki_slug: "mount-calandaryl",
        source_history_label: "CF Mount Calandaryl factual topology and mechanics only",
        summary: "An endgame climb far northwest, guarded by skeletons.",
        level_band: (40, 51),
        rooms: vec![
            room(
                "room.mount_calandaryl.frozen_ascent",
                "Frozen Ascent",
                RoomKind::Wilderness,
                "An ice-slick climb toward the summit.",
                RoomConcealment::None,
                &["npc.mount_calandaryl.skeleton"],
            ),
            room(
                "room.mount_calandaryl.summit",
                "Calandaryl Summit",
                RoomKind::Landmark,
                "The endgame summit chamber.",
                RoomConcealment::None,
                &["npc.mount_calandaryl.summit_guardian"],
            ),
        ],
        connections: vec![
            conn(
                "conn.mount_calandaryl.ascent_to_summit",
                "room.mount_calandaryl.frozen_ascent",
                MOUNT_CALANDARYL_AREA_ID,
                Some("room.mount_calandaryl.summit"),
                ConnectionDirection::Up,
                TravelKind::Stair,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.mount_calandaryl.to_northern_mountains",
                "room.mount_calandaryl.frozen_ascent",
                NORTHERN_MOUNTAINS_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Stair,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.mount_calandaryl.cycle",
        reset_minutes: 45,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.mount_calandaryl.population",
        repop_budget: 16,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.wilderness.mount_calandaryl",
        model_id: "model.wilderness.mount_calandaryl",
        props: vec![
            "prop.mountain.ice_shelf",
            "prop.mountain.summit_marker",
            "prop.mountain.wind_marker",
        ],
        silhouette: "endgame ice-bound summit far in the northwest",
        material_notes: "Original Thera-Calandaryl ice and stone dressing; no copied iconography.",
        notes: vec!["Altitude/cold survival applies (#953 temperature)."],
    })
}

pub(crate) fn the_pass_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: THE_PASS_AREA_ID,
        display_name: "The Pass",
        area_kind: AreaKind::Road,
        source_issue: 1652,
        canon_issues: vec![987, 1598, 1632, 1652, 955],
        wiki_slug: "the-pass",
        source_history_label: "CF The Pass (High Pass) factual topology and mechanics only",
        summary: "A mountain road crossing and chokepoint PK terrain hotspot (#955).",
        level_band: (20, 40),
        rooms: vec![
            room(
                "room.the_pass.low_switchback",
                "Low Switchback",
                RoomKind::Wilderness,
                "The lower approach to the High Pass crossing.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.the_pass.high_pass",
                "High Pass",
                RoomKind::Landmark,
                "The narrow chokepoint crossing at the top of the pass (#955 PK hotspot).",
                RoomConcealment::GateChokepoint,
                &["npc.the_pass.mountain_bandit"],
            ),
        ],
        connections: vec![
            conn(
                "conn.the_pass.low_to_high",
                "room.the_pass.low_switchback",
                THE_PASS_AREA_ID,
                Some("room.the_pass.high_pass"),
                ConnectionDirection::Up,
                TravelKind::Stair,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.the_pass.to_northern_mountains",
                "room.the_pass.low_switchback",
                NORTHERN_MOUNTAINS_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.the_pass.to_frigid_wasteland",
                "room.the_pass.high_pass",
                FRIGID_WASTELAND_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.the_pass.cycle",
        reset_minutes: 25,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.the_pass.population",
        repop_budget: 14,
        repop_minutes: 12,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.the_pass",
        model_id: "model.wilderness.the_pass",
        props: vec![
            "prop.mountain.chokepoint_rockface",
            "prop.mountain.switchback_trail",
            "prop.mountain.cairn_marker",
        ],
        silhouette: "narrow mountain-road chokepoint linking the northern range to the frigid west",
        material_notes: "Original Thera-Pass road and rockface dressing; no copied iconography.",
        notes: vec!["Chokepoint PK terrain hotspot per #955; sure-footing inclines apply."],
    })
}

pub(crate) fn frigid_wasteland_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: FRIGID_WASTELAND_AREA_ID,
        display_name: "Frigid Wasteland",
        area_kind: AreaKind::Wilderness,
        source_issue: 1652,
        canon_issues: vec![987, 1598, 1632, 1652, 1012],
        wiki_slug: "frigid-wasteland",
        source_history_label: "CF Frigid Wasteland factual topology and mechanics only",
        summary: "Frost giant homeland; CF ranking guide places frost giants at 30-35 good ranking.",
        level_band: (25, 40),
        rooms: vec![
            room(
                "room.frigid_wasteland.ice_flat",
                "Ice Flat",
                RoomKind::Wilderness,
                "A wind-scoured ice flat.",
                RoomConcealment::None,
                &["npc.frigid_wasteland.frost_giant"],
            ),
            room(
                "room.frigid_wasteland.giant_hold",
                "Frost Giant Hold",
                RoomKind::Landmark,
                "A carved-ice hold, homeland of the frost giants.",
                RoomConcealment::None,
                &["npc.frigid_wasteland.frost_giant_jarl"],
            ),
        ],
        connections: vec![
            conn(
                "conn.frigid_wasteland.flat_to_hold",
                "room.frigid_wasteland.ice_flat",
                FRIGID_WASTELAND_AREA_ID,
                Some("room.frigid_wasteland.giant_hold"),
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.frigid_wasteland.to_the_pass",
                "room.frigid_wasteland.ice_flat",
                THE_PASS_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.frigid_wasteland.to_jade_mountains",
                "room.frigid_wasteland.giant_hold",
                JADE_MOUNTAINS_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.frigid_wasteland.to_loch_terradian",
                "room.frigid_wasteland.ice_flat",
                LOCH_TERRADIAN_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.frigid_wasteland.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.frigid_wasteland.population",
        repop_budget: 22,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.frigid_wasteland",
        model_id: "model.wilderness.frigid_wasteland",
        props: vec![
            "prop.mountain.ice_shelf",
            "prop.mountain.frost_giant_hold",
            "prop.mountain.wind_marker",
        ],
        silhouette: "wind-scoured ice wasteland dominated by a carved-ice frost giant hold",
        material_notes: "Original Thera-Frigid Wasteland dressing; no copied iconography.",
        notes: vec!["Altitude/cold survival (#953); boreal wind."],
    })
}

pub(crate) fn all_mountains_a_specs() -> Vec<AreaSpec> {
    vec![
        northern_mountains_area_spec(),
        mount_calandaryl_area_spec(),
        the_pass_area_spec(),
        frigid_wasteland_area_spec(),
    ]
}
