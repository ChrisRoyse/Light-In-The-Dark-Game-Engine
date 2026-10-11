// #1649 world/region: the Underdark realm, part B (Underdark Mines, Troglodyte
// Caverns, Svirfneblin Enclave). Split from regions_thera_fixture_underdark.rs
// (part A: tunnels, Velkyn Oloth, Arin-gird, Gol Galath) to respect the
// <500-line ratchet (#1466/#800 sec.6) after rustfmt expansion. Mechanics
// captured factually from the issue body (itself CF-factual per #984); only
// flavor prose is authored fresh.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::regions_thera_fixture_underdark::{GOL_GALATH_AREA_ID, UNDERDARK_TUNNELS_AREA_ID};
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const UNDERDARK_MINES_AREA_ID: &str = "area.thera.underdark.mines";
pub(crate) const TROGLODYTE_CAVERNS_AREA_ID: &str = "area.thera.underdark.troglodyte_caverns";
pub(crate) const SVIRFNEBLIN_ENCLAVE_AREA_ID: &str = "area.thera.underdark.svirfneblin_enclave";

pub(crate) fn underdark_b_area_ids() -> [&'static str; 3] {
    [
        UNDERDARK_MINES_AREA_ID,
        TROGLODYTE_CAVERNS_AREA_ID,
        SVIRFNEBLIN_ENCLAVE_AREA_ID,
    ]
}

