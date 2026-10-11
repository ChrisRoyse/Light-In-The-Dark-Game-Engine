// #1642 Tir-Talath: the evil mage city, interior NW of Galadon on the map
// spine. CF-factual topology from the real wiki.carrionfields.net Isildur's
// Ranking Guide (Tir-Talath: Mages/Shopkeepers/Militiamen 15-20, EVIL aligned
// characters table), helpfile 2167 (Tir-Talath hosts a THUG branch, Push first
// skill, no membership -- Dagdan is the thug HQ, not authored in this batch),
// and the real wiki.carrionfields.net ranger class page ("rangers... practice
// ... south of Tir-Talath"). Only names/flavor phrasing are original (#984).
use litd_engine::rpg::{
    AreaAccessRule, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile, AreaResetProfile,
    AreaSpec, AreaVisualContract, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID,
    RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind, TravelKind,
};

use super::rpg_cities_thera_fixture_darsylon::DARSYLON_AREA_ID;
use super::rpg_cities_thera_fixture_support::{external, hidden_internal, internal, room};

pub(crate) const TIR_TALATH_AREA_ID: &str = "area.thera.tir_talath";
// #1704 registry fix: the road out of Tir-Talath runs to Cragstone, which is
// authored top-level (#1651 mountains batch); point the edge there. The southern
// Ranger Woods remains a deferred anchor (tracked in the registry's
// known-deferred ledger).
const CRAGSTONE_ROAD: &str = "area.thera.cragstone";
const RANGER_WOODS_SOUTH: &str = "area.thera.tir_talath.ranger_woods_south";

pub(crate) fn tir_talath_known_area_ids() -> [&'static str; 5] {
    [
        TIR_TALATH_AREA_ID,
        GALADON_AREA_ID,
        CRAGSTONE_ROAD,
        RANGER_WOODS_SOUTH,
        DARSYLON_AREA_ID,
    ]
}

