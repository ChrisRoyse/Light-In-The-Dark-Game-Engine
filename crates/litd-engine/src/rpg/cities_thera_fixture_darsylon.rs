// #1643 Darsylon: the High Elf kingdom, far NW, ruled by the House of
// Darsylon. CF-factual topology from the real wiki.carrionfields.net Isildur's
// Ranking Guide (Darsylon: Shopkeepers/Guild Guards 25-30+, EVIL aligned
// characters table -- i.e. evil characters raid a good/neutral elven kingdom),
// helpfile 2167 (Darsylon hosts the LOCKSMITH path, Advanced Picklock first
// skill, no membership anywhere for this path), and the "Flow of Shadows"
// helpfile's reference to "the Honor Guard of Darsylon's royalty." Only
// names/flavor phrasing are original; the iron-taboo flavor and elven-sentry
// framing are as captured in the issue body (#984).
use litd_engine::rpg::{
    AreaAccessRule, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile, AreaResetProfile,
    AreaSpec, AreaVisualContract, ConnectionDirection, ConnectionDiscovery, RPG_AREA_SPEC_SCHEMA,
    RoomConcealment, RoomKind, TravelKind,
};

use super::rpg_cities_thera_fixture_support::{external, hidden_internal, internal, room};
use super::rpg_cities_thera_fixture_tirtalath::TIR_TALATH_AREA_ID;

pub(crate) const DARSYLON_AREA_ID: &str = "area.thera.darsylon";
const ELVEN_SENTRY_WOODS: &str = "area.thera.darsylon.elven_sentry_woods";

pub(crate) fn darsylon_known_area_ids() -> [&'static str; 3] {
    [DARSYLON_AREA_ID, ELVEN_SENTRY_WOODS, TIR_TALATH_AREA_ID]
}

