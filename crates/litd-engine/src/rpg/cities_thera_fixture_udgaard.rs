// #1639 Udgaard: the northern hold, reached north out of Galadon past the
// Crossroads. CF-factual topology from the real wiki.carrionfields.net
// Isildur's Ranking Guide (Udgaard: Guild Guards/Shopkeepers 30-40; Plains of
// Udgaard: Onehearts/Seers 15-20+; Northern Mountains: Udgaard Patrollers
// 35-45), helpfile 2167 (Udgaard hosts a PICKPOCKET branch, Plant first skill,
// no membership -- Galadon is the pickpocket HQ), and helpfile 2523 (NEWBIE3):
// "Troglodyte Caverns: Straight south from Udgaard's south gate." / "Plains of
// Udgaard: Due east from Udgaard's east gate." The farm satellite is CF-factual
// as a farm reachable from Udgaard; its proper name is originalized per #984.
use litd_engine::rpg::{
    AreaAccessRule, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile, AreaResetProfile,
    AreaSpec, AreaVisualContract, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID,
    RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind, TravelKind,
};

use super::rpg_cities_thera_fixture_support::{external, hidden_internal, internal, room};

pub(crate) const UDGAARD_AREA_ID: &str = "area.thera.udgaard";
// #1704 registry fix: Plains of Udgaard, the Troglodyte Caverns (Underdark), and
// the Northern Mountains are authored top-level by the #1649-#1660 region batch;
// the city over-namespaced them. Bjornstead Farm remains a deferred anchor
// (tracked in the registry's known-deferred ledger).
const PLAINS_OF_UDGAARD: &str = "area.thera.udgaardian_plains";
const TROGLODYTE_CAVERNS: &str = "area.thera.underdark.troglodyte_caverns";
const BJORNSTEAD_FARM: &str = "area.thera.udgaard.bjornstead_farm";
const NORTHERN_MOUNTAINS: &str = "area.thera.northern_mountains";

pub(crate) fn udgaard_known_area_ids() -> [&'static str; 6] {
    [
        UDGAARD_AREA_ID,
        GALADON_AREA_ID,
        PLAINS_OF_UDGAARD,
        TROGLODYTE_CAVERNS,
        BJORNSTEAD_FARM,
        NORTHERN_MOUNTAINS,
    ]
}

