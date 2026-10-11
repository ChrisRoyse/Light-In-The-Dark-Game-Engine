// #1638 Hamsah Mu-tazz: the southern desert port city. CF-factual topology from
// the real wiki.carrionfields.net Isildur's Ranking Guide, helpfile 2167 (Hamsah
// Mu'Tazz is the BINDER guild HQ: Nimble Fingers first skill, membership
// required, kingpin), and helpfile 2523 (NEWBIE3): "The Seaport of Hamsah
// Mu'tazz: Head straight east of the Southern Market Square of Hamsah Mu'tazz,
// about eleven paces." Only names/flavor phrasing are original (#984).
use litd_engine::rpg::{
    AreaAccessRule, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile, AreaResetProfile,
    AreaSpec, AreaVisualContract, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID,
    RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind, TravelKind,
};

use super::rpg_cities_thera_fixture_seantryn::SEANTRYN_MODAN_AREA_ID;
use super::rpg_cities_thera_fixture_support::{external, hidden_internal, internal, room};

pub(crate) const HAMSAH_AREA_ID: &str = "area.thera.hamsah_mu_tazz";
// #1704 registry fix: the southern desert areas are authored top-level by the
// #1653 region batch (Sands of Sorrow / Desert of Araile / Oryx Steppes), not as
// city-namespaced sub-areas. Point the edges at the real authored ids.
const SANDS_OF_SORROW: &str = "area.thera.sands_of_sorrow";
const DESERT_OF_ARAILE: &str = "area.thera.desert_of_araile";
const ORYX_STEPPES: &str = "area.thera.oryx_steppes";

pub(crate) fn hamsah_known_area_ids() -> [&'static str; 6] {
    [
        HAMSAH_AREA_ID,
        GALADON_AREA_ID,
        SANDS_OF_SORROW,
        DESERT_OF_ARAILE,
        ORYX_STEPPES,
        SEANTRYN_MODAN_AREA_ID,
    ]
}

