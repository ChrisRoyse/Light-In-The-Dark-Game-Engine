use std::collections::{BTreeMap, BTreeSet};

use litd_hash::Fnv1a64;

use crate::{
    EngineError,
    nav::{
        BakedNavBounds, BakedNavCell, BakedNavMesh, NAV_BAKE_INPUT_SCHEMA, NAVMESH_SCHEMA,
        NavBakeInput, NavPathingFlags, NavRebakeReport, NavStaticBlockerInput, NavTerrainCellInput,
    },
};

pub fn bake_navmesh(input: &NavBakeInput) -> Result<BakedNavMesh, EngineError> {
    validate_input(input)?;
    let sorted_cells = sorted_cells(input)?;
    let blockers = sorted_blockers(input)?;
    let bounds = bounds_for(&sorted_cells)?;
    validate_rectangular_coverage(&sorted_cells, bounds)?;
    validate_blockers_in_bounds(&blockers, bounds)?;

    let mut cells = Vec::with_capacity(sorted_cells.len());
    let mut walkable_cell_count = 0_usize;
    for source in sorted_cells {
        let static_blocker_ids = blockers
            .iter()
            .filter(|blocker| covers(blocker, source.x, source.z))
            .map(|blocker| blocker.id.clone())
            .collect::<Vec<_>>();
        let blocked_by_pathing = source.flags.blocked || !source.flags.walkable;
        let walkable = !blocked_by_pathing && static_blocker_ids.is_empty();
        if walkable {
            walkable_cell_count += 1;
        }
        cells.push(BakedNavCell {
            id: source.id.clone(),
            x: source.x,
            z: source.z,
            height_fp: source.height_fp,
            walkable,
            buildable: walkable && source.flags.buildable,
            flyable: source.flags.flyable && !source.flags.blocked,
            blocked_by_pathing,
            static_blocker_ids,
        });
    }

    let boundary_matches_input = boundary_matches_input(&cells, input, bounds);
    let mut mesh = BakedNavMesh {
        schema: NAVMESH_SCHEMA.to_string(),
        source_input_schema: input.schema.clone(),
        cell_size_fp: input.cell_size_fp,
        bounds,
        terrain_cell_count: cells.len(),
        static_blocker_count: blockers.len(),
        walkable_cell_count,
        blocked_cell_count: cells.len() - walkable_cell_count,
        boundary_matches_input,
        cells,
        fnv1a64: String::new(),
    };
    mesh.fnv1a64 = compute_navmesh_fnv1a64(&mesh);
    verify_navmesh_integrity(&mesh)?;
    Ok(mesh)
}

pub fn rebake_navmesh(
    input: &NavBakeInput,
    previous: Option<&BakedNavMesh>,
) -> Result<NavRebakeReport, EngineError> {
    let navmesh = bake_navmesh(input)?;
    let previous_fnv1a64 = previous.map(|mesh| mesh.fnv1a64.clone());
    let changed = previous_fnv1a64
        .as_deref()
        .map(|hash| hash != navmesh.fnv1a64)
        .unwrap_or(true);
    Ok(NavRebakeReport {
        previous_fnv1a64,
        current_fnv1a64: navmesh.fnv1a64.clone(),
        changed,
        navmesh,
    })
}