pub(crate) fn underdark_mines_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: UNDERDARK_MINES_AREA_ID,
        display_name: "Underdark Mines",
        area_kind: AreaKind::Dungeon,
        source_issue: 1649,
        canon_issues: vec![987, 1598, 1632, 1649],
        wiki_slug: "underdark-mines",
        source_history_label: "CF Underdark Mines factual topology and mechanics only",
        summary: "An endgame explore-class dungeon at the deepest reach of the tunnel web.",
        level_band: (45, 51),
        rooms: vec![
            room(
                "room.underdark_mines.entrance",
                "Mine Entrance",
                RoomKind::Gate,
                "A collapsed-looking shaft mouth that opens into worked stone.",
                RoomConcealment::GateChokepoint,
                &["npc.underdark_mines.mine_warden"],
            ),
            room(
                "room.underdark_mines.shaft",
                "Descending Shaft",
                RoomKind::Wilderness,
                "A long worked shaft spiraling down into darkness.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.underdark_mines.deep_vein",
                "Deep Vein",
                RoomKind::Chamber,
                "The endgame explore chamber at the bottom of the mines.",
                RoomConcealment::None,
                &["npc.underdark_mines.deep_guardian"],
            ),
        ],
        connections: vec![
            conn(
                "conn.underdark_mines.entrance_to_shaft",
                "room.underdark_mines.entrance",
                UNDERDARK_MINES_AREA_ID,
                Some("room.underdark_mines.shaft"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.underdark_mines.shaft_to_vein",
                "room.underdark_mines.shaft",
                UNDERDARK_MINES_AREA_ID,
                Some("room.underdark_mines.deep_vein"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.underdark_mines.to_gol_galath",
                "room.underdark_mines.entrance",
                GOL_GALATH_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.underdark_mines.cycle",
        reset_minutes: 45,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.underdark_mines.population",
        repop_budget: 18,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.dungeon.underdark_mines",
        model_id: "model.dungeon.underdark_mines",
        props: vec![
            "prop.underdark.mine_cart",
            "prop.underdark.support_timber",
            "prop.underdark.ore_vein_glow",
        ],
        silhouette: "deep worked mine shaft ending in a raw ore chamber",
        material_notes: "Original Thera-mine timber and stonework; no copied iconography.",
        notes: vec![
            "Endgame explore-class per source; no built-in map (#944), full darkness (#1618).",
        ],
    })
}

pub(crate) fn troglodyte_caverns_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: TROGLODYTE_CAVERNS_AREA_ID,
        display_name: "Troglodyte Caverns",
        area_kind: AreaKind::Lair,
        source_issue: 1649,
        canon_issues: vec![987, 1598, 1632, 1649],
        wiki_slug: "troglodyte-caverns",
        source_history_label: "CF Troglodyte Caverns factual topology and mechanics only",
        summary: "A newbie-band entrance to the deep (NW surface entrance per NEWBIE3): the taste of \
            the Underdark for low-level explorers.",
        level_band: (5, 15),
        rooms: vec![
            room(
                "room.troglodyte_caverns.nw_entrance",
                "Northwest Surface Entrance",
                RoomKind::Gate,
                "A low cave mouth in the northwest hills, the newbie route to the deep.",
                RoomConcealment::GateChokepoint,
                &[],
            ),
            room(
                "room.troglodyte_caverns.warren",
                "Trog Warren",
                RoomKind::Wilderness,
                "A muddy warren of low tunnels.",
                RoomConcealment::None,
                &["npc.troglodyte_caverns.troglodyte"],
            ),
            room(
                "room.troglodyte_caverns.nest",
                "Troglodyte Nest",
                RoomKind::Chamber,
                "The warren's breeding nest, deepest chamber before the true Underdark.",
                RoomConcealment::None,
                &["npc.troglodyte_caverns.troglodyte_chief"],
            ),
        ],
        connections: vec![
            conn(
                "conn.troglodyte_caverns.entrance_to_warren",
                "room.troglodyte_caverns.nw_entrance",
                TROGLODYTE_CAVERNS_AREA_ID,
                Some("room.troglodyte_caverns.warren"),
                ConnectionDirection::In,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.troglodyte_caverns.warren_to_nest",
                "room.troglodyte_caverns.warren",
                TROGLODYTE_CAVERNS_AREA_ID,
                Some("room.troglodyte_caverns.nest"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.troglodyte_caverns.to_galadon",
                "room.troglodyte_caverns.nw_entrance",
                GALADON_AREA_ID,
                None,
                ConnectionDirection::Southeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.troglodyte_caverns.to_underdark_tunnels",
                "room.troglodyte_caverns.nest",
                UNDERDARK_TUNNELS_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.troglodyte_caverns.cycle",
        reset_minutes: 25,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.troglodyte_caverns.population",
        repop_budget: 15,
        repop_minutes: 12,
        pressure_tier: 1,
        tileset_id: "tileset.lair.troglodyte_caverns",
        model_id: "model.lair.troglodyte_caverns",
        props: vec![
            "prop.underdark.mud_pool",
            "prop.underdark.bone_midden",
            "prop.underdark.low_cave_ceiling",
        ],
        silhouette: "low muddy cave warren with a bright surface mouth at one end",
        material_notes: "Original Thera-troglodyte den dressing; no copied iconography.",
        notes: vec![
            "The newbie taste of the deep per source; surface gate is Visible, the deeper \
            Underdark link is Hidden (#944).",
        ],
    })
}

pub(crate) fn svirfneblin_enclave_area_spec() -> AreaSpec {
    let mut spec = build_wild_area(AreaBlueprint {
        area_id: SVIRFNEBLIN_ENCLAVE_AREA_ID,
        display_name: "Svirfneblin Enclave",
        area_kind: AreaKind::Lair,
        source_issue: 1649,
        canon_issues: vec![987, 1598, 1632, 1649, 1008],
        wiki_slug: "svirfneblin-enclave",
        source_history_label: "CF Svirfneblin enclave factual topology and mechanics only",
        summary: "The deep gnome hometown (#1008), a Blingdenstone-class enclave; source gives no \
            explicit CF level range for this node, so the band below is estimated, not a cited number.",
        level_band: (20, 40),
        rooms: vec![
            room(
                "room.svirfneblin_enclave.gate",
                "Enclave Gate",
                RoomKind::Gate,
                "A stone-camouflaged gate into the deep gnome enclave.",
                RoomConcealment::GateChokepoint,
                &["npc.svirfneblin_enclave.gnome_sentry"],
            ),
            room(
                "room.svirfneblin_enclave.gnomish_hall",
                "Gnomish Hall",
                RoomKind::Guildhall,
                "The hometown hall of the svirfneblin.",
                RoomConcealment::None,
                &["npc.svirfneblin_enclave.gnome_elder"],
            ),
            room(
                "room.svirfneblin_enclave.mushroom_garden",
                "Mushroom Garden",
                RoomKind::Landmark,
                "Cultivated glow-mushroom terraces and the enclave recall point.",
                RoomConcealment::None,
                &["npc.svirfneblin_enclave.gnome_farmer"],
            ),
        ],
        connections: vec![
            conn(
                "conn.svirfneblin_enclave.gate_to_hall",
                "room.svirfneblin_enclave.gate",
                SVIRFNEBLIN_ENCLAVE_AREA_ID,
                Some("room.svirfneblin_enclave.gnomish_hall"),
                ConnectionDirection::In,
                TravelKind::Gate,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.svirfneblin_enclave.hall_to_garden",
                "room.svirfneblin_enclave.gnomish_hall",
                SVIRFNEBLIN_ENCLAVE_AREA_ID,
                Some("room.svirfneblin_enclave.mushroom_garden"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.svirfneblin_enclave.to_underdark_tunnels",
                "room.svirfneblin_enclave.gate",
                UNDERDARK_TUNNELS_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
        ],
        reset_id: "reset.svirfneblin_enclave.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.svirfneblin_enclave.population",
        repop_budget: 25,
        repop_minutes: 15,
        pressure_tier: 2,
        tileset_id: "tileset.city.svirfneblin_enclave",
        model_id: "model.city.svirfneblin_enclave",
        props: vec![
            "prop.underdark.glow_mushroom_terrace",
            "prop.underdark.camouflaged_stone_gate",
            "prop.underdark.gnomish_workbench",
        ],
        silhouette: "camouflaged deep-gnome enclave built into a mushroom-lit cavern wall",
        material_notes: "Original Thera-svirfneblin stonework and mushroom-terrace dressing; no \
            copied iconography.",
        notes: vec![
            "Deep gnome hometown (#1008): is_hometown_option=true, recall at the Mushroom \
            Garden. Enclave gate is a ConcealedTrail from the tunnel web (#944).",
        ],
    });
    spec.recall.is_hometown_option = true;
    spec.recall.recall_room_id = Some("room.svirfneblin_enclave.mushroom_garden".to_string());
    spec
}

pub(crate) fn all_underdark_b_specs() -> Vec<AreaSpec> {
    vec![
        underdark_mines_area_spec(),
        troglodyte_caverns_area_spec(),
        svirfneblin_enclave_area_spec(),
    ]
}
