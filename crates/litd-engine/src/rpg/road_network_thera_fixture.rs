// Authored #1633 road-network fixture for Thera. It is DATA over the authored
// area corpus: every road waypoint, gate location, and crossroad area id is
// validated against the #1704 registry by the sibling test module. Ground truth
// is the factual CF road placements in help id 2523 (North Road, Eastern Road,
// Holy Road, Silverpine Road, the southern desert road out of Galadon) plus the
// authored area corpus. The #1632 overworld topology is its sibling in
// `overworld_topology_thera_fixture.rs`.

use crate::rpg::{
    Crossroad, NamedRoad, RPG_ROAD_NETWORK_SCHEMA, RoadExposure, RoadGate, RoadKind,
    RoadNetworkSpec, THERA_WORLD_ID,
};

/// The authored Thera named-road spine: the six factual CF roads out of the hub
/// cities (help id 2523), their gate chokepoints (Galadon's four Tribunal-law
/// gates), and the crossroad landmarks players navigate by.
pub fn thera_road_network_spec() -> RoadNetworkSpec {
    RoadNetworkSpec {
        schema: RPG_ROAD_NETWORK_SCHEMA.to_string(),
        world_id: THERA_WORLD_ID.to_string(),
        display_name: "Thera Road Spine".to_string(),
        source_issue: 1633,
        canon_issues: vec![987, 1598, 1629, 1633, 1704],
        mechanics_source_url: "https://wiki.carrionfields.net/game/world".to_string(),
        summary: "The named roads of Thera as data over the area graph: ordered waypoint areas, \
            Galadon's guarded Tribunal gates, and the crossroad landmarks where roads meet. Roads \
            are fast exposed terrain (#1629) with weak concealment."
            .to_string(),
        roads: vec![
            road(
                "road.thera.north_road",
                "The North Road",
                RoadKind::Road,
                RoadExposure::Exposed,
                "The north-south spine from Galadon's north gate to Udgaard.",
                &[
                    "area.thera.galadon",
                    "area.thera.inn_of_the_eternal_star",
                    "area.thera.udgaard",
                ],
                vec![gate(
                    "gate.galadon.north",
                    "area.thera.galadon",
                    true,
                    true,
                    "Galadon's north gate: guarded Tribunal law zone onto the North Road.",
                )],
            ),
            road(
                "road.thera.eastern_road",
                "The Eastern Road",
                RoadKind::Road,
                RoadExposure::Exposed,
                "East from Galadon past Aldevari to Arkham.",
                &[
                    "area.thera.galadon",
                    "area.thera.aldevari",
                    "area.thera.arkham",
                ],
                vec![gate(
                    "gate.galadon.east",
                    "area.thera.galadon",
                    true,
                    true,
                    "Galadon's east gate: guarded Tribunal law zone onto the Eastern Road.",
                )],
            ),
            road(
                "road.thera.holy_road",
                "The Holy Road",
                RoadKind::HolyRoad,
                RoadExposure::Sheltered,
                "The processional road out the northwest gate to the Vale of Arendyl.",
                &["area.thera.galadon", "area.thera.vale_of_arendyl"],
                vec![gate(
                    "gate.galadon.northwest",
                    "area.thera.galadon",
                    true,
                    true,
                    "Galadon's northwest gate: guarded Tribunal law zone onto the Holy Road.",
                )],
            ),
            road(
                "road.thera.southern_desert_road",
                "The Southern Desert Road",
                RoadKind::Road,
                RoadExposure::Exposed,
                "South from Galadon to Hamsah Mu'tazz and the twin deserts.",
                &[
                    "area.thera.galadon",
                    "area.thera.hamsah_mu_tazz",
                    "area.thera.sands_of_sorrow",
                    "area.thera.desert_of_araile",
                    "area.thera.oryx_steppes",
                ],
                vec![
                    gate(
                        "gate.galadon.south",
                        "area.thera.galadon",
                        true,
                        true,
                        "Galadon's south gate: guarded Tribunal law zone onto the desert road.",
                    ),
                    gate(
                        "gate.hamsah.market",
                        "area.thera.hamsah_mu_tazz",
                        true,
                        true,
                        "Hamsah Mu'tazz's Southern Market gate onto the desert road.",
                    ),
                ],
            ),
            road(
                "road.thera.silverpine_road",
                "Silverpine Road",
                RoadKind::Road,
                RoadExposure::Exposed,
                "East out of Voralian City until it ends at Loch Grynmear.",
                &["area.thera.voralian", "area.thera.loch_grynmear"],
                vec![gate(
                    "gate.voralian.east",
                    "area.thera.voralian",
                    true,
                    true,
                    "Voralian City's east gate onto Silverpine Road.",
                )],
            ),
            road(
                "road.thera.tir_talath_pass",
                "The Tir-Talath Pass",
                RoadKind::Pass,
                RoadExposure::Chokepoint,
                "The mountain pass linking Hillcrest to Cragstone in the northwest interior.",
                &["area.thera.hillcrest", "area.thera.cragstone"],
                vec![gate(
                    "gate.pass.hillcrest",
                    "area.thera.hillcrest",
                    false,
                    true,
                    "The guarded watchpost commanding the Hillcrest end of the pass.",
                )],
            ),
        ],
        crossroads: vec![
            crossroad(
                "crossroad.thera.galadon_large_intersection",
                "The Large Intersection",
                "area.thera.galadon",
                &[
                    "road.thera.north_road",
                    "road.thera.eastern_road",
                    "road.thera.holy_road",
                    "road.thera.southern_desert_road",
                ],
                "Galadon's crowded meeting-heart where every main road converges.",
            ),
            crossroad(
                "crossroad.thera.north_sutherspring",
                "North Sutherspring Crossroads",
                "area.thera.inn_of_the_eternal_star",
                &["road.thera.north_road", "road.thera.holy_road"],
                "The northern crossroads by the Inn of the Eternal Star toward the Vale.",
            ),
        ],
        implementation_notes: vec![
            "Every waypoint, gate, and crossroad area is validated against the #1704 registry."
                .to_string(),
            "Roads are exposed fast-travel terrain (#1629); Galadon's four gates are guarded \
                Tribunal law zones. Roads are cartographer-sellable as acquired maps, never a free \
                GPS (#944)."
                .to_string(),
        ],
    }
}

fn road(
    road_id: &str,
    display_name: &str,
    road_kind: RoadKind,
    exposure: RoadExposure,
    summary: &str,
    waypoint_area_ids: &[&str],
    gates: Vec<RoadGate>,
) -> NamedRoad {
    NamedRoad {
        road_id: road_id.to_string(),
        display_name: display_name.to_string(),
        road_kind,
        exposure,
        summary: summary.to_string(),
        waypoint_area_ids: waypoint_area_ids.iter().map(|s| s.to_string()).collect(),
        gates,
    }
}

fn gate(
    gate_id: &str,
    at_area_id: &str,
    is_law_zone: bool,
    is_guarded: bool,
    summary: &str,
) -> RoadGate {
    RoadGate {
        gate_id: gate_id.to_string(),
        at_area_id: at_area_id.to_string(),
        is_law_zone,
        is_guarded,
        summary: summary.to_string(),
    }
}

fn crossroad(
    crossroad_id: &str,
    display_name: &str,
    at_area_id: &str,
    road_ids: &[&str],
    summary: &str,
) -> Crossroad {
    Crossroad {
        crossroad_id: crossroad_id.to_string(),
        display_name: display_name.to_string(),
        at_area_id: at_area_id.to_string(),
        road_ids: road_ids.iter().map(|s| s.to_string()).collect(),
        summary: summary.to_string(),
    }
}