pub(crate) fn udgaard_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: UDGAARD_AREA_ID.to_string(),
        display_name: "Udgaard".to_string(),
        player_facing_canon_name: "Udgaard".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1639,
        canon_issues: vec![987, 1598, 1632, 1639],
        mechanics_source_url: "https://wiki.carrionfields.net/guide/isildurs-ranking-guide"
            .to_string(),
        source_history_label: "CF Udgaard factual topology/mechanics (Isildur's Ranking Guide; \
            thief-guild-path helpfile 2167; NEWBIE3 helpfile 2523 south/east gate directions) \
            only; name/flavor phrasing original, farm satellite name originalized."
            .to_string(),
        summary: "The northern hold city, reached past the Crossroads north of Galadon: a \
            fortified square with a pickpocket guild branch, an east gate onto the Plains of \
            Udgaard, and a south gate onto the road to the troglodyte caverns."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 45,
        },
        rooms: vec![
            room(
                "room.udgaard.north_road",
                "North Road",
                RoomKind::Street,
                "The road down from the Crossroads north of Galadon.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.udgaard.south_gate",
                "South Gate",
                RoomKind::Gate,
                "Southern gate opening onto the road toward the troglodyte caverns.",
                RoomConcealment::GateChokepoint,
                &["npc.udgaard.gate_guard"],
            ),
            room(
                "room.udgaard.east_gate",
                "East Gate",
                RoomKind::Gate,
                "Eastern gate opening onto the Plains of Udgaard.",
                RoomConcealment::GateChokepoint,
                &["npc.udgaard.gate_guard"],
            ),
            room(
                "room.udgaard.hold_square",
                "Hold Square",
                RoomKind::Intersection,
                "The fortified heart of the hold, where the roads and gates converge.",
                RoomConcealment::CrowdCover,
                &["npc.udgaard.crier"],
            ),
            room(
                "room.udgaard.pickpocket_guildhall",
                "Pickpocket Guildhall",
                RoomKind::Guildhall,
                "A branch hall teaching the Plant skill; no membership required here.",
                RoomConcealment::AlleyShadow,
                &["npc.udgaard.pickpocket_path_trainer"],
            ),
            room(
                "room.udgaard.warriors_guild",
                "Warriors' Guildhall",
                RoomKind::Guildhall,
                "Training hall for the martial classes.",
                RoomConcealment::None,
                &["npc.udgaard.warrior_guildmaster"],
            ),
            room(
                "room.udgaard.temple",
                "Udgaard Temple",
                RoomKind::Temple,
                "The altar that serves as the city recall point.",
                RoomConcealment::None,
                &["npc.udgaard.priest"],
            ),
            room(
                "room.udgaard.inn",
                "Udgaard Inn",
                RoomKind::Inn,
                "A warm hearth against the northern cold.",
                RoomConcealment::None,
                &["npc.udgaard.innkeeper"],
            ),
            room(
                "room.udgaard.cartographer",
                "Cartographer's Shop",
                RoomKind::Cartographer,
                "Sells region maps as acquired items; no free minimap is granted.",
                RoomConcealment::None,
                &["npc.udgaard.cartographer"],
            ),
            room(
                "room.udgaard.academy",
                "Udgaard Academy",
                RoomKind::Academy,
                "The level 1-5 school entrance for characters hometowned here.",
                RoomConcealment::None,
                &["npc.udgaard.academy_guildmaster"],
            ),
            room(
                "room.udgaard.market",
                "Udgaard Market",
                RoomKind::Market,
                "Stalls of hold-guards and shopkeepers.",
                RoomConcealment::CrowdCover,
                &["npc.udgaard.shopkeeper"],
            ),
        ],
        connections: vec![
            internal(
                "conn.udgaard.north_road_to_square",
                "room.udgaard.north_road",
                UDGAARD_AREA_ID,
                "room.udgaard.hold_square",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            internal(
                "conn.udgaard.square_to_south_gate",
                "room.udgaard.hold_square",
                UDGAARD_AREA_ID,
                "room.udgaard.south_gate",
                ConnectionDirection::South,
                TravelKind::Gate,
            ),
            internal(
                "conn.udgaard.square_to_east_gate",
                "room.udgaard.hold_square",
                UDGAARD_AREA_ID,
                "room.udgaard.east_gate",
                ConnectionDirection::East,
                TravelKind::Gate,
            ),
            internal(
                "conn.udgaard.square_to_warriors",
                "room.udgaard.hold_square",
                UDGAARD_AREA_ID,
                "room.udgaard.warriors_guild",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.udgaard.square_to_temple",
                "room.udgaard.hold_square",
                UDGAARD_AREA_ID,
                "room.udgaard.temple",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.udgaard.square_to_inn",
                "room.udgaard.hold_square",
                UDGAARD_AREA_ID,
                "room.udgaard.inn",
                ConnectionDirection::Northwest,
                TravelKind::Road,
            ),
            internal(
                "conn.udgaard.square_to_cartographer",
                "room.udgaard.hold_square",
                UDGAARD_AREA_ID,
                "room.udgaard.cartographer",
                ConnectionDirection::Northeast,
                TravelKind::Road,
            ),
            internal(
                "conn.udgaard.square_to_academy",
                "room.udgaard.hold_square",
                UDGAARD_AREA_ID,
                "room.udgaard.academy",
                ConnectionDirection::Southwest,
                TravelKind::Road,
            ),
            internal(
                "conn.udgaard.square_to_market",
                "room.udgaard.hold_square",
                UDGAARD_AREA_ID,
                "room.udgaard.market",
                ConnectionDirection::Southeast,
                TravelKind::Road,
            ),
            // Hidden alley entrance to the pickpocket branch (#944).
            hidden_internal(
                "conn.udgaard.square_to_pickpockets",
                "room.udgaard.hold_square",
                UDGAARD_AREA_ID,
                "room.udgaard.pickpocket_guildhall",
                ConnectionDirection::Up,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            // External edges to the hub, plains, caverns, farm, and mountain patrol routes.
            external(
                "conn.udgaard.to_galadon",
                "room.udgaard.north_road",
                GALADON_AREA_ID,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.udgaard.to_plains",
                "room.udgaard.east_gate",
                PLAINS_OF_UDGAARD,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.udgaard.to_trog_caverns",
                "room.udgaard.south_gate",
                TROGLODYTE_CAVERNS,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.udgaard.to_farm",
                "room.udgaard.hold_square",
                BJORNSTEAD_FARM,
                ConnectionDirection::Northeast,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.udgaard.to_northern_mountains",
                "room.udgaard.east_gate",
                NORTHERN_MOUNTAINS,
                ConnectionDirection::Northeast,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.udgaard.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.udgaard.city_population".to_string(),
            baseline_population_budget: 30,
            repop_interval_minutes: 15,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.udgaard.temple".to_string()),
            has_academy: true,
            // No explicit alignment gate is documented for Udgaard hometown selection
            // (unlike Galadon/Tir-Talath/Darsylon); treated as open (judgment call, see PR).
            hometown_access: AreaAccessRule::Any,
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.udgaard.nordic_hold".to_string(),
            model_id: "model.city.udgaard.hold".to_string(),
            required_props: vec![
                "prop.city.gate_portcullis".to_string(),
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.guild_banner".to_string(),
            ],
            silhouette: "fortified nordic hold city with a stockaded square, east gate onto open \
                plains, south gate onto rocky caverns"
                .to_string(),
            material_notes: "Original Udgaard timber-and-stone hold work; no copied iconography \
                from the source MUD."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Topology is discoverable, not an omniscient minimap (#944): the pickpocket branch \
                entrance and the Northern Mountains patrol trail are Hidden/ConcealedTrail."
                .to_string(),
            "Plains of Udgaard, the troglodyte caverns, the farm satellite, and the Northern \
                Mountains patrol routes are satellite areas authored by later batch issues and \
                referenced here as connections."
                .to_string(),
            "Open-plains sightline/camouflage terrain profile (#1612 plainsrunner) is deferred \
                to the terrain system, not modeled as AreaSpec fields here."
                .to_string(),
        ],
    }
}