pub(crate) fn hamsah_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: HAMSAH_AREA_ID.to_string(),
        display_name: "Hamsah Mu-tazz".to_string(),
        player_facing_canon_name: "Hamsah Mu-tazz".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1638,
        canon_issues: vec![987, 1598, 1632, 1638],
        mechanics_source_url: "https://wiki.carrionfields.net/guide/isildurs-ranking-guide"
            .to_string(),
        source_history_label: "CF Hamsah Mu-tazz factual topology/mechanics (Isildur's Ranking \
            Guide; thief-guild-path helpfile 2167; NEWBIE3 helpfile 2523 Seaport directions) \
            only; name/flavor phrasing original."
            .to_string(),
        summary: "The southern desert metropolis: a walled souk city built around its Southern \
            Market Square, with the newbie docks of the Seaport eleven paces east, the binder \
            thieves' guild HQ, and caravan roads out into the dunes."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 25,
        },
        rooms: vec![
            room(
                "room.hamsah.desert_gate",
                "Desert Gate",
                RoomKind::Gate,
                "The walled city's gate onto the caravan roads through the dunes.",
                RoomConcealment::GateChokepoint,
                &["npc.hamsah.gate_guard"],
            ),
            room(
                "room.hamsah.southern_market",
                "Southern Market Square",
                RoomKind::Market,
                "The souk at the city's center, thick with traders and buyers.",
                RoomConcealment::CrowdCover,
                &["npc.hamsah.merchant"],
            ),
            room(
                "room.hamsah.seaport_road",
                "Seaport Road",
                RoomKind::Street,
                "The road running east from the Southern Market Square toward the docks.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.hamsah.seaport",
                "The Seaport",
                RoomKind::Landmark,
                "Newbie docks about eleven paces east of the market; levels 1-10.",
                RoomConcealment::None,
                &["npc.hamsah.dockhand"],
            ),
            room(
                "room.hamsah.binder_guildhall",
                "Binder Guildhall",
                RoomKind::Guildhall,
                "The kidnap-flavored binder guild's hall; membership required, led by a kingpin.",
                RoomConcealment::AlleyShadow,
                &["npc.hamsah.binder_kingpin"],
            ),
            room(
                "room.hamsah.warriors_guild",
                "Warriors' Guildhall",
                RoomKind::Guildhall,
                "Training hall for the martial classes.",
                RoomConcealment::None,
                &["npc.hamsah.warrior_guildmaster"],
            ),
            room(
                "room.hamsah.mages_guild",
                "Mages' Guildhall",
                RoomKind::Guildhall,
                "Spell-tutors for the arcane classes.",
                RoomConcealment::None,
                &["npc.hamsah.mage_guildmaster"],
            ),
            room(
                "room.hamsah.temple",
                "Hamsah Temple",
                RoomKind::Temple,
                "The altar that serves as the city recall point.",
                RoomConcealment::None,
                &["npc.hamsah.priest"],
            ),
            room(
                "room.hamsah.inn",
                "Hamsah Inn",
                RoomKind::Inn,
                "Shade and a hearth for road-weary desert travelers.",
                RoomConcealment::None,
                &["npc.hamsah.innkeeper"],
            ),
            room(
                "room.hamsah.cartographer",
                "Cartographer's Shop",
                RoomKind::Cartographer,
                "Sells region maps as acquired items; no free minimap is granted.",
                RoomConcealment::None,
                &["npc.hamsah.cartographer"],
            ),
            room(
                "room.hamsah.academy",
                "Hamsah Academy",
                RoomKind::Academy,
                "The level 1-5 school entrance for characters hometowned here.",
                RoomConcealment::None,
                &["npc.hamsah.academy_guildmaster"],
            ),
            room(
                "room.hamsah.caravan_yard",
                "Caravan Yard",
                RoomKind::Landmark,
                "Trade caravans marshal here before setting out into the dunes.",
                RoomConcealment::None,
                &["npc.hamsah.caravan_master"],
            ),
        ],
        connections: vec![
            internal(
                "conn.hamsah.gate_to_market",
                "room.hamsah.desert_gate",
                HAMSAH_AREA_ID,
                "room.hamsah.southern_market",
                ConnectionDirection::North,
                TravelKind::Gate,
            ),
            internal(
                "conn.hamsah.market_to_seaport_road",
                "room.hamsah.southern_market",
                HAMSAH_AREA_ID,
                "room.hamsah.seaport_road",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.hamsah.seaport_road_to_port",
                "room.hamsah.seaport_road",
                HAMSAH_AREA_ID,
                "room.hamsah.seaport",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.hamsah.market_to_warriors",
                "room.hamsah.southern_market",
                HAMSAH_AREA_ID,
                "room.hamsah.warriors_guild",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.hamsah.market_to_mages",
                "room.hamsah.southern_market",
                HAMSAH_AREA_ID,
                "room.hamsah.mages_guild",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.hamsah.market_to_temple",
                "room.hamsah.southern_market",
                HAMSAH_AREA_ID,
                "room.hamsah.temple",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            internal(
                "conn.hamsah.market_to_inn",
                "room.hamsah.southern_market",
                HAMSAH_AREA_ID,
                "room.hamsah.inn",
                ConnectionDirection::Southwest,
                TravelKind::Road,
            ),
            internal(
                "conn.hamsah.market_to_cartographer",
                "room.hamsah.southern_market",
                HAMSAH_AREA_ID,
                "room.hamsah.cartographer",
                ConnectionDirection::Southeast,
                TravelKind::Road,
            ),
            internal(
                "conn.hamsah.market_to_academy",
                "room.hamsah.southern_market",
                HAMSAH_AREA_ID,
                "room.hamsah.academy",
                ConnectionDirection::Northwest,
                TravelKind::Road,
            ),
            internal(
                "conn.hamsah.gate_to_caravan_yard",
                "room.hamsah.desert_gate",
                HAMSAH_AREA_ID,
                "room.hamsah.caravan_yard",
                ConnectionDirection::Out,
                TravelKind::Road,
            ),
            // Hidden alley entrance to the binder guild (#944).
            hidden_internal(
                "conn.hamsah.market_to_binders",
                "room.hamsah.southern_market",
                HAMSAH_AREA_ID,
                "room.hamsah.binder_guildhall",
                ConnectionDirection::Northeast,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            // External edges to the hub, desert approaches, and a sibling city (#1632).
            external(
                "conn.hamsah.to_galadon",
                "room.hamsah.desert_gate",
                GALADON_AREA_ID,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.hamsah.to_sands_of_sorrow",
                "room.hamsah.desert_gate",
                SANDS_OF_SORROW,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.hamsah.to_desert_of_araile",
                "room.hamsah.desert_gate",
                DESERT_OF_ARAILE,
                ConnectionDirection::Southwest,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.hamsah.to_oryx_steppes",
                "room.hamsah.caravan_yard",
                ORYX_STEPPES,
                ConnectionDirection::West,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.hamsah.to_seantryn_modan",
                "room.hamsah.caravan_yard",
                SEANTRYN_MODAN_AREA_ID,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.hamsah.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.hamsah.city_population".to_string(),
            baseline_population_budget: 35,
            repop_interval_minutes: 15,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.hamsah.temple".to_string()),
            has_academy: true,
            // Documented (help newbie) alongside Galadon and Voralian as an overflow start
            // city; treated the same as Galadon's good/neutral gate (#1634).
            hometown_access: AreaAccessRule::Restricted {
                values: vec![
                    "alignment.good".to_string(),
                    "alignment.neutral".to_string(),
                ],
            },
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.hamsah.desert_walls".to_string(),
            model_id: "model.city.hamsah.souk_port".to_string(),
            required_props: vec![
                "prop.city.gate_portcullis".to_string(),
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.dock_crane".to_string(),
            ],
            silhouette: "sunbaked walled desert port city ringed by dunes, souk at its heart, \
                docks a short walk east"
                .to_string(),
            material_notes: "Original Hamsah sandstone, awnings, and signage; no copied \
                iconography from the source MUD."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Topology is discoverable, not an omniscient minimap (#944): the binder guild's \
                alley entrance and the Oryx Steppes trail are Hidden/ConcealedTrail."
                .to_string(),
            "Sands of Sorrow, Desert of Araile, and the Oryx Steppes (bedouin ranger homeland) \
                are satellite areas authored by later batch issues and referenced here as \
                connections."
                .to_string(),
            "Sandstorm weather masking (#848 ties) and heat-shimmer open-desert exposure are \
                deferred to the terrain/weather system, not modeled as AreaSpec fields here."
                .to_string(),
        ],
    }
}
