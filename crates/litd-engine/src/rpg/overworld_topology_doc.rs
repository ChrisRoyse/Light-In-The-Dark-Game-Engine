use super::overworld_topology_types::*;

/// Renders the master map document (#1632) FROM the topology data -- never a
/// hand-maintained parallel prose map. The committed `docs/world/` markdown is
/// asserted equal to this output by an FSV test, so the doc can never drift from
/// the authoritative graph.
pub fn render_overworld_master_map_markdown(spec: &OverworldTopologySpec) -> String {
    let mut out = String::new();
    out.push_str("<!-- GENERATED FROM litd.rpg.overworld_topology.v1 by\n");
    out.push_str("     render_overworld_master_map_markdown (#1632). DO NOT EDIT BY HAND;\n");
    out.push_str("     edit the authored OverworldTopologySpec and regenerate. -->\n\n");
    out.push_str(&format!("# {} - Master Map\n\n", spec.display_name));
    out.push_str(&format!("{}\n\n", spec.summary));
    out.push_str(&format!("- Source: {}\n", spec.mechanics_source_url));
    out.push_str(&format!(
        "- Canon issues: {}\n",
        join_u32(&spec.canon_issues)
    ));
    out.push_str(&format!("- Regions: {}\n", spec.regions.len()));
    out.push_str(&format!("- Spine edges: {}\n\n", spec.edges.len()));

    out.push_str("## Regions by sector\n\n");
    for sector in SECTOR_ORDER {
        let regions: Vec<&TopologyRegion> = spec
            .regions
            .iter()
            .filter(|r| r.sector == *sector)
            .collect();
        if regions.is_empty() {
            continue;
        }
        out.push_str(&format!("### {}\n\n", sector_label(*sector)));
        for region in regions {
            out.push_str(&format!(
                "- **{}** (`{}`), levels {}-{}: {}\n",
                region.display_name,
                region.region_id,
                region.level_band.min_level,
                region.level_band.max_level,
                region.summary
            ));
            for area_id in &region.member_area_ids {
                out.push_str(&format!("  - `{area_id}`\n"));
            }
        }
        out.push('\n');
    }

    out.push_str("## Region spine (adjacency)\n\n");
    out.push_str("| From | To | Spine | Notes |\n");
    out.push_str("|------|----|-------|-------|\n");
    for edge in &spec.edges {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            edge.from_region_id,
            edge.to_region_id,
            spine_label(edge.spine),
            edge.summary
        ));
    }
    out.push('\n');

    out.push_str("## Implementation notes\n\n");
    for note in &spec.implementation_notes {
        out.push_str(&format!("- {note}\n"));
    }
    out.push('\n');
    out
}

const SECTOR_ORDER: &[WorldSector] = &[
    WorldSector::Center,
    WorldSector::North,
    WorldSector::East,
    WorldSector::South,
    WorldSector::West,
    WorldSector::Northwest,
    WorldSector::Below,
    WorldSector::Beyond,
];

fn sector_label(sector: WorldSector) -> &'static str {
    match sector {
        WorldSector::Center => "Center",
        WorldSector::North => "North",
        WorldSector::East => "East",
        WorldSector::South => "South",
        WorldSector::West => "West / Southwest",
        WorldSector::Northwest => "Northwest interior",
        WorldSector::Below => "Below (the Underdark)",
        WorldSector::Beyond => "Beyond (planes, portals, island chains)",
    }
}

fn spine_label(spine: RegionSpine) -> &'static str {
    match spine {
        RegionSpine::Road => "road",
        RegionSpine::Sea => "sea",
        RegionSpine::Portal => "portal",
        RegionSpine::Underdark => "underdark",
        RegionSpine::Trail => "trail",
    }
}

fn join_u32(values: &[u32]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
