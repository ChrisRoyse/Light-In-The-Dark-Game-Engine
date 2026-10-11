// #1640 Seantryn Modan: the far-southeast coastal capital. CF-factual topology
// from the issue-captured facts (Harbor district 1-51; Sewers of Seantryn
// Modan thief layer) plus helpfile 2167 (Seantryn Modan hosts a BINDER branch,
// Nimble Fingers first skill, no membership -- Hamsah Mu'tazz is the binder HQ)
// and helpfile 2523 (NEWBIE3): "Caravans of the Jhaana: From the Center of
// Seantryn Plaza, travel five paces east to Copperhead Road. Take three paces
// south, then travel east along Windflower Road. Continue east, traveling
// outside of Seantryn Modan. A small trail off of the main road will guide you
// to the caravans." Only names/flavor phrasing are original (#984).
use litd_engine::rpg::{
    AreaAccessRule, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile, AreaResetProfile,
    AreaSpec, AreaVisualContract, ConnectionDirection, ConnectionDiscovery, RPG_AREA_SPEC_SCHEMA,
    RoomConcealment, RoomKind, TravelKind,
};

use super::rpg_cities_thera_fixture_hamsah::HAMSAH_AREA_ID;
use super::rpg_cities_thera_fixture_support::{external, hidden_internal, internal, room};

pub(crate) const SEANTRYN_MODAN_AREA_ID: &str = "area.thera.seantryn_modan";
// #1704 registry fix: the Seantryn wetlands are authored top-level by the #1654
// wetlands region batch. The Caravans of the Jhaana, Q'Habiszan, and the Felar
// Encampment remain deferred anchors (tracked in the registry's known-deferred
// ledger).
const CARAVANS_OF_THE_JHAANA: &str = "area.thera.seantryn_modan.caravans_of_the_jhaana";
const WETLANDS: &str = "area.thera.seantryn_wetlands";
const QHABISZAN: &str = "area.thera.seantryn_modan.qhabiszan";
const FELAR_ENCAMPMENT: &str = "area.thera.seantryn_modan.felar_encampment";

pub(crate) fn seantryn_known_area_ids() -> [&'static str; 6] {
    [
        SEANTRYN_MODAN_AREA_ID,
        CARAVANS_OF_THE_JHAANA,
        WETLANDS,
        QHABISZAN,
        FELAR_ENCAMPMENT,
        HAMSAH_AREA_ID,
    ]
}

