// #1649 world/region: the Underdark realm, part A (tunnels, Velkyn Oloth,
// Arin-gird, Gol Galath). Split from a single file to respect the <500-line
// ratchet (#1466/#800 sec.6) after rustfmt expansion; part B (Underdark Mines,
// Troglodyte Caverns, Svirfneblin Enclave) lives in
// regions_thera_fixture_underdark_b.rs. Mechanics captured factually from the
// issue body (itself CF-factual per #984); only flavor prose is authored
// fresh. Design constants cited throughout: permanent darkness / dark-
// infravision required (#1618), echo acoustics (#1628), ceiling ambush
// volumes (#1611), duergar/goblin/orc hide terrains (#1606), no day/night
// cycle underground.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::regions_thera_fixture_underdark_b::{
    SVIRFNEBLIN_ENCLAVE_AREA_ID, TROGLODYTE_CAVERNS_AREA_ID, UNDERDARK_MINES_AREA_ID,
};
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const UNDERDARK_TUNNELS_AREA_ID: &str = "area.thera.underdark.tunnels";
pub(crate) const VELKYN_OLOTH_AREA_ID: &str = "area.thera.underdark.velkyn_oloth";
pub(crate) const ARIN_GIRD_AREA_ID: &str = "area.thera.underdark.arin_gird";
pub(crate) const GOL_GALATH_AREA_ID: &str = "area.thera.underdark.gol_galath";
/// Surface link named on the #1632 map ("Cragstone -> The Underdark"). Not yet
/// authored by any batch; referenced here as a satellite target, same pattern
/// Galadon's fixture used for its own not-yet-authored satellites.
pub(crate) const CRAGSTONE_AREA_ID: &str = "area.thera.cragstone";

pub(crate) fn underdark_a_area_ids() -> [&'static str; 5] {
    [
        UNDERDARK_TUNNELS_AREA_ID,
        VELKYN_OLOTH_AREA_ID,
        ARIN_GIRD_AREA_ID,
        GOL_GALATH_AREA_ID,
        CRAGSTONE_AREA_ID,
    ]
}