pub(crate) fn darsylon_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: DARSYLON_AREA_ID.to_string(),
        display_name: "Darsylon".to_string(),
        player_facing_canon_name: "Darsylon".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1643,
        canon_issues: vec![987, 1598, 1632, 1643],
        mechanics_source_url: "https://wiki.carrionfields.net/guide/isildurs-ranking-guide"
            .to_string(),
        source_history_label: "CF Darsylon factual topology/mechanics (Isildur's Ranking Guide, \
            evil-aligned table; thief-guild-path helpfile 2167; 'Flow of Shadows' helpfile \
            reference to the Honor Guard of Darsylon's royalty) only; name/flavor phrasing \
            original."
            .to_string(),
        summary: "The High Elf kingdom in the far northwest, ruled by the House of Darsylon: a \
            canopy glade around the royal court, elven guilds including the locksmith path, and \
            forest approaches watched by elven sentries."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 35,
        },
        rooms: vec![
            room(
                "room.darsylon.sentry_gate",
                "Sentry Gate",
                RoomKind::Gate,
                "The forest gate where elven sentries watch the canopy road; iron-bearing \
                    visitors are noted here (iron-taboo lore).",
                RoomConcealment::GateChokepoint,
                &["npc.darsylon.elven_sentry"],
            ),
            room(
                "room.darsylon.canopy_glade",
                "Canopy Glade",
                RoomKind::Intersection,
                "The kingdom's heart, open beneath the forest canopy.",
                RoomConcealment::None,
                &["npc.darsylon.herald"],
            ),
            room(
                "room.darsylon.royal_court",
                "House of Darsylon Court",
                RoomKind::Tribunal,
                "The royal seat of the House of Darsylon.",
                RoomConcealment::None,
                &["npc.darsylon.royal_guard"],
            ),
            room(
                "room.darsylon.locksmith_guildhall",
                "Locksmith Guildhall",
                RoomKind::Guildhall,
                "Teaches Advanced Picklock; no membership required anywhere for this path.",
                RoomConcealment::AlleyShadow,
                &["npc.darsylon.locksmith_path_trainer"],
            ),
            room(
                "room.darsylon.mages_guild",
                "Mages' Guildhall",
                RoomKind::Guildhall,
                "Elven high-magic tutors.",
                RoomConcealment::None,
                &["npc.darsylon.mage_guildmaster"],
            ),
            room(
                "room.darsylon.temple",
                "Darsylon Temple",
                RoomKind::Temple,
                "The altar that serves as the elf hometown recall point.",
                RoomConcealment::None,
                &["npc.darsylon.priest"],
            ),
            room(
                "room.darsylon.inn",
                "Darsylon Inn",
                RoomKind::Inn,
                "Rest beneath the canopy for travelers of the kingdom.",
                RoomConcealment::None,
                &["npc.darsylon.innkeeper"],
            ),
            room(
                "room.darsylon.cartographer",
                "Cartographer's Shop",
                RoomKind::Cartographer,
                "Sells region maps as acquired items; no free minimap is granted.",
                RoomConcealment::None,
                &["npc.darsylon.cartographer"],
            ),
            room(
                "room.darsylon.market",
                "Darsylon Market",
                RoomKind::Market,
                "Shopkeepers and guild guards who are 25-30+ ranking targets for evil raiders; \
                    no iron vendors here (iron-taboo lore).",
                RoomConcealment::None,
                &["npc.darsylon.shopkeeper"],
            ),
        ],
        connections: vec![
            internal(
                "conn.darsylon.gate_to_glade",
                "room.darsylon.sentry_gate",
                DARSYLON_AREA_ID,
                "room.darsylon.canopy_glade",
                ConnectionDirection::Southeast,
                TravelKind::Gate,
            ),
            internal(
                "conn.darsylon.glade_to_court",
                "room.darsylon.canopy_glade",
                DARSYLON_AREA_ID,
                "room.darsylon.royal_court",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.darsylon.glade_to_mages",
                "room.darsylon.canopy_glade",
                DARSYLON_AREA_ID,
                "room.darsylon.mages_guild",
                ConnectionDirection::Northeast,
                TravelKind::Road,
            ),
            internal(
                "conn.darsylon.glade_to_temple",
                "room.darsylon.canopy_glade",
                DARSYLON_AREA_ID,
                "room.darsylon.temple",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.darsylon.glade_to_inn",
                "room.darsylon.canopy_glade",
                DARSYLON_AREA_ID,
                "room.darsylon.inn",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            internal(
                "conn.darsylon.glade_to_cartographer",
                "room.darsylon.canopy_glade",
                DARSYLON_AREA_ID,
                "room.darsylon.cartographer",
                ConnectionDirection::Southwest,
                TravelKind::Road,
            ),
            internal(
                "conn.darsylon.glade_to_market",
                "room.darsylon.canopy_glade",
                DARSYLON_AREA_ID,
                "room.darsylon.market",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            // Hidden alley entrance to the locksmith guild (#944).
            hidden_internal(
                "conn.darsylon.glade_to_locksmiths",
                "room.darsylon.canopy_glade",
                DARSYLON_AREA_ID,
                "room.darsylon.locksmith_guildhall",
                ConnectionDirection::Up,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            // External edges: forest approach with elven sentries (concealed, permanent-sneak
            // natives per #1603), and a real sibling city in this batch (Tir-Talath, #1632).
            external(
                "conn.darsylon.to_sentry_woods",
                "room.darsylon.sentry_gate",
                ELVEN_SENTRY_WOODS,
                ConnectionDirection::Northwest,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.darsylon.to_tir_talath",
                "room.darsylon.sentry_gate",
                TIR_TALATH_AREA_ID,
                ConnectionDirection::Southeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.darsylon.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.darsylon.city_population".to_string(),
            baseline_population_budget: 30,
            repop_interval_minutes: 15,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.darsylon.temple".to_string()),
            has_academy: true,
            // Explicit per issue: elves are raided BY evil characters ("raiding elves is an
            // evil pastime"), i.e. Darsylon is a good/neutral elf hometown, mirroring Galadon's
            // gate (#1634).
            hometown_access: AreaAccessRule::Restricted {
                values: vec![
                    "alignment.good".to_string(),
                    "alignment.neutral".to_string(),
                ],
            },
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.darsylon.canopy_kingdom".to_string(),
            model_id: "model.city.darsylon.royal_glade".to_string(),
            required_props: vec![
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.guild_banner".to_string(),
                "prop.city.canopy_arch".to_string(),
            ],
            silhouette: "elven canopy kingdom built around an open glade court, forest on every \
                side watched by sentries"
                .to_string(),
            material_notes: "Original Darsylon living-wood architecture; no iron fittings \
                (iron-taboo lore); no copied iconography from the source MUD."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Topology is discoverable, not an omniscient minimap (#944): the locksmith guild \
                alley and the sentry-woods approach are Hidden/ConcealedTrail."
                .to_string(),
            "Elven sentry woods showcase the #1603 permanent-sneak-native / camouflage-legal \
                terrain design; attacking the kingdom as an evil character means beating elite \
                wilderness perception, not a free walk-in."
                .to_string(),
            "The elven sentry woods are a satellite area authored by a later batch issue and \
                referenced here as a connection; Tir-Talath (#1642) is a real sibling area \
                authored in this same batch."
                .to_string(),
        ],
    }
}
