// Authored #1632 overworld topology fixture for Thera. It is DATA over the
// authored area corpus: every region member area id is validated against the
// #1704 registry by the sibling test module, so the topology cannot drift from
// the authored areas it summarizes. The #1633 road-network fixture is its
// sibling in `road_network_thera_fixture.rs`.
//
// Ground truth is the authored AreaSpec corpus itself (the CF wiki ASCII world
// map renders garbled, per the cf-source skill) plus the factual road placements
// in CF help id 2523.

use crate::rpg::{
    AreaLevelBand, OverworldTopologySpec, RPG_OVERWORLD_TOPOLOGY_SCHEMA, RegionEdge, RegionSpine,
    THERA_WORLD_ID, TopologyRegion, WorldSector,
};

/// The authored Thera continental topology: ten regions across the eight world
/// sectors, partitioning the major authored areas, wired by a twelve-edge
/// region spine (roads, seas, the Underdark descent, and the Past portal).
pub fn thera_overworld_topology_spec() -> OverworldTopologySpec {
    OverworldTopologySpec {
        schema: RPG_OVERWORLD_TOPOLOGY_SCHEMA.to_string(),
        world_id: THERA_WORLD_ID.to_string(),
        display_name: "Thera".to_string(),
        source_issue: 1632,
        canon_issues: vec![987, 1598, 1632, 1704],
        mechanics_source_url: "https://wiki.carrionfields.net/game/world".to_string(),
        summary: "The continent of Thera organized as a region graph: the Galadon hub at the \
            center, cardinal road spokes to the north/east/south/west reaches, the northwest keep \
            country, the Underdark below, and the Past and island seas beyond."
            .to_string(),
        regions: vec![
            region(
                "region.thera.central_crossroads",
                "Central Crossroads",
                WorldSector::Center,
                (1, 30),
                "The Galadon hub and its immediate satellites: the road-heart of Thera.",
                &[
                    "area.thera.galadon",
                    "area.thera.galadon.lumber_yard",
                    "area.thera.galadon.kobold_warrens",
                    "area.thera.galadon.graveyard",
                    "area.thera.galadon.goblin_village",
                    "area.thera.galadon.copeham_inn",
                    "area.thera.inn_of_the_eternal_star",
                    "area.thera.aldevari",
                    "area.thera.open_plains",
                ],
            ),
            region(
                "region.thera.northern_reach",
                "Northern Reach",
                WorldSector::North,
                (10, 40),
                "Udgaard and the cold northern plains, foothills, and mountains.",
                &[
                    "area.thera.udgaard",
                    "area.thera.udgaardian_plains",
                    "area.thera.northern_mountains",
                    "area.thera.northern_foothills",
                    "area.thera.hillcrest",
                    "area.thera.blackclaw",
                    "area.thera.dagdan",
                    "area.thera.frigid_wasteland",
                ],
            ),
            region(
                "region.thera.eastern_arkham",
                "Eastern Arkham Country",
                WorldSector::East,
                (15, 51),
                "Arkham and the eastern coast, reached down the Eastern Road.",
                &[
                    "area.thera.arkham",
                    "area.thera.forgotten_crypts",
                    "area.thera.mansion_of_vdramir",
                    "area.thera.tower_of_dragons",
                ],
            ),
            region(
                "region.thera.southern_desert",
                "Southern Desert",
                WorldSector::South,
                (5, 51),
                "Hamsah Mu'tazz and the twin deserts, steppes, and southern mountains.",
                &[
                    "area.thera.hamsah_mu_tazz",
                    "area.thera.sands_of_sorrow",
                    "area.thera.desert_of_araile",
                    "area.thera.pyramid_of_azhan",
                    "area.thera.oryx_steppes",
                    "area.thera.tahril_mountains",
                    "area.thera.wastes_of_nonviel",
                    "area.thera.xvart_lair",
                ],
            ),
            region(
                "region.thera.western_seantryn",
                "Western Seantryn",
                WorldSector::West,
                (10, 45),
                "Seantryn Modan and its wetlands and marsh belt west of the hub.",
                &[
                    "area.thera.seantryn_modan",
                    "area.thera.seantryn_wetlands",
                    "area.thera.marsh_belt",
                ],
            ),
            region(
                "region.thera.darsylon_wilds",
                "Darsylon Wilds",
                WorldSector::West,
                (15, 51),
                "The elven city of Darsylon and the deep western forests.",
                &[
                    "area.thera.darsylon",
                    "area.thera.azreth_forest",
                    "area.thera.dranettie_wood",
                    "area.thera.feanwyyn_weald",
                    "area.thera.emerald_forest",
                ],
            ),
            region(
                "region.thera.northwest_keeps",
                "Northwest Keep Country",
                WorldSector::Northwest,
                (20, 51),
                "Tir-Talath, Cragstone, and the ruined keeps and towers of the interior.",
                &[
                    "area.thera.tir_talath",
                    "area.thera.cragstone",
                    "area.thera.keep_of_fhalaugash",
                    "area.thera.ruined_keep",
                    "area.thera.lord_tabershaw_keep",
                    "area.thera.ruins_of_maethien",
                    "area.thera.octagonal_tower",
                    "area.thera.teth_azeleth",
                ],
            ),
            region(
                "region.thera.the_underdark",
                "The Underdark",
                WorldSector::Below,
                (20, 51),
                "The deep tunnels, drow and duergar holds, and troglodyte caverns below Thera.",
                &[
                    "area.thera.underdark.tunnels",
                    "area.thera.underdark.mines",
                    "area.thera.underdark.troglodyte_caverns",
                    "area.thera.underdark.svirfneblin_enclave",
                    "area.thera.underdark.arin_gird",
                    "area.thera.underdark.gol_galath",
                    "area.thera.underdark.velkyn_oloth",
                    "area.thera.kuo_toa_lair",
                    "area.thera.mines_of_zakiim",
                ],
            ),
            region(
                "region.thera.the_past",
                "The Past",
                WorldSector::Beyond,
                (1, 51),
                "The time-portal reach: Thera as it was, reached through the Vale.",
                &[
                    "area.thera.past.citadel_of_ostalagiah",
                    "area.thera.past.delar_tol",
                    "area.thera.past.maethien",
                    "area.thera.past.past_grove",
                    "area.thera.past.plains_of_arendyl",
                    "area.thera.past.the_battlefield",
                    "area.thera.past.wagon_marked_road",
                ],
            ),
            region(
                "region.thera.island_seas",
                "Island Seas",
                WorldSector::Beyond,
                (20, 51),
                "The ocean chains and island holds beyond Thera's coasts.",
                &[
                    "area.thera.isle_of_corte",
                    "area.thera.crystal_island",
                    "area.thera.coral_head",
                    "area.thera.coral_palace",
                    "area.thera.sea_of_despair",
                    "area.thera.waters_of_the_deep",
                    "area.thera.dragon_sea",
                    "area.thera.aryth_ocean",
                ],
            ),
        ],
        edges: vec![
            edge(
                "edge.thera.center_north",
                "region.thera.central_crossroads",
                "region.thera.northern_reach",
                RegionSpine::Road,
                "The North Road out of Galadon's north gate to Udgaard.",
            ),
            edge(
                "edge.thera.center_east",
                "region.thera.central_crossroads",
                "region.thera.eastern_arkham",
                RegionSpine::Road,
                "The Eastern Road past Aldevari to Arkham.",
            ),
            edge(
                "edge.thera.center_south",
                "region.thera.central_crossroads",
                "region.thera.southern_desert",
                RegionSpine::Road,
                "The southern road out of Galadon's south gate to Hamsah Mu'tazz.",
            ),
            edge(
                "edge.thera.center_west",
                "region.thera.central_crossroads",
                "region.thera.western_seantryn",
                RegionSpine::Road,
                "The western road to Seantryn Modan.",
            ),
            edge(
                "edge.thera.center_northwest",
                "region.thera.central_crossroads",
                "region.thera.northwest_keeps",
                RegionSpine::Road,
                "The northwest road into the keep country.",
            ),
            edge(
                "edge.thera.north_northwest",
                "region.thera.northern_reach",
                "region.thera.northwest_keeps",
                RegionSpine::Road,
                "The upland road linking the Northern Reach to the northwest interior.",
            ),
            edge(
                "edge.thera.west_darsylon",
                "region.thera.western_seantryn",
                "region.thera.darsylon_wilds",
                RegionSpine::Trail,
                "Forest trails west from Seantryn into the Darsylon wilds.",
            ),
            edge(
                "edge.thera.center_underdark",
                "region.thera.central_crossroads",
                "region.thera.the_underdark",
                RegionSpine::Underdark,
                "The Galadon sewer descent into the Underdark tunnels.",
            ),
            edge(
                "edge.thera.northwest_underdark",
                "region.thera.northwest_keeps",
                "region.thera.the_underdark",
                RegionSpine::Underdark,
                "Keep-country delvings that break into the deep tunnels.",
            ),
            edge(
                "edge.thera.center_past",
                "region.thera.central_crossroads",
                "region.thera.the_past",
                RegionSpine::Portal,
                "The time portal at the Vale of Arendyl into the Past.",
            ),
            edge(
                "edge.thera.east_islands",
                "region.thera.eastern_arkham",
                "region.thera.island_seas",
                RegionSpine::Sea,
                "The Dragon Sea lanes off the eastern coast.",
            ),
            edge(
                "edge.thera.south_islands",
                "region.thera.southern_desert",
                "region.thera.island_seas",
                RegionSpine::Sea,
                "The southern ocean crossing to the island holds.",
            ),
        ],
        implementation_notes: vec![
            "Regions partition the authored area corpus; membership is validated against the #1704 \
                registry so the topology cannot name an unauthored area."
                .to_string(),
            "The graph is one connected component (FSV asserts connectivity); the overworld is \
                authoritative WORLD data, never a free in-game minimap (#944)."
                .to_string(),
        ],
    }
}

fn region(
    region_id: &str,
    display_name: &str,
    sector: WorldSector,
    level_band: (u8, u8),
    summary: &str,
    member_area_ids: &[&str],
) -> TopologyRegion {
    TopologyRegion {
        region_id: region_id.to_string(),
        display_name: display_name.to_string(),
        sector,
        summary: summary.to_string(),
        level_band: AreaLevelBand {
            min_level: level_band.0,
            max_level: level_band.1,
        },
        member_area_ids: member_area_ids.iter().map(|s| s.to_string()).collect(),
    }
}

fn edge(
    edge_id: &str,
    from_region_id: &str,
    to_region_id: &str,
    spine: RegionSpine,
    summary: &str,
) -> RegionEdge {
    RegionEdge {
        edge_id: edge_id.to_string(),
        from_region_id: from_region_id.to_string(),
        to_region_id: to_region_id.to_string(),
        spine,
        summary: summary.to_string(),
    }
}