pub(crate) fn underdark_tunnels_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: UNDERDARK_TUNNELS_AREA_ID,
        display_name: "The Underdark",
        area_kind: AreaKind::Region,
        source_issue: 1649,
        canon_issues: vec![987, 1598, 1632, 1649, 1618, 1628],
        wiki_slug: "the-underdark",
        source_history_label: "CF Underdark tunnel-web factual topology and mechanics only",
        summary: "The tunnel web entered from Cragstone: the underground world that looms larger \
            than all other landscapes combined, permanently dark and acoustically alive.",
        level_band: (35, 51),
        rooms: vec![
            room(
                "room.underdark_tunnels.cragstone_descent",
                "Cragstone Descent",
                RoomKind::Landmark,
                "The secret way down from Cragstone into the deep dark.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
            room(
                "room.underdark_tunnels.tunnel_web",
                "Tunnel Web",
                RoomKind::Wilderness,
                "A branching maze of passages; sound carries strangely along the stone.",
                RoomConcealment::None,
                &["npc.underdark.tunnel_stalker"],
            ),
            room(
                "room.underdark_tunnels.echoing_gallery",
                "Echoing Gallery",
                RoomKind::Chamber,
                "A vast cavern where every footstep and whisper returns distorted (#1628).",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.underdark_tunnels.descent_to_web",
                "room.underdark_tunnels.cragstone_descent",
                UNDERDARK_TUNNELS_AREA_ID,
                Some("room.underdark_tunnels.tunnel_web"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.underdark_tunnels.web_to_gallery",
                "room.underdark_tunnels.tunnel_web",
                UNDERDARK_TUNNELS_AREA_ID,
                Some("room.underdark_tunnels.echoing_gallery"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.underdark_tunnels.to_cragstone",
                "room.underdark_tunnels.cragstone_descent",
                CRAGSTONE_AREA_ID,
                None,
                ConnectionDirection::Up,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.underdark_tunnels.to_velkyn_oloth",
                "room.underdark_tunnels.echoing_gallery",
                VELKYN_OLOTH_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.underdark_tunnels.to_troglodyte_caverns",
                "room.underdark_tunnels.tunnel_web",
                TROGLODYTE_CAVERNS_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.underdark_tunnels.to_svirfneblin_enclave",
                "room.underdark_tunnels.tunnel_web",
                SVIRFNEBLIN_ENCLAVE_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
        ],
        reset_id: "reset.underdark_tunnels.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.underdark_tunnels.population",
        repop_budget: 30,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.wilderness.underdark_tunnels",
        model_id: "model.wilderness.underdark_tunnels",
        props: vec![
            "prop.underdark.stalactite_field",
            "prop.underdark.glow_fungus",
            "prop.underdark.rubble_pile",
            "prop.underdark.cragstone_stair",
        ],
        silhouette: "black basalt tunnel web with no sky, glow-fungus the only ambient light",
        material_notes: "Original Thera-Underdark stonework and fungal bioluminescence; no copied \
            iconography from the source MUD.",
        notes: vec![
            "Permanent darkness: dark vision/infravision required to see at all (#1618); no \
            day/night cycle applies underground.",
            "Echo acoustics (#1628) and ceiling ambush volumes \
            (#1611) are design requirements captured here, not yet lowered to a runtime system.",
        ],
    })
}

pub(crate) fn velkyn_oloth_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: VELKYN_OLOTH_AREA_ID,
        display_name: "Velkyn Oloth",
        area_kind: AreaKind::Lair,
        source_issue: 1649,
        canon_issues: vec![987, 1598, 1632, 1649],
        wiki_slug: "velkyn-oloth",
        source_history_label: "CF Velkyn Oloth factual topology and mechanics only",
        summary: "A dark-elf surface-adjacent hold guarding the approach to the deeper drow city; \
            CF ranking guide places its guards at 25-30 for good-aligned characters.",
        level_band: (25, 35),
        rooms: vec![
            room(
                "room.velkyn_oloth.outpost_gate",
                "Outpost Gate",
                RoomKind::Gate,
                "The drow-warded gate into Velkyn Oloth.",
                RoomConcealment::GateChokepoint,
                &["npc.velkyn_oloth.drow_guard"],
            ),
            room(
                "room.velkyn_oloth.hold_interior",
                "Hold Interior",
                RoomKind::Chamber,
                "Webbed stone halls lit by faerie fire.",
                RoomConcealment::None,
                &["npc.velkyn_oloth.drow_warrior"],
            ),
            room(
                "room.velkyn_oloth.deep_city_hook",
                "Passage to the Deeper City",
                RoomKind::Landmark,
                "A guarded descent hinting at a larger drow city below (out of scope here).",
                RoomConcealment::HiddenLayer,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.velkyn_oloth.gate_to_interior",
                "room.velkyn_oloth.outpost_gate",
                VELKYN_OLOTH_AREA_ID,
                Some("room.velkyn_oloth.hold_interior"),
                ConnectionDirection::In,
                TravelKind::Gate,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.velkyn_oloth.interior_to_hook",
                "room.velkyn_oloth.hold_interior",
                VELKYN_OLOTH_AREA_ID,
                Some("room.velkyn_oloth.deep_city_hook"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.velkyn_oloth.to_underdark_tunnels",
                "room.velkyn_oloth.outpost_gate",
                UNDERDARK_TUNNELS_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.velkyn_oloth.to_arin_gird",
                "room.velkyn_oloth.hold_interior",
                ARIN_GIRD_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.velkyn_oloth.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.velkyn_oloth.population",
        repop_budget: 20,
        repop_minutes: 15,
        pressure_tier: 2,
        tileset_id: "tileset.lair.velkyn_oloth",
        model_id: "model.lair.velkyn_oloth",
        props: vec![
            "prop.underdark.faerie_fire_sconce",
            "prop.underdark.drow_web_hanging",
            "prop.underdark.obsidian_gate",
        ],
        silhouette: "webbed obsidian dark-elf hold lit in cold violet faerie fire",
        material_notes: "Original Thera-drow stonework and heraldry; no copied iconography.",
        notes: vec![
            "Duergar/goblin/orc hide terrains (#1606) and permanent darkness (#1618) apply \
            throughout this hold.",
        ],
    })
}

pub(crate) fn arin_gird_area_spec() -> AreaSpec {
    let mut spec = build_wild_area(AreaBlueprint {
        area_id: ARIN_GIRD_AREA_ID,
        display_name: "Arin-gird",
        area_kind: AreaKind::Lair,
        source_issue: 1649,
        canon_issues: vec![987, 1598, 1632, 1649, 1006],
        wiki_slug: "arin-gird",
        source_history_label: "CF Arin-gird factual topology and mechanics only",
        summary: "The duergar city and duergar hometown (#1006); CF ranking guide places duergar at \
            40-51 for good-aligned characters.",
        level_band: (40, 51),
        rooms: vec![
            room(
                "room.arin_gird.gate",
                "Arin-gird Gate",
                RoomKind::Gate,
                "The iron-bound gate of the dark dwarf city.",
                RoomConcealment::GateChokepoint,
                &["npc.arin_gird.duergar_sentry"],
            ),
            room(
                "room.arin_gird.forge_hall",
                "Forge Hall",
                RoomKind::Guildhall,
                "The duergar smiths' hall, hometown training ground.",
                RoomConcealment::None,
                &["npc.arin_gird.duergar_smith"],
            ),
            room(
                "room.arin_gird.deep_market",
                "Deep Market",
                RoomKind::Market,
                "A stone-cut market lit by forge-glow, the duergar recall point.",
                RoomConcealment::None,
                &["npc.arin_gird.duergar_trader"],
            ),
        ],
        connections: vec![
            conn(
                "conn.arin_gird.gate_to_forge",
                "room.arin_gird.gate",
                ARIN_GIRD_AREA_ID,
                Some("room.arin_gird.forge_hall"),
                ConnectionDirection::In,
                TravelKind::Gate,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.arin_gird.forge_to_market",
                "room.arin_gird.forge_hall",
                ARIN_GIRD_AREA_ID,
                Some("room.arin_gird.deep_market"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.arin_gird.to_velkyn_oloth",
                "room.arin_gird.gate",
                VELKYN_OLOTH_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.arin_gird.to_gol_galath",
                "room.arin_gird.deep_market",
                GOL_GALATH_AREA_ID,
                None,
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.arin_gird.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.arin_gird.population",
        repop_budget: 35,
        repop_minutes: 15,
        pressure_tier: 3,
        tileset_id: "tileset.city.arin_gird",
        model_id: "model.city.arin_gird",
        props: vec![
            "prop.underdark.duergar_forge",
            "prop.underdark.iron_gate",
            "prop.underdark.deep_market_stall",
        ],
        silhouette: "iron-and-basalt duergar hold with forge-glow lighting the deep market",
        material_notes: "Original Thera-duergar stonework and forge iconography; no copied \
            iconography from the source MUD.",
        notes: vec![
            "Duergar hometown (#1006): is_hometown_option=true, recall at the Deep Market.",
        ],
    });
    spec.recall.is_hometown_option = true;
    spec.recall.recall_room_id = Some("room.arin_gird.deep_market".to_string());
    spec
}

pub(crate) fn gol_galath_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: GOL_GALATH_AREA_ID,
        display_name: "Gol Galath",
        area_kind: AreaKind::Region,
        source_issue: 1649,
        canon_issues: vec![987, 1598, 1632, 1649],
        wiki_slug: "gol-galath",
        source_history_label: "CF Gol Galath factual topology and mechanics only",
        summary: "A map node on the underground band between Arin-gird and the Underdark Mines; no \
            explicit CF level numbers are cited for this node, so the band below is estimated from \
            its neighbors, not a cited CF number.",
        level_band: (30, 45),
        rooms: vec![
            room(
                "room.gol_galath.cavern_approach",
                "Cavern Approach",
                RoomKind::Wilderness,
                "A wide cavern mouth between the duergar city and the mines.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.gol_galath.deep_chamber",
                "Deep Chamber",
                RoomKind::Chamber,
                "A vaulted chamber where the tunnels converge before the mines.",
                RoomConcealment::None,
                &["npc.gol_galath.cavern_dweller"],
            ),
        ],
        connections: vec![
            conn(
                "conn.gol_galath.approach_to_chamber",
                "room.gol_galath.cavern_approach",
                GOL_GALATH_AREA_ID,
                Some("room.gol_galath.deep_chamber"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.gol_galath.to_arin_gird",
                "room.gol_galath.cavern_approach",
                ARIN_GIRD_AREA_ID,
                None,
                ConnectionDirection::Up,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.gol_galath.to_underdark_mines",
                "room.gol_galath.deep_chamber",
                UNDERDARK_MINES_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.gol_galath.cycle",
        reset_minutes: 40,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.gol_galath.population",
        repop_budget: 22,
        repop_minutes: 20,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.gol_galath",
        model_id: "model.wilderness.gol_galath",
        props: vec![
            "prop.underdark.cavern_column",
            "prop.underdark.mineral_vein_glow",
            "prop.underdark.rubble_pile",
        ],
        silhouette: "vaulted cavern convergence point between duergar city and the deep mines",
        material_notes: "Original Thera-Underdark cavern geometry; no copied iconography.",
        notes: vec!["No day/night cycle underground; permanent darkness (#1618) applies."],
    })
}

pub(crate) fn all_underdark_a_specs() -> Vec<AreaSpec> {
    vec![
        underdark_tunnels_area_spec(),
        velkyn_oloth_area_spec(),
        arin_gird_area_spec(),
        gol_galath_area_spec(),
    ]
}