pub fn verify_navmesh_integrity(mesh: &BakedNavMesh) -> Result<(), EngineError> {
    if mesh.schema != NAVMESH_SCHEMA {
        return invalid_navmesh("schema", "must be litd.navmesh.grid.v1");
    }
    if mesh.source_input_schema != NAV_BAKE_INPUT_SCHEMA {
        return invalid_navmesh("source_input_schema", "must be litd.navmesh.bake_input.v1");
    }
    if mesh.cell_size_fp <= 0 {
        return invalid_navmesh("cell_size_fp", "must be a positive fixed-point value");
    }
    if mesh.cells.is_empty() {
        return invalid_navmesh("cells", "serialized navmesh must contain at least one cell");
    }
    if mesh.bounds.width == 0 || mesh.bounds.depth == 0 {
        return invalid_navmesh("bounds", "serialized bounds width/depth must be positive");
    }
    if mesh.terrain_cell_count != mesh.cells.len() {
        return invalid_navmesh("terrain_cell_count", "must match serialized cell count");
    }
    if mesh.walkable_cell_count + mesh.blocked_cell_count != mesh.cells.len() {
        return invalid_navmesh("cell_counts", "walkable + blocked must match cell count");
    }
    validate_serialized_cells(mesh)?;
    let actual = compute_navmesh_fnv1a64(mesh);
    if actual != mesh.fnv1a64 {
        return Err(EngineError::NavReadbackMismatch {
            field: "fnv1a64".to_string(),
            expected: mesh.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

pub fn compute_navmesh_fnv1a64(mesh: &BakedNavMesh) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &mesh.schema);
    update_str(&mut hasher, &mesh.source_input_schema);
    hasher.update_i64_le(mesh.cell_size_fp);
    hash_bounds(&mut hasher, mesh.bounds);
    hasher.update_u64_le(mesh.terrain_cell_count as u64);
    hasher.update_u64_le(mesh.static_blocker_count as u64);
    hasher.update_u64_le(mesh.walkable_cell_count as u64);
    hasher.update_u64_le(mesh.blocked_cell_count as u64);
    hasher.update_u8(u8::from(mesh.boundary_matches_input));
    hasher.update_u64_le(mesh.cells.len() as u64);
    for cell in &mesh.cells {
        update_str(&mut hasher, &cell.id);
        hasher.update_i64_le(i64::from(cell.x));
        hasher.update_i64_le(i64::from(cell.z));
        hasher.update_i64_le(cell.height_fp);
        hasher.update_u8(u8::from(cell.walkable));
        hasher.update_u8(u8::from(cell.buildable));
        hasher.update_u8(u8::from(cell.flyable));
        hasher.update_u8(u8::from(cell.blocked_by_pathing));
        hasher.update_u64_le(cell.static_blocker_ids.len() as u64);
        for blocker_id in &cell.static_blocker_ids {
            update_str(&mut hasher, blocker_id);
        }
    }
    hasher.finish_hex()
}

fn validate_input(input: &NavBakeInput) -> Result<(), EngineError> {
    if input.schema != NAV_BAKE_INPUT_SCHEMA {
        return invalid_config("schema", "must be litd.navmesh.bake_input.v1");
    }
    if input.cell_size_fp <= 0 {
        return invalid_config("cell_size_fp", "must be a positive fixed-point value");
    }
    if input.terrain_cells.is_empty() {
        return invalid_config("terrain_cells", "must contain at least one terrain cell");
    }
    for cell in &input.terrain_cells {
        validate_id("terrain_cells.id", &cell.id)?;
        validate_flags(cell.flags)?;
    }
    for blocker in &input.static_blockers {
        validate_id("static_blockers.id", &blocker.id)?;
        if blocker.min_x > blocker.max_x || blocker.min_z > blocker.max_z {
            return invalid_config(
                format!("static_blockers.{}", blocker.id),
                "min coordinates must be <= max coordinates",
            );
        }
    }
    Ok(())
}

fn sorted_cells(input: &NavBakeInput) -> Result<Vec<&NavTerrainCellInput>, EngineError> {
    let mut ids = BTreeSet::new();
    let mut coords = BTreeSet::new();
    let mut sorted = input.terrain_cells.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|cell| (cell.z, cell.x, cell.id.as_str()));
    for cell in &sorted {
        if !ids.insert(cell.id.as_str()) {
            return invalid_config("terrain_cells.id", format!("duplicate id {}", cell.id));
        }
        if !coords.insert((cell.x, cell.z)) {
            return invalid_config(
                "terrain_cells.coord",
                format!("duplicate coordinate {},{}", cell.x, cell.z),
            );
        }
    }
    Ok(sorted)
}

fn sorted_blockers(input: &NavBakeInput) -> Result<Vec<&NavStaticBlockerInput>, EngineError> {
    let mut ids = BTreeSet::new();
    let mut blockers = input.static_blockers.iter().collect::<Vec<_>>();
    blockers.sort_by_key(|blocker| {
        (
            blocker.id.as_str(),
            blocker.min_z,
            blocker.min_x,
            blocker.max_z,
            blocker.max_x,
        )
    });
    for blocker in &blockers {
        if !ids.insert(blocker.id.as_str()) {
            return invalid_config("static_blockers.id", format!("duplicate id {}", blocker.id));
        }
    }
    Ok(blockers)
}

fn bounds_for(cells: &[&NavTerrainCellInput]) -> Result<BakedNavBounds, EngineError> {
    let min_x = cells.iter().map(|cell| cell.x).min().unwrap();
    let max_x = cells.iter().map(|cell| cell.x).max().unwrap();
    let min_z = cells.iter().map(|cell| cell.z).min().unwrap();
    let max_z = cells.iter().map(|cell| cell.z).max().unwrap();
    let width = span_len("bounds.width", min_x, max_x)?;
    let depth = span_len("bounds.depth", min_z, max_z)?;
    Ok(BakedNavBounds {
        min_x,
        max_x,
        min_z,
        max_z,
        width,
        depth,
    })
}

fn validate_rectangular_coverage(
    cells: &[&NavTerrainCellInput],
    bounds: BakedNavBounds,
) -> Result<(), EngineError> {
    let expected = u128::from(bounds.width) * u128::from(bounds.depth);
    if expected != cells.len() as u128 {
        return invalid_config(
            "terrain_cells.coverage",
            format!(
                "rectangular bounds require {expected} cells but input contains {}",
                cells.len()
            ),
        );
    }
    Ok(())
}

fn validate_blockers_in_bounds(
    blockers: &[&NavStaticBlockerInput],
    bounds: BakedNavBounds,
) -> Result<(), EngineError> {
    for blocker in blockers {
        if blocker.min_x < bounds.min_x
            || blocker.max_x > bounds.max_x
            || blocker.min_z < bounds.min_z
            || blocker.max_z > bounds.max_z
        {
            return invalid_config(
                format!("static_blockers.{}", blocker.id),
                "blocker bounds must stay inside terrain bounds",
            );
        }
    }
    Ok(())
}

fn validate_serialized_cells(mesh: &BakedNavMesh) -> Result<(), EngineError> {
    let mut coords = BTreeMap::new();
    let mut walkable = 0_usize;
    for cell in &mesh.cells {
        validate_id("cells.id", &cell.id)?;
        if coords.insert((cell.x, cell.z), &cell.id).is_some() {
            return invalid_navmesh("cells.coord", "serialized cells contain duplicate coords");
        }
        if cell.x < mesh.bounds.min_x
            || cell.x > mesh.bounds.max_x
            || cell.z < mesh.bounds.min_z
            || cell.z > mesh.bounds.max_z
        {
            return invalid_navmesh("cells.coord", "serialized cell is outside bounds");
        }
        if cell.walkable {
            walkable += 1;
        }
    }
    if walkable != mesh.walkable_cell_count {
        return invalid_navmesh("walkable_cell_count", "must match serialized cells");
    }
    validate_rectangular_serialized_coverage(mesh, coords.len())?;
    Ok(())
}

fn validate_rectangular_serialized_coverage(
    mesh: &BakedNavMesh,
    coord_count: usize,
) -> Result<(), EngineError> {
    let expected = u128::from(mesh.bounds.width) * u128::from(mesh.bounds.depth);
    if expected != coord_count as u128 || !mesh.boundary_matches_input {
        return invalid_navmesh(
            "bounds",
            "serialized cells must exactly cover rectangular input bounds",
        );
    }
    Ok(())
}

fn boundary_matches_input(
    cells: &[BakedNavCell],
    input: &NavBakeInput,
    bounds: BakedNavBounds,
) -> bool {
    let input_coords = input
        .terrain_cells
        .iter()
        .map(|cell| (cell.x, cell.z))
        .collect::<BTreeSet<_>>();
    let baked_coords = cells
        .iter()
        .map(|cell| (cell.x, cell.z))
        .collect::<BTreeSet<_>>();
    let expected = u128::from(bounds.width) * u128::from(bounds.depth);
    input_coords == baked_coords
        && baked_coords.len() as u128 == expected
        && baked_coords.iter().all(|(x, z)| {
            *x >= bounds.min_x && *x <= bounds.max_x && *z >= bounds.min_z && *z <= bounds.max_z
        })
}

fn validate_flags(flags: NavPathingFlags) -> Result<(), EngineError> {
    if flags.blocked && (flags.walkable || flags.buildable || flags.flyable) {
        return invalid_config(
            "flags.blocked",
            "blocked cells cannot also be walkable, buildable, or flyable",
        );
    }
    if flags.buildable && !flags.walkable {
        return invalid_config("flags.buildable", "buildable cells must also be walkable");
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_config(field, "must be non-empty");
    }
    if value.bytes().any(|byte| byte.is_ascii_control()) {
        return invalid_config(field, "must not contain control characters");
    }
    Ok(())
}

fn span_len(field: &'static str, min: i32, max: i32) -> Result<u64, EngineError> {
    let span = i64::from(max) - i64::from(min) + 1;
    if span <= 0 {
        return invalid_config(field, "bounds span must be positive");
    }
    Ok(span as u64)
}

fn covers(blocker: &NavStaticBlockerInput, x: i32, z: i32) -> bool {
    x >= blocker.min_x && x <= blocker.max_x && z >= blocker.min_z && z <= blocker.max_z
}

fn hash_bounds(hasher: &mut Fnv1a64, bounds: BakedNavBounds) {
    hasher.update_i64_le(i64::from(bounds.min_x));
    hasher.update_i64_le(i64::from(bounds.max_x));
    hasher.update_i64_le(i64::from(bounds.min_z));
    hasher.update_i64_le(i64::from(bounds.max_z));
    hasher.update_u64_le(bounds.width);
    hasher.update_u64_le(bounds.depth);
}

fn update_str(hasher: &mut Fnv1a64, value: &str) {
    hasher.update_u64_le(value.len() as u64);
    hasher.update(value.as_bytes());
}

fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::NavInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

fn invalid_navmesh<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    invalid_config(field, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_matches_input_is_derived_from_baked_cells() {
        let input = NavBakeInput::new(
            1024,
            vec![input_cell("cell_0_0", 0, 0), input_cell("cell_1_0", 1, 0)],
            Vec::new(),
        );
        let bounds = BakedNavBounds {
            min_x: 0,
            max_x: 1,
            min_z: 0,
            max_z: 0,
            width: 2,
            depth: 1,
        };
        let matching = vec![baked_cell("cell_0_0", 0, 0), baked_cell("cell_1_0", 1, 0)];
        let missing = vec![baked_cell("cell_0_0", 0, 0)];
        let shifted = vec![baked_cell("cell_0_0", 0, 0), baked_cell("cell_2_0", 2, 0)];

        assert!(boundary_matches_input(&matching, &input, bounds));
        assert!(!boundary_matches_input(&missing, &input, bounds));
        assert!(!boundary_matches_input(&shifted, &input, bounds));
    }

    fn input_cell(id: &str, x: i32, z: i32) -> NavTerrainCellInput {
        NavTerrainCellInput {
            id: id.to_string(),
            x,
            z,
            height_fp: 1024,
            flags: NavPathingFlags::default(),
        }
    }

    fn baked_cell(id: &str, x: i32, z: i32) -> BakedNavCell {
        BakedNavCell {
            id: id.to_string(),
            x,
            z,
            height_fp: 1024,
            walkable: true,
            buildable: false,
            flyable: true,
            blocked_by_pathing: false,
            static_blocker_ids: Vec::new(),
        }
    }
}
