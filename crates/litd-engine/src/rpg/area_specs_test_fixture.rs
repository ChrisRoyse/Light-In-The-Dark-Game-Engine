use litd_engine::rpg::{
    AreaAccessRule, AreaConnection, AreaKind, AreaLevelBand, AreaRecallProfile, AreaRepopProfile,
    AreaResetProfile, AreaRoom, AreaSpec, AreaVisualContract, ConnectionDirection,
    ConnectionDiscovery, GALADON_AREA_ID, RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind,
    TravelKind,
};

// Satellite + world-graph area ids that Galadon connects out to (#1636 satellites,
// #1632 world topology). Their full AreaSpecs are authored by later batch issues;
// here they are referential targets for the connection topology.
pub const GALADON_GRAVEYARD_AREA_ID: &str = "area.thera.galadon.graveyard";
pub const GALADON_LUMBER_YARD_AREA_ID: &str = "area.thera.galadon.lumber_yard";
pub const GALADON_KOBOLD_WARRENS_AREA_ID: &str = "area.thera.galadon.kobold_warrens";
pub const GALADON_GOBLIN_VILLAGE_AREA_ID: &str = "area.thera.galadon.goblin_village";
pub const GALADON_COPEHAM_INN_AREA_ID: &str = "area.thera.galadon.copeham_inn";
pub const GALADON_OUTSKIRTS_AREA_ID: &str = "area.thera.galadon.outskirts";
pub const VALE_OF_ARENDYL_AREA_ID: &str = "area.thera.vale_of_arendyl";

/// The complete set of area ids Galadon references, for cross-area referential
/// integrity checks (there is no standalone area registry yet; see #987 report).
pub fn galadon_known_area_ids() -> [&'static str; 8] {
    [
        GALADON_AREA_ID,
        GALADON_GRAVEYARD_AREA_ID,
        GALADON_LUMBER_YARD_AREA_ID,
        GALADON_KOBOLD_WARRENS_AREA_ID,
        GALADON_GOBLIN_VILLAGE_AREA_ID,
        GALADON_COPEHAM_INN_AREA_ID,
        GALADON_OUTSKIRTS_AREA_ID,
        VALE_OF_ARENDYL_AREA_ID,
    ]
}

