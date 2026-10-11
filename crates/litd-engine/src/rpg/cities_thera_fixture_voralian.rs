// #1637 Voralian City: the western twin city, reached from Galadon via
// Shepherds Row. CF-factual topology sourced from the real, fetched
// wiki.carrionfields.net "Isildur's Ranking Guide" page (Voralian City rows:
// 10-20 good "Serving Girls, Etc.", 10-20+ evil "Commoners, Etc.", 30-40 evil
// "Shopkeepers") plus helpfile 2167 ('THIEF PATHS' ... GUILDHALLS) which lists
// Voralian City as the POISONER guild HQ (Envenom first skill, membership
// required, kingpin, hometown-switch on join) and helpfile 2523 (NEWBIE3) which
// gives the Loch Grynmear approach: "From the Voralian City, head east down
// Silverpine Road until it ends. Go east, and look for a path that leads up
// into the surrounding wooded hills." Only names/flavor text are original
// phrasing; the topology and mechanics facts above are as sourced (#984).
use litd_engine::rpg::{
    AreaAccessRule, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile, AreaResetProfile,
    AreaSpec, AreaVisualContract, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID,
    RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind, TravelKind,
};

use super::rpg_cities_thera_fixture_support::{external, hidden_internal, internal, room};

pub(crate) const VORALIAN_AREA_ID: &str = "area.thera.voralian";
// #1704 registry fix: these edges targeted city-namespaced ids that no spec
// authored; the real authored areas are the top-level Loch Grynmear (#1656
// wetlands batch) and Blackwater (#1656 wetlands batch). Bramblefield remains a
// deferred anchor (tracked in the registry's known-deferred ledger).
const LOCH_GRYNMEAR: &str = "area.thera.loch_grynmear";
const BRAMBLEFIELD: &str = "area.thera.voralian.bramblefield";
const BLACKWATER_APPROACH: &str = "area.thera.blackwater";

pub(crate) fn voralian_known_area_ids() -> [&'static str; 5] {
    [
        VORALIAN_AREA_ID,
        GALADON_AREA_ID,
        LOCH_GRYNMEAR,
        BRAMBLEFIELD,
        BLACKWATER_APPROACH,
    ]
}

