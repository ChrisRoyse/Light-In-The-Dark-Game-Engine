// #1653 world/region: desert & far south, part B (Tahril Mountains, Wastes of
// Nonviel, Xvart Lair). Split from regions_thera_fixture_desert.rs (part A:
// Sands of Sorrow, Desert of Araile, Pyramid of Azhan, Oryx Steppes) to
// respect the <500-line ratchet (#1466/#800 sec.6) after rustfmt expansion.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::regions_thera_fixture_desert::ORYX_STEPPES_AREA_ID;
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const TAHRIL_MOUNTAINS_AREA_ID: &str = "area.thera.tahril_mountains";
pub(crate) const WASTES_OF_NONVIEL_AREA_ID: &str = "area.thera.wastes_of_nonviel";
pub(crate) const XVART_LAIR_AREA_ID: &str = "area.thera.xvart_lair";

pub(crate) fn desert_b_area_ids() -> [&'static str; 3] {
    [
        TAHRIL_MOUNTAINS_AREA_ID,
        WASTES_OF_NONVIEL_AREA_ID,
        XVART_LAIR_AREA_ID,
    ]
}

pub(crate) fn tahril_mountains_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: TAHRIL_MOUNTAINS_AREA_ID,
        display_name: "Tahril Mountains",
        area_kind: AreaKind::Region,
        source_issue: 1653,
        canon_issues: vec![987, 1598, 1632, 1653],
        wiki_slug: "tahril-mountains",
        source_history_label: "CF Tahril Mountains factual topology and mechanics only",
        summary: "1-51 southern range road, the far-south mountain spine.",
        level_band: (1, 51),
        rooms: vec![
            room(
                "room.tahril_mountains.dry_riverbed",
                "A Dry Riverbed",
                RoomKind::Wilderness,
                "The 1-51 traverse dry riverbed at the range's foot.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.tahril_mountains.range_road",
                "Southern Range Road",
                RoomKind::Street,
                "The southern road climbing through the Tahril range.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.tahril_mountains.riverbed_to_road",
                "room.tahril_mountains.dry_riverbed",
                TAHRIL_MOUNTAINS_AREA_ID,
                Some("room.tahril_mountains.range_road"),
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.tahril_mountains.to_oryx_steppes",
                "room.tahril_mountains.dry_riverbed",
                ORYX_STEPPES_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.tahril_mountains.to_wastes_of_nonviel",
                "room.tahril_mountains.range_road",
                WASTES_OF_NONVIEL_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.tahril_mountains.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.tahril_mountains.population",
        repop_budget: 20,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.tahril_mountains",
        model_id: "model.wilderness.tahril_mountains",
        props: vec![
            "prop.desert.dry_riverbed",
            "prop.mountain.rocky_outcrop",
            "prop.desert.wind_worn_marker",
        ],
        silhouette: "dry riverbed rising into a rocky southern mountain road",
        material_notes: "Original Thera-Tahril dressing; no copied iconography.",
        notes: vec!["No water on the riverbed = find-water/stash gameplay (#953)."],
    })
}

pub(crate) fn wastes_of_nonviel_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: WASTES_OF_NONVIEL_AREA_ID,
        display_name: "Wastes of Nonviel",
        area_kind: AreaKind::Wilderness,
        source_issue: 1653,
        canon_issues: vec![987, 1598, 1632, 1653],
        wiki_slug: "wastes-of-nonviel",
        source_history_label: "CF Wastes of Nonviel factual topology and mechanics only",
        summary: "Far-south badlands.",
        level_band: (30, 51),
        rooms: vec![
            room(
                "room.wastes_of_nonviel.cracked_flat",
                "Cracked Badland Flat",
                RoomKind::Wilderness,
                "Sun-cracked badland stretching to the horizon.",
                RoomConcealment::None,
                &["npc.wastes_of_nonviel.badland_stalker"],
            ),
            room(
                "room.wastes_of_nonviel.bone_field",
                "Bone Field",
                RoomKind::Landmark,
                "A field of bleached bones from ages of badland death.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.wastes_of_nonviel.flat_to_bones",
                "room.wastes_of_nonviel.cracked_flat",
                WASTES_OF_NONVIEL_AREA_ID,
                Some("room.wastes_of_nonviel.bone_field"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.wastes_of_nonviel.to_tahril_mountains",
                "room.wastes_of_nonviel.cracked_flat",
                TAHRIL_MOUNTAINS_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.wastes_of_nonviel.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.wastes_of_nonviel.population",
        repop_budget: 18,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.wilderness.wastes_of_nonviel",
        model_id: "model.wilderness.wastes_of_nonviel",
        props: vec![
            "prop.desert.cracked_earth",
            "prop.desert.bleached_bone_field",
            "prop.desert.wind_worn_marker",
        ],
        silhouette: "sun-cracked far-south badlands strewn with bleached bone",
        material_notes: "Original Thera-Nonviel dressing; no copied iconography.",
        notes: vec!["Heat survival (endure heat) and no-water stash gameplay apply (#953)."],
    })
}

pub(crate) fn xvart_lair_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: XVART_LAIR_AREA_ID,
        display_name: "Xvart Lair",
        area_kind: AreaKind::Lair,
        source_issue: 1653,
        canon_issues: vec![987, 1598, 1632, 1653],
        wiki_slug: "xvart-lair",
        source_history_label: "CF Xvart Lair (and Caves) factual topology and mechanics only",
        summary: "A low-hills warren southwest on the map, marked tX-vart.",
        level_band: (5, 15),
        rooms: vec![
            room(
                "room.xvart_lair.warren_mouth",
                "Warren Mouth",
                RoomKind::Gate,
                "A low burrow mouth in the hills.",
                RoomConcealment::GateChokepoint,
                &["npc.xvart_lair.xvart"],
            ),
            room(
                "room.xvart_lair.xvart_caves",
                "Xvart Caves",
                RoomKind::Chamber,
                "The warren's inner caves.",
                RoomConcealment::None,
                &["npc.xvart_lair.xvart_chief"],
            ),
        ],
        connections: vec![
            conn(
                "conn.xvart_lair.mouth_to_caves",
                "room.xvart_lair.warren_mouth",
                XVART_LAIR_AREA_ID,
                Some("room.xvart_lair.xvart_caves"),
                ConnectionDirection::In,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.xvart_lair.to_oryx_steppes",
                "room.xvart_lair.warren_mouth",
                ORYX_STEPPES_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
        ],
        reset_id: "reset.xvart_lair.cycle",
        reset_minutes: 25,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.xvart_lair.population",
        repop_budget: 14,
        repop_minutes: 12,
        pressure_tier: 1,
        tileset_id: "tileset.lair.xvart_lair",
        model_id: "model.lair.xvart_lair",
        props: vec![
            "prop.desert.low_burrow_mouth",
            "prop.desert.warren_midden",
            "prop.desert.rocky_outcrop",
        ],
        silhouette: "low hill warren of xvart burrows and caves",
        material_notes: "Original Thera-Xvart warren dressing; no copied iconography.",
        notes: vec!["Approach from the steppes is a ConcealedTrail (#944)."],
    })
}

pub(crate) fn all_desert_b_specs() -> Vec<AreaSpec> {
    vec![
        tahril_mountains_area_spec(),
        wastes_of_nonviel_area_spec(),
        xvart_lair_area_spec(),
    ]
}