pub(crate) fn tir_talath_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: TIR_TALATH_AREA_ID.to_string(),
        display_name: "Tir-Talath".to_string(),
        player_facing_canon_name: "Tir-Talath".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1642,
        canon_issues: vec![987, 1598, 1632, 1642],
        mechanics_source_url: "https://wiki.carrionfields.net/guide/isildurs-ranking-guide"
            .to_string(),
        source_history_label: "CF Tir-Talath factual topology/mechanics (Isildur's Ranking \
            Guide, evil-aligned table; thief-guild-path helpfile 2167; ranger class page 'south \
            of Tir-Talath' practice grounds) only; name/flavor phrasing original."
            .to_string(),
        summary: "The evil-leaning arcane interior city, northwest of Galadon on the world-graph \
            spine: an arcane tower district, militia patrols that make city stealth hard (#1618), \
            and an inn/recall welcoming evil-aligned characters."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 25,
        },
        rooms: vec![
            room(
                "room.tirtalath.gate",
                "Tir-Talath Gate",
                RoomKind::Gate,
                "The southeastern gate onto the interior road spine back toward Galadon.",
                RoomConcealment::GateChokepoint,
                &["npc.tirtalath.gate_militia"],
            ),
            room(
                "room.tirtalath.city_square",
                "Tir-Talath Square",
                RoomKind::Intersection,
                "A heavily patrolled, well-lit square; militia here readily detect the hidden \
                    (#1618 city counter-stealth).",
                RoomConcealment::None,
                &["npc.tirtalath.militia_patrol"],
            ),
            room(
                "room.tirtalath.tower_district",
                "Arcane Tower District",
                RoomKind::Academy,
                "The dark-arcane academy and tower district for evil-aligned spellcasters.",
                RoomConcealment::None,
                &["npc.tirtalath.academy_guildmaster"],
            ),
            room(
                "room.tirtalath.thug_guildhall",
                "Thug Guildhall",
                RoomKind::Guildhall,
                "A branch hall teaching Push; no membership required here.",
                RoomConcealment::AlleyShadow,
                &["npc.tirtalath.thug_path_trainer"],
            ),
            room(
                "room.tirtalath.mages_guild",
                "Mages' Guildhall",
                RoomKind::Guildhall,
                "The city's primary arcane guild.",
                RoomConcealment::None,
                &["npc.tirtalath.mage_guildmaster"],
            ),
            room(
                "room.tirtalath.militia_barracks",
                "Militia Barracks",
                RoomKind::Chamber,
                "The patrol post; militiamen here are 15-20 ranking targets for evil characters.",
                RoomConcealment::None,
                &["npc.tirtalath.militiaman"],
            ),
            room(
                "room.tirtalath.temple",
                "Tir-Talath Temple",
                RoomKind::Temple,
                "The altar recall point, welcoming evil-aligned characters.",
                RoomConcealment::None,
                &["npc.tirtalath.priest"],
            ),
            room(
                "room.tirtalath.inn",
                "Tir-Talath Inn",
                RoomKind::Inn,
                "An inn for evil-friendly characters passing through.",
                RoomConcealment::None,
                &["npc.tirtalath.innkeeper"],
            ),
            room(
                "room.tirtalath.cartographer",
                "Cartographer's Shop",
                RoomKind::Cartographer,
                "Sells region maps as acquired items; no free minimap is granted.",
                RoomConcealment::None,
                &["npc.tirtalath.cartographer"],
            ),
            room(
                "room.tirtalath.market",
                "Tir-Talath Market",
                RoomKind::Market,
                "Stalls of shopkeepers who are themselves 15-20 ranking targets for evil \
                    characters.",
                RoomConcealment::None,
                &["npc.tirtalath.shopkeeper"],
            ),
        ],
        connections: vec![
            internal(
                "conn.tirtalath.gate_to_square",
                "room.tirtalath.gate",
                TIR_TALATH_AREA_ID,
                "room.tirtalath.city_square",
                ConnectionDirection::North,
                TravelKind::Gate,
            ),
            internal(
                "conn.tirtalath.square_to_tower",
                "room.tirtalath.city_square",
                TIR_TALATH_AREA_ID,
                "room.tirtalath.tower_district",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.tirtalath.square_to_mages",
                "room.tirtalath.city_square",
                TIR_TALATH_AREA_ID,
                "room.tirtalath.mages_guild",
                ConnectionDirection::Northeast,
                TravelKind::Road,
            ),
            internal(
                "conn.tirtalath.square_to_militia",
                "room.tirtalath.city_square",
                TIR_TALATH_AREA_ID,
                "room.tirtalath.militia_barracks",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.tirtalath.square_to_temple",
                "room.tirtalath.city_square",
                TIR_TALATH_AREA_ID,
                "room.tirtalath.temple",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.tirtalath.square_to_inn",
                "room.tirtalath.city_square",
                TIR_TALATH_AREA_ID,
                "room.tirtalath.inn",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            internal(
                "conn.tirtalath.square_to_cartographer",
                "room.tirtalath.city_square",
                TIR_TALATH_AREA_ID,
                "room.tirtalath.cartographer",
                ConnectionDirection::Southwest,
                TravelKind::Road,
            ),
            internal(
                "conn.tirtalath.square_to_market",
                "room.tirtalath.city_square",
                TIR_TALATH_AREA_ID,
                "room.tirtalath.market",
                ConnectionDirection::Southeast,
                TravelKind::Road,
            ),
            // Hidden alley entrance to the thug branch (#944).
            hidden_internal(
                "conn.tirtalath.square_to_thugs",
                "room.tirtalath.city_square",
                TIR_TALATH_AREA_ID,
                "room.tirtalath.thug_guildhall",
                ConnectionDirection::Up,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            // External edges to the hub, Cragstone road, ranger practice woods south of
            // Tir-Talath, and a real sibling city in this batch (Darsylon, #1632).
            external(
                "conn.tirtalath.to_galadon",
                "room.tirtalath.gate",
                GALADON_AREA_ID,
                ConnectionDirection::Southeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.tirtalath.to_cragstone_road",
                "room.tirtalath.gate",
                CRAGSTONE_ROAD,
                ConnectionDirection::Northwest,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.tirtalath.to_ranger_woods_south",
                "room.tirtalath.city_square",
                RANGER_WOODS_SOUTH,
                ConnectionDirection::South,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.tirtalath.to_darsylon",
                "room.tirtalath.city_square",
                DARSYLON_AREA_ID,
                ConnectionDirection::Northwest,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.tirtalath.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.tirtalath.city_population".to_string(),
            baseline_population_budget: 30,
            repop_interval_minutes: 15,
            max_pressure_tier: 4,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.tirtalath.temple".to_string()),
            has_academy: true,
            // Explicit per issue: "inn/recall for evil-friendly characters" -- the mirror image
            // of Galadon's good/neutral gate (#1634).
            hometown_access: AreaAccessRule::Restricted {
                values: vec!["alignment.evil".to_string()],
            },
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.tirtalath.dark_arcane".to_string(),
            model_id: "model.city.tirtalath.tower_district".to_string(),
            required_props: vec![
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.guild_banner".to_string(),
                "prop.city.arcane_tower_spire".to_string(),
            ],
            silhouette: "dark-arcane interior city dominated by a tower district, militia \
                patrolling well-lit streets"
                .to_string(),
            material_notes: "Original Tir-Talath black stone and arcane-glow signage; no copied \
                iconography from the source MUD."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Topology is discoverable, not an omniscient minimap (#944): the thug branch alley \
                and the southern ranger-practice trail are Hidden/ConcealedTrail."
                .to_string(),
            "The city square is deliberately RoomConcealment::None (militia patrol density) to \
                showcase the #1618 easy-city-detection stealth profile named in the issue."
                .to_string(),
            "Cragstone road and the southern ranger woods are satellite areas authored by later \
                batch issues and referenced here as connections; Darsylon (#1643) is a real \
                sibling area authored in this same batch."
                .to_string(),
        ],
    }
}
