// #1641 Arkham: the eastern crossroads city on the Eaststride Road. CF-factual
// topology from the real wiki.carrionfields.net Isildur's Ranking Guide
// (Arkham: Guards/Shopkeepers 25-35 evil; Forgotten Crypts (Arkham):
// Zombies/Skeletons 20-25+ good; Village of Azuremain: Explorers 30-35+ evil),
// helpfile 2167 (Arkham hosts the MYSTIC path, Scrolls first skill, no
// membership anywhere for this path), helpfile 2523 (NEWBIE3): "V'Dramir's
// Cove: From Arkham's Town Square, head north along Dragon Road," and the real
// wiki.carrionfields.net ranger class page ("rangers... practice... north of
// Arkham"). Only names/flavor phrasing are original (#984).
use litd_engine::rpg::{
    AreaAccessRule, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile, AreaResetProfile,
    AreaSpec, AreaVisualContract, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID,
    RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind, TravelKind,
};

use super::rpg_cities_thera_fixture_support::{external, hidden_internal, internal, room};

pub(crate) const ARKHAM_AREA_ID: &str = "area.thera.arkham";
// #1704 registry fix: the Forgotten Crypts are authored top-level by the #1657
// dungeon batch. The Dragon Port, Outlying Villages, Ranger Woods, and the
// Village of Azuremain remain deferred anchors (tracked in the registry's
// known-deferred ledger).
const THE_DRAGON_PORT: &str = "area.thera.arkham.the_dragon_port";
const FORGOTTEN_CRYPTS: &str = "area.thera.forgotten_crypts";
const OUTLYING_VILLAGES: &str = "area.thera.arkham.outlying_villages";
const RANGER_WOODS: &str = "area.thera.arkham.ranger_woods";
const VILLAGE_OF_AZUREMAIN: &str = "area.thera.arkham.village_of_azuremain";

pub(crate) fn arkham_known_area_ids() -> [&'static str; 7] {
    [
        ARKHAM_AREA_ID,
        GALADON_AREA_ID,
        THE_DRAGON_PORT,
        FORGOTTEN_CRYPTS,
        OUTLYING_VILLAGES,
        RANGER_WOODS,
        VILLAGE_OF_AZUREMAIN,
    ]
}