pub(crate) fn voralian_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: VORALIAN_AREA_ID.to_string(),
        display_name: "Voralian City".to_string(),
        player_facing_canon_name: "Voralian City".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1637,
        canon_issues: vec![987, 1598, 1632, 1637],
        mechanics_source_url: "https://wiki.carrionfields.net/guide/isildurs-ranking-guide"
            .to_string(),
        source_history_label: "CF Voralian City factual topology/mechanics (Isildur's Ranking \
            Guide levels; thief-guild-path helpfile 2167; NEWBIE3 helpfile 2523 Loch Grynmear \
            directions) only; name/flavor phrasing original."
            .to_string(),
        summary: "The western twin city, reached from Galadon by the road through Shepherds Row: \
            a North-South Road spine, Silverpine Road running east toward Loch Grynmear, and the \
            poisoner thieves' guildhall in its shadowed backstreets."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 40,
        },
        rooms: vec![
            room(
                "room.voralian.shepherds_gate",
                "Shepherds Gate",
                RoomKind::Gate,
                "The western gate onto Shepherds Row, the road connecting to Galadon.",
                RoomConcealment::GateChokepoint,
                &["npc.voralian.gate_guard"],
            ),
            room(
                "room.voralian.shepherds_row",
                "Shepherds Row",
                RoomKind::Street,
                "The approach road outside the gate, worn smooth by trade caravans from Galadon.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.voralian.city_square",
                "Voralian City Square",
                RoomKind::Intersection,
                "The crowded heart of the city where the North-South Road, Silverpine Road, and \
                    the market all meet.",
                RoomConcealment::CrowdCover,
                &["npc.voralian.crier", "npc.voralian.pickpocket"],
            ),
            room(
                "room.voralian.ns_road",
                "North-South Road",
                RoomKind::Street,
                "The road running the length of the city, connecting its northern and southern \
                    approaches.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.voralian.silverpine_road",
                "Silverpine Road",
                RoomKind::Street,
                "The eastern road out of the city, running toward Loch Grynmear.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.voralian.market",
                "Voralian Market",
                RoomKind::Market,
                "Stalls thick with serving girls, commoners, and shopkeepers.",
                RoomConcealment::CrowdCover,
                &["npc.voralian.serving_girl", "npc.voralian.shopkeeper"],
            ),
            room(
                "room.voralian.poisoner_guildhall",
                "Poisoner Guildhall",
                RoomKind::Guildhall,
                "The venom-guild's backstreet hall; membership here switches a joiner's hometown.",
                RoomConcealment::AlleyShadow,
                &["npc.voralian.poisoner_kingpin"],
            ),
            room(
                "room.voralian.warriors_guild",
                "Warriors' Guildhall",
                RoomKind::Guildhall,
                "Training hall for the martial classes.",
                RoomConcealment::None,
                &["npc.voralian.warrior_guildmaster"],
            ),
            room(
                "room.voralian.mages_guild",
                "Mages' Guildhall",
                RoomKind::Guildhall,
                "Spell-tutors for the arcane classes.",
                RoomConcealment::None,
                &["npc.voralian.mage_guildmaster"],
            ),
            room(
                "room.voralian.temple",
                "Voralian Temple",
                RoomKind::Temple,
                "The altar that serves as the city recall point.",
                RoomConcealment::None,
                &["npc.voralian.priest"],
            ),
            room(
                "room.voralian.inn",
                "Voralian Inn",
                RoomKind::Inn,
                "Rooms and a hearth for travelers on the western road.",
                RoomConcealment::None,
                &["npc.voralian.innkeeper"],
            ),
            room(
                "room.voralian.cartographer",
                "Cartographer's Shop",
                RoomKind::Cartographer,
                "Sells region maps as acquired items; no free minimap is granted.",
                RoomConcealment::None,
                &["npc.voralian.cartographer"],
            ),
            room(
                "room.voralian.academy",
                "Voralian Academy",
                RoomKind::Academy,
                "The level 1-5 school entrance for characters hometowned here.",
                RoomConcealment::None,
                &["npc.voralian.academy_guildmaster"],
            ),
        ],
        connections: vec![
            internal(
                "conn.voralian.gate_to_row",
                "room.voralian.shepherds_gate",
                VORALIAN_AREA_ID,
                "room.voralian.shepherds_row",
                ConnectionDirection::West,
                TravelKind::Gate,
            ),
            internal(
                "conn.voralian.row_to_square",
                "room.voralian.shepherds_row",
                VORALIAN_AREA_ID,
                "room.voralian.city_square",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_ns_road",
                "room.voralian.city_square",
                VORALIAN_AREA_ID,
                "room.voralian.ns_road",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_silverpine",
                "room.voralian.city_square",
                VORALIAN_AREA_ID,
                "room.voralian.silverpine_road",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_market",
                "room.voralian.city_square",
                VORALIAN_AREA_ID,
                "room.voralian.market",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_warriors",
                "room.voralian.city_square",
                VORALIAN_AREA_ID,
                "room.voralian.warriors_guild",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_mages",
                "room.voralian.city_square",
                VORALIAN_AREA_ID,
                "room.voralian.mages_guild",
                ConnectionDirection::Northeast,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_temple",
                "room.voralian.city_square",
                VORALIAN_AREA_ID,
                "room.voralian.temple",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_inn",
                "room.voralian.city_square",
                VORALIAN_AREA_ID,
                "room.voralian.inn",
                ConnectionDirection::Southwest,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.ns_road_to_cartographer",
                "room.voralian.ns_road",
                VORALIAN_AREA_ID,
                "room.voralian.cartographer",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_academy",
                "room.voralian.city_square",
                VORALIAN_AREA_ID,
                "room.voralian.academy",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            // Hidden alley entrance to the poisoner guild (#944: guild backrooms are not
            // plainly visible from the crowded square).
            hidden_internal(
                "conn.voralian.square_to_poisoners",
                "room.voralian.city_square",
                VORALIAN_AREA_ID,
                "room.voralian.poisoner_guildhall",
                ConnectionDirection::Southeast,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            // External edges: back to the hub, and out to the satellites/approaches (#1632).
            external(
                "conn.voralian.to_galadon",
                "room.voralian.shepherds_row",
                GALADON_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.voralian.to_loch_grynmear",
                "room.voralian.silverpine_road",
                LOCH_GRYNMEAR,
                ConnectionDirection::East,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.voralian.to_bramblefield",
                "room.voralian.ns_road",
                BRAMBLEFIELD,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.voralian.to_blackwater",
                "room.voralian.ns_road",
                BLACKWATER_APPROACH,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.voralian.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.voralian.city_population".to_string(),
            baseline_population_budget: 35,
            repop_interval_minutes: 15,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.voralian.temple".to_string()),
            has_academy: true,
            // Documented (help newbie) as the overflow hometown alongside Galadon when
            // Galadon is full; treated the same as Galadon's good/neutral gate (#1634).
            hometown_access: AreaAccessRule::Restricted {
                values: vec![
                    "alignment.good".to_string(),
                    "alignment.neutral".to_string(),
                ],
            },
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.voralian.stone_streets".to_string(),
            model_id: "model.city.voralian.twin_city".to_string(),
            required_props: vec![
                "prop.city.gate_portcullis".to_string(),
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.guild_banner".to_string(),
                "prop.city.venom_vial_sign".to_string(),
            ],
            silhouette: "twin-city crossroads split by a North-South Road spine, poison-guild \
                alleyways, wooded eastern approach"
                .to_string(),
            material_notes: "Original Voralian stonework, banners, and signage; no copied \
                iconography from the source MUD."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Topology is discoverable, not an omniscient minimap (#944): the poisoner guild's \
                alley entrance and the Blackwater approach are Hidden; the Loch Grynmear path is \
                a ConcealedTrail."
                .to_string(),
            "Poisoner guild membership auto-switches hometown per helpfile 2167; this is a \
                gameplay rule enforced elsewhere, noted here for context only."
                .to_string(),
            "Bramblefield and Blackwater approaches, and Loch Grynmear itself, are satellite \
                areas authored by later batch issues and referenced here as connections."
                .to_string(),
        ],
    }
}