pub fn galadon_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: GALADON_AREA_ID.to_string(),
        display_name: "Galadon".to_string(),
        player_facing_canon_name: "Galadon".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1636,
        canon_issues: vec![987, 1598, 1632, 1636],
        mechanics_source_url: "https://wiki.carrionfields.net/area/galadon".to_string(),
        source_history_label: "CF Galadon hub-city factual topology and mechanics only".to_string(),
        summary: "The hub city at the center of Thera: four gates, a Large Intersection, guildhalls \
            for every class, a Tribunal law presence, cartographer, inn, a temple recall point, the \
            Academy entrance, and the thief-country sewers beneath."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 30,
        },
        rooms: vec![
            room("room.galadon.north_gate", "North Gate", RoomKind::Gate,
                "Northern chokepoint onto the Crossroads road; city guards check traffic.",
                RoomConcealment::GateChokepoint, &["npc.galadon.gate_guard"]),
            room("room.galadon.east_gate", "East Gate", RoomKind::Gate,
                "Eastern chokepoint onto the Eastern Road toward Aldevari and Arkham.",
                RoomConcealment::GateChokepoint, &["npc.galadon.gate_guard"]),
            room("room.galadon.south_gate", "South Gate", RoomKind::Gate,
                "Southern chokepoint toward the Rocky Cliffs and the desert road.",
                RoomConcealment::GateChokepoint, &["npc.galadon.gate_guard"]),
            room("room.galadon.northwest_gate", "Northwest Gate", RoomKind::Gate,
                "Northwest chokepoint onto the Holy Road toward the Vale of Arendyl.",
                RoomConcealment::GateChokepoint, &["npc.galadon.gate_guard"]),
            room("room.galadon.large_intersection", "The Large Intersection", RoomKind::Intersection,
                "The crowded meeting-heart of the city where every main street converges.",
                RoomConcealment::CrowdCover, &["npc.galadon.crier", "npc.galadon.pickpocket"]),
            room("room.galadon.holy_road", "Holy Road", RoomKind::Street,
                "The processional road running out the northwest gate toward the Vale.",
                RoomConcealment::None, &[]),
            room("room.galadon.market_row", "Market Row", RoomKind::Market,
                "Stalls and shopfronts thick with buyers, hawkers, and cutpurses.",
                RoomConcealment::CrowdCover, &["npc.galadon.merchant"]),
            room("room.galadon.warriors_guild", "Warriors' Guildhall", RoomKind::Guildhall,
                "Training hall and guildmaster for the martial classes.",
                RoomConcealment::None, &["npc.galadon.warrior_guildmaster"]),
            room("room.galadon.mages_guild", "Mages' Guildhall", RoomKind::Guildhall,
                "Spell-tutors and guildmaster for the arcane classes.",
                RoomConcealment::None, &["npc.galadon.mage_guildmaster"]),
            room("room.galadon.thieves_guild", "Thieves' Guildhall", RoomKind::Guildhall,
                "A shadowed backroom guild reached down a narrow alley.",
                RoomConcealment::AlleyShadow, &["npc.galadon.thief_guildmaster"]),
            room("room.galadon.tribunal_hall", "Tribunal Hall", RoomKind::Tribunal,
                "The seat of city law; crimes witnessed here draw enforcement.",
                RoomConcealment::None, &["npc.galadon.tribunal_magistrate"]),
            room("room.galadon.cartographer", "Cartographer's Shop", RoomKind::Cartographer,
                "Sells region maps as acquired items; no free minimap is granted.",
                RoomConcealment::None, &["npc.galadon.cartographer"]),
            room("room.galadon.travelers_inn", "Travelers' Inn", RoomKind::Inn,
                "Rooms, rumors, and a hearth for road-weary adventurers.",
                RoomConcealment::None, &["npc.galadon.innkeeper"]),
            room("room.galadon.grand_temple", "Grand Temple Altar", RoomKind::Temple,
                "The altar that serves as the city recall point.",
                RoomConcealment::None, &["npc.galadon.high_priest"]),
            room("room.galadon.academy_gate", "Academy Entrance", RoomKind::Academy,
                "The level 1-5 school entrance; adventurers are ejected at level 6.",
                RoomConcealment::None, &["npc.galadon.academy_guildmaster"]),
            room("room.galadon.sewer_underway", "Sewer Underway", RoomKind::Sewer,
                "The hidden layer beneath the streets; thief country, levels 5-30.",
                RoomConcealment::HiddenLayer, &["npc.galadon.sewer_rat", "npc.galadon.sewer_thug"]),
        ],
        connections: vec![
            // Internal wiring: gates funnel into the Large Intersection.
            internal("conn.galadon.north_gate_in", "room.galadon.north_gate",
                "room.galadon.large_intersection", ConnectionDirection::South, TravelKind::Gate),
            internal("conn.galadon.east_gate_in", "room.galadon.east_gate",
                "room.galadon.large_intersection", ConnectionDirection::West, TravelKind::Gate),
            internal("conn.galadon.south_gate_in", "room.galadon.south_gate",
                "room.galadon.large_intersection", ConnectionDirection::North, TravelKind::Gate),
            internal("conn.galadon.market_from_intersection", "room.galadon.large_intersection",
                "room.galadon.market_row", ConnectionDirection::East, TravelKind::Road),
            internal("conn.galadon.temple_from_intersection", "room.galadon.large_intersection",
                "room.galadon.grand_temple", ConnectionDirection::North, TravelKind::Road),
            internal("conn.galadon.holy_road_from_nw_gate", "room.galadon.northwest_gate",
                "room.galadon.holy_road", ConnectionDirection::Northwest, TravelKind::Road),
            // Hidden internal descent into the sewers (topology, not a free map, #944).
            hidden_internal("conn.galadon.sewer_grate", "room.galadon.large_intersection",
                "room.galadon.sewer_underway", ConnectionDirection::Down, TravelKind::Descend,
                ConnectionDiscovery::Hidden),
            // External edges to satellite areas (#1636) and the world graph (#1632).
            external("conn.galadon.to_lumber_yard", "room.galadon.south_gate",
                GALADON_LUMBER_YARD_AREA_ID, ConnectionDirection::South, TravelKind::Path,
                ConnectionDiscovery::Visible),
            external("conn.galadon.to_kobold_warrens", "room.galadon.northwest_gate",
                GALADON_KOBOLD_WARRENS_AREA_ID, ConnectionDirection::Northwest, TravelKind::Path,
                ConnectionDiscovery::Visible),
            external("conn.galadon.to_graveyard", "room.galadon.holy_road",
                GALADON_GRAVEYARD_AREA_ID, ConnectionDirection::West, TravelKind::Path,
                ConnectionDiscovery::Visible),
            external("conn.galadon.to_copeham_inn", "room.galadon.large_intersection",
                GALADON_COPEHAM_INN_AREA_ID, ConnectionDirection::West, TravelKind::Road,
                ConnectionDiscovery::Visible),
            external("conn.galadon.to_goblin_village", "room.galadon.south_gate",
                GALADON_GOBLIN_VILLAGE_AREA_ID, ConnectionDirection::Southeast,
                TravelKind::ConcealedTrail, ConnectionDiscovery::ConcealedTrail),
            external("conn.galadon.to_vale_of_arendyl", "room.galadon.holy_road",
                VALE_OF_ARENDYL_AREA_ID, ConnectionDirection::Northwest, TravelKind::Road,
                ConnectionDiscovery::Visible),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.galadon.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.galadon.city_population".to_string(),
            baseline_population_budget: 40,
            repop_interval_minutes: 15,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.galadon.grand_temple".to_string()),
            has_academy: true,
            // Evil races cannot make the good hub city their hometown (#1634 gating).
            hometown_access: AreaAccessRule::Restricted {
                values: vec!["alignment.good".to_string(), "alignment.neutral".to_string()],
            },
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.galadon.stone_streets".to_string(),
            model_id: "model.city.galadon.hub".to_string(),
            required_props: vec![
                "prop.city.gate_portcullis".to_string(),
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.sewer_grate".to_string(),
                "prop.city.guild_banner".to_string(),
            ],
            silhouette: "walled stone hub city radiating four gate-roads from a broad intersection"
                .to_string(),
            material_notes: "Original Thera-Galadon stonework, banners, and signage; no copied \
                iconography from the source MUD."
                .to_string(),
        },
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Topology is discoverable, not an omniscient minimap (#944): sewer descent is Hidden, \
                Goblin Village is a ConcealedTrail."
                .to_string(),
            "Recall point is the Grand Temple altar; Academy ejects at level 6 (#1634)."
                .to_string(),
            "Satellite areas (graveyard, lumber yard, kobold warrens, goblin village, copeham inn, \
                outskirts) are authored by later batch issues and referenced here as connections."
                .to_string(),
        ],
    }
}

fn room(
    room_id: &str,
    display_name: &str,
    room_kind: RoomKind,
    summary: &str,
    concealment: RoomConcealment,
    npcs: &[&str],
) -> AreaRoom {
    AreaRoom {
        room_id: room_id.to_string(),
        display_name: display_name.to_string(),
        room_kind,
        summary: summary.to_string(),
        concealment,
        npc_population_ids: npcs.iter().map(|id| id.to_string()).collect(),
    }
}

fn internal(
    connection_id: &str,
    from_room_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: GALADON_AREA_ID.to_string(),
        to_room_id: Some(to_room_id.to_string()),
        direction,
        travel_kind,
        discovery: ConnectionDiscovery::Visible,
        locked_by_ref: None,
    }
}

fn hidden_internal(
    connection_id: &str,
    from_room_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: GALADON_AREA_ID.to_string(),
        to_room_id: Some(to_room_id.to_string()),
        direction,
        travel_kind,
        discovery,
        locked_by_ref: None,
    }
}

fn external(
    connection_id: &str,
    from_room_id: &str,
    to_area_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: to_area_id.to_string(),
        to_room_id: None,
        direction,
        travel_kind,
        discovery,
        locked_by_ref: None,
    }
}