pub(crate) fn arkham_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: ARKHAM_AREA_ID.to_string(),
        display_name: "Arkham".to_string(),
        player_facing_canon_name: "Arkham".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1641,
        canon_issues: vec![987, 1598, 1632, 1641],
        mechanics_source_url: "https://wiki.carrionfields.net/guide/isildurs-ranking-guide"
            .to_string(),
        source_history_label: "CF Arkham factual topology/mechanics (Isildur's Ranking Guide; \
            thief-guild-path helpfile 2167; NEWBIE3 helpfile 2523 Dragon Road directions; ranger \
            class page 'north of Arkham' practice grounds) only; name/flavor phrasing original."
            .to_string(),
        summary: "The eastern crossroads city at the end of the Eaststride Road: Town Square \
            where Dragon Road turns north toward the coast, the mystic thieves' scroll-trade, and \
            the Forgotten Crypts beneath."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 35,
        },
        rooms: vec![
            room(
                "room.arkham.town_square",
                "Arkham Town Square",
                RoomKind::Intersection,
                "Where the Eaststride Road meets Dragon Road.",
                RoomConcealment::CrowdCover,
                &["npc.arkham.crier"],
            ),
            room(
                "room.arkham.eaststride_road",
                "Eaststride Road",
                RoomKind::Street,
                "The western approach road, connecting back toward Galadon.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.arkham.dragon_road",
                "Dragon Road",
                RoomKind::Street,
                "The northern road toward the coast and the Dragon port.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.arkham.mystic_guildhall",
                "Mystic Guildhall",
                RoomKind::Guildhall,
                "Teaches the Scrolls path; no membership required anywhere for this path.",
                RoomConcealment::AlleyShadow,
                &["npc.arkham.mystic_scroll_trainer"],
            ),
            room(
                "room.arkham.warriors_guild",
                "Warriors' Guildhall",
                RoomKind::Guildhall,
                "Training hall for the martial classes.",
                RoomConcealment::None,
                &["npc.arkham.warrior_guildmaster"],
            ),
            room(
                "room.arkham.temple",
                "Arkham Temple",
                RoomKind::Temple,
                "The altar that serves as the city recall point.",
                RoomConcealment::None,
                &["npc.arkham.priest"],
            ),
            room(
                "room.arkham.inn",
                "Arkham Inn",
                RoomKind::Inn,
                "Rooms and a hearth for travelers off the Eaststride Road.",
                RoomConcealment::None,
                &["npc.arkham.innkeeper"],
            ),
            room(
                "room.arkham.cartographer",
                "Cartographer's Shop",
                RoomKind::Cartographer,
                "Sells region maps as acquired items; no free minimap is granted.",
                RoomConcealment::None,
                &["npc.arkham.cartographer"],
            ),
            room(
                "room.arkham.market",
                "Arkham Market",
                RoomKind::Market,
                "Stalls of guild guards and shopkeepers.",
                RoomConcealment::CrowdCover,
                &["npc.arkham.shopkeeper"],
            ),
        ],
        connections: vec![
            internal(
                "conn.arkham.square_to_eaststride",
                "room.arkham.town_square",
                ARKHAM_AREA_ID,
                "room.arkham.eaststride_road",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.arkham.square_to_dragon_road",
                "room.arkham.town_square",
                ARKHAM_AREA_ID,
                "room.arkham.dragon_road",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.arkham.square_to_warriors",
                "room.arkham.town_square",
                ARKHAM_AREA_ID,
                "room.arkham.warriors_guild",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.arkham.square_to_temple",
                "room.arkham.town_square",
                ARKHAM_AREA_ID,
                "room.arkham.temple",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.arkham.square_to_inn",
                "room.arkham.town_square",
                ARKHAM_AREA_ID,
                "room.arkham.inn",
                ConnectionDirection::Southwest,
                TravelKind::Road,
            ),
            internal(
                "conn.arkham.square_to_cartographer",
                "room.arkham.town_square",
                ARKHAM_AREA_ID,
                "room.arkham.cartographer",
                ConnectionDirection::Southeast,
                TravelKind::Road,
            ),
            internal(
                "conn.arkham.square_to_market",
                "room.arkham.town_square",
                ARKHAM_AREA_ID,
                "room.arkham.market",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            // Hidden alley entrance to the mystic guild (#944).
            hidden_internal(
                "conn.arkham.square_to_mystics",
                "room.arkham.town_square",
                ARKHAM_AREA_ID,
                "room.arkham.mystic_guildhall",
                ConnectionDirection::Northeast,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            // External edges: hub, Dragon port, Forgotten Crypts, Outlying Villages, ranger
            // practice woods north of Arkham, and Village of Azuremain (#1632).
            external(
                "conn.arkham.to_galadon",
                "room.arkham.eaststride_road",
                GALADON_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.arkham.to_dragon_port",
                "room.arkham.dragon_road",
                THE_DRAGON_PORT,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.arkham.to_forgotten_crypts",
                "room.arkham.town_square",
                FORGOTTEN_CRYPTS,
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.arkham.to_outlying_villages",
                "room.arkham.town_square",
                OUTLYING_VILLAGES,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.arkham.to_ranger_woods",
                "room.arkham.town_square",
                RANGER_WOODS,
                ConnectionDirection::North,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.arkham.to_azuremain",
                "room.arkham.town_square",
                VILLAGE_OF_AZUREMAIN,
                ConnectionDirection::Southeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.arkham.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.arkham.city_population".to_string(),
            baseline_population_budget: 35,
            repop_interval_minutes: 15,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.arkham.temple".to_string()),
            has_academy: false,
            // Isildur's guide lists Arkham's own guards/shopkeepers only under the evil-ranking
            // table, but the issue does not make an explicit hometown-alignment claim the way
            // Tir-Talath/Darsylon do; treated as open rather than over-claiming (judgment call).
            hometown_access: AreaAccessRule::Any,
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.arkham.crossroads".to_string(),
            model_id: "model.city.arkham.eastern_gateway".to_string(),
            required_props: vec![
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.guild_banner".to_string(),
            ],
            silhouette: "eastern crossroads city where the Eaststride Road ends and Dragon Road \
                turns north toward the coast"
                .to_string(),
            material_notes: "Original Arkham timber-frame and slate roofing; no copied \
                iconography from the source MUD."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Topology is discoverable, not an omniscient minimap (#944): the mystic guild alley, \
                the Forgotten Crypts descent, and the ranger practice woods are Hidden/\
                ConcealedTrail."
                .to_string(),
            "The Dragon port, Forgotten Crypts, Outlying Villages, ranger practice woods, and \
                Village of Azuremain are satellite areas authored by later batch issues and \
                referenced here as connections."
                .to_string(),
            "Rural hedgerow ambush lanes and crypt-darkness stealth design notes are deferred to \
                the terrain system, not modeled as AreaSpec fields here."
                .to_string(),
        ],
    }
}