pub(crate) fn seantryn_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: SEANTRYN_MODAN_AREA_ID.to_string(),
        display_name: "Seantryn Modan".to_string(),
        player_facing_canon_name: "Seantryn Modan".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1640,
        canon_issues: vec![987, 1598, 1632, 1640],
        mechanics_source_url: "https://wiki.carrionfields.net/guide/isildurs-ranking-guide"
            .to_string(),
        source_history_label: "CF Seantryn Modan factual topology/mechanics (issue-captured \
            harbor/sewer facts; thief-guild-path helpfile 2167; NEWBIE3 helpfile 2523 Caravans \
            of the Jhaana directions) only; name/flavor phrasing original."
            .to_string(),
        summary: "The far-southeast coastal capital, plaza-centered around the Center of Seantryn \
            Plaza with Copperhead Road and Windflower Road, a binder thieves' guild branch, a \
            harbor district, and a thief-country sewer layer beneath."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 51,
        },
        rooms: vec![
            room(
                "room.seantryn.plaza",
                "Center of Seantryn Plaza",
                RoomKind::Intersection,
                "The plaza heart of the city, where Copperhead Road begins.",
                RoomConcealment::CrowdCover,
                &["npc.seantryn.crier"],
            ),
            room(
                "room.seantryn.copperhead_road",
                "Copperhead Road",
                RoomKind::Street,
                "Runs east from the plaza toward Windflower Road.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.seantryn.windflower_road",
                "Windflower Road",
                RoomKind::Street,
                "Runs east out of the city, past the trail to the Jhaana caravans.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.seantryn.harbor",
                "Seantryn Harbor",
                RoomKind::Landmark,
                "The coastal port district; ranking content spans the full level range here.",
                RoomConcealment::None,
                &["npc.seantryn.dockhand"],
            ),
            room(
                "room.seantryn.sewer_underway",
                "Sewers of Seantryn Modan",
                RoomKind::Sewer,
                "The hidden thief-country layer beneath the plaza.",
                RoomConcealment::HiddenLayer,
                &["npc.seantryn.sewer_thug"],
            ),
            room(
                "room.seantryn.binder_guildhall",
                "Binder Guildhall",
                RoomKind::Guildhall,
                "A branch hall teaching Nimble Fingers; no membership required here.",
                RoomConcealment::AlleyShadow,
                &["npc.seantryn.binder_path_trainer"],
            ),
            room(
                "room.seantryn.warriors_guild",
                "Warriors' Guildhall",
                RoomKind::Guildhall,
                "Training hall for the martial classes.",
                RoomConcealment::None,
                &["npc.seantryn.warrior_guildmaster"],
            ),
            room(
                "room.seantryn.mages_guild",
                "Mages' Guildhall",
                RoomKind::Guildhall,
                "Spell-tutors for the arcane classes.",
                RoomConcealment::None,
                &["npc.seantryn.mage_guildmaster"],
            ),
            room(
                "room.seantryn.temple",
                "Seantryn Temple",
                RoomKind::Temple,
                "The altar that serves as the city recall point.",
                RoomConcealment::None,
                &["npc.seantryn.priest"],
            ),
            room(
                "room.seantryn.inn",
                "Seantryn Inn",
                RoomKind::Inn,
                "Rooms with a sea breeze for travelers.",
                RoomConcealment::None,
                &["npc.seantryn.innkeeper"],
            ),
            room(
                "room.seantryn.cartographer",
                "Cartographer's Shop",
                RoomKind::Cartographer,
                "Sells region maps as acquired items; no free minimap is granted.",
                RoomConcealment::None,
                &["npc.seantryn.cartographer"],
            ),
            room(
                "room.seantryn.market",
                "Seantryn Market",
                RoomKind::Market,
                "Stalls of coastal traders and fishmongers.",
                RoomConcealment::CrowdCover,
                &["npc.seantryn.merchant"],
            ),
        ],
        connections: vec![
            internal(
                "conn.seantryn.plaza_to_copperhead",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.copperhead_road",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.seantryn.copperhead_to_windflower",
                "room.seantryn.copperhead_road",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.windflower_road",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            internal(
                "conn.seantryn.plaza_to_harbor",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.harbor",
                ConnectionDirection::Southeast,
                TravelKind::Road,
            ),
            internal(
                "conn.seantryn.plaza_to_warriors",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.warriors_guild",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.seantryn.plaza_to_mages",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.mages_guild",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.seantryn.plaza_to_temple",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.temple",
                ConnectionDirection::Northwest,
                TravelKind::Road,
            ),
            internal(
                "conn.seantryn.plaza_to_inn",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.inn",
                ConnectionDirection::Southwest,
                TravelKind::Road,
            ),
            internal(
                "conn.seantryn.plaza_to_cartographer",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.cartographer",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            internal(
                "conn.seantryn.plaza_to_market",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.market",
                ConnectionDirection::Southeast,
                TravelKind::Road,
            ),
            // Hidden sewer descent and hidden alley guild entrance (#944).
            hidden_internal(
                "conn.seantryn.plaza_to_sewers",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.sewer_underway",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            hidden_internal(
                "conn.seantryn.plaza_to_binders",
                "room.seantryn.plaza",
                SEANTRYN_MODAN_AREA_ID,
                "room.seantryn.binder_guildhall",
                ConnectionDirection::Up,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            // External edges: caravans (concealed trail, per NEWBIE3), wetlands, satellites,
            // and the road link west toward Hamsah Mu-tazz (#1632).
            external(
                "conn.seantryn.to_caravans",
                "room.seantryn.windflower_road",
                CARAVANS_OF_THE_JHAANA,
                ConnectionDirection::East,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.seantryn.to_wetlands",
                "room.seantryn.harbor",
                WETLANDS,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.seantryn.to_qhabiszan",
                "room.seantryn.plaza",
                QHABISZAN,
                ConnectionDirection::Southeast,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.seantryn.to_felar_encampment",
                "room.seantryn.plaza",
                FELAR_ENCAMPMENT,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.seantryn.to_hamsah",
                "room.seantryn.copperhead_road",
                HAMSAH_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.seantryn.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.seantryn.city_population".to_string(),
            baseline_population_budget: 40,
            repop_interval_minutes: 15,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.seantryn.temple".to_string()),
            has_academy: false,
            // No explicit alignment gate or newbie-start-academy documentation for Seantryn
            // Modan; treated as an open hometown without a dedicated academy (judgment call).
            hometown_access: AreaAccessRule::Any,
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.seantryn.coastal_plaza".to_string(),
            model_id: "model.city.seantryn.harbor_capital".to_string(),
            required_props: vec![
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.sewer_grate".to_string(),
                "prop.city.dock_crane".to_string(),
            ],
            silhouette: "plaza-centered coastal capital with a harbor district to the southeast \
                and sewer grates beneath the main plaza"
                .to_string(),
            material_notes: "Original Seantryn coral-stone and rope-work; no copied iconography \
                from the source MUD."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Topology is discoverable, not an omniscient minimap (#944): the sewer descent, the \
                binder guild entrance, the Jhaana caravan trail, and the Qhabiszan approach are \
                Hidden/ConcealedTrail."
                .to_string(),
            "Caravans of the Jhaana, the Wetlands, Qhabiszan, and the Felar Encampment are \
                satellite areas authored by later batch issues and referenced here as \
                connections."
                .to_string(),
            "Harbor fog and wetland night-hide stealth design notes are deferred to the \
                terrain/weather system, not modeled as AreaSpec fields here."
                .to_string(),
        ],
    }
}
