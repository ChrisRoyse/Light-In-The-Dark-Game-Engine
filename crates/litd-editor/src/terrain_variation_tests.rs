use std::collections::BTreeMap;

use super::{
    TerrainRandomGroup, TerrainRandomGroupCycleDirection, TerrainRandomGroupMember,
    TerrainRandomGroupMemberKind, TerrainRandomGroupPreview, TerrainTileVariation,
    auto_cycle_random_group_preview, cycle_random_group_preview, reset_texture_variations,
    tile_variation_summary, validate_random_groups, validate_tile_variations,
};

#[test]
fn reset_texture_variations_is_seeded_and_reproducible() {
    let mut tiles = fixture_tiles();
    let before = tile_variation_summary(&tiles);
    let expected_by_tile = tiles
        .iter()
        .map(|(id, tile)| (id.clone(), tile.deterministic_index_for_seed(77_001)))
        .collect::<BTreeMap<_, _>>();

    let report = reset_texture_variations(&mut tiles, 77_001).unwrap();
    let after = tile_variation_summary(&tiles);

    assert_eq!(report.seed, 77_001);
    assert_eq!(report.tile_count, 3);
    assert_eq!(report.before_summary, before);
    assert_eq!(report.after_summary, after);
    for (id, expected_index) in &expected_by_tile {
        let tile = tiles.get(id).unwrap();
        assert_eq!(tile.seed, 77_001);
        assert_eq!(tile.variation_index, *expected_index);
    }

    let mut rerolled_from_different_start = fixture_tiles();
    rerolled_from_different_start
        .get_mut("tile_grass_0_0")
        .unwrap()
        .variation_index = 3;
    reset_texture_variations(&mut rerolled_from_different_start, 77_001).unwrap();

    assert_eq!(
        tile_variation_summary(&rerolled_from_different_start),
        after
    );
    println!(
        "reset_texture_variations before={} after={} report_changed={} report_unchanged={}",
        before, after, report.changed_tile_count, report.unchanged_tile_count
    );
}

#[test]
fn random_groups_select_weighted_members_and_single_member_groups_are_fixed() {
    let group = fixture_group();

    let first = group.select_member("terrain-cell:12,-4", 9).unwrap();
    let second = group.select_member("terrain-cell:12,-4", 9).unwrap();
    assert_eq!(first, second);

    let fixed_group = TerrainRandomGroup::new(
        "fixed_grass",
        "Fixed Grass",
        44,
        vec![TerrainRandomGroupMember::new(
            "only",
            TerrainRandomGroupMemberKind::Tile,
            "tileset.grass.only",
            1,
        )],
    );
    assert_eq!(fixed_group.select_member_index("a", 1).unwrap(), 0);
    assert_eq!(fixed_group.select_member_index("b", 9_999).unwrap(), 0);
    assert_eq!(
        fixed_group.select_member("b", 9_999).unwrap().asset_ref,
        "tileset.grass.only"
    );
    println!(
        "random_group deterministic={} fixed_member={}",
        first.asset_ref,
        fixed_group.select_member("b", 9_999).unwrap().asset_ref
    );
}

#[test]
fn auto_cycle_preview_changes_preview_without_mutating_saved_group() {
    let group = fixture_group();
    let saved_group_before = group.clone();
    let preview = TerrainRandomGroupPreview::new(&group).unwrap();

    let cycled =
        cycle_random_group_preview(&group, &preview, TerrainRandomGroupCycleDirection::Next)
            .unwrap();
    let auto_cycled = auto_cycle_random_group_preview(&group, &cycled).unwrap();

    assert_eq!(group, saved_group_before);
    assert_eq!(preview.current_member(&group).unwrap().id, "grass_a");
    assert_eq!(cycled.current_member(&group).unwrap().id, "grass_b");
    assert_eq!(auto_cycled.current_member(&group).unwrap().id, "fern");
    assert!(!cycled.auto_cycle);
    assert!(auto_cycled.auto_cycle);
    assert_eq!(cycled.tick, 1);
    assert_eq!(auto_cycled.tick, 2);
    println!(
        "preview before_index={} after_index={} auto_index={} group_members={}",
        preview.current_index,
        cycled.current_index,
        auto_cycled.current_index,
        group.members.len()
    );
}

#[test]
fn invalid_variation_and_group_state_fails_before_mutation() {
    let mut duplicate_tiles = fixture_tiles();
    let before_tiles = duplicate_tiles.clone();
    duplicate_tiles.insert(
        "tile_duplicate_coord".to_string(),
        TerrainTileVariation::new("tile_duplicate_coord", 0, 0, "tileset.grass", 4, 0, 1),
    );

    let tile_error = validate_tile_variations(&duplicate_tiles).unwrap_err();
    assert_eq!(before_tiles.len(), 3);
    assert!(tile_error.to_string().contains("tile.coord"));

    let mut invalid_groups = BTreeMap::new();
    invalid_groups.insert(
        "bad_group".to_string(),
        TerrainRandomGroup::new(
            "bad_group",
            "Bad Group",
            9,
            vec![TerrainRandomGroupMember::new(
                "zero",
                TerrainRandomGroupMemberKind::Doodad,
                "doodad.bad",
                0,
            )],
        ),
    );

    let group_error = validate_random_groups(&invalid_groups).unwrap_err();
    assert!(group_error.to_string().contains("weight"));
    println!("invalid_tiles_error={tile_error} invalid_group_error={group_error}");
}

#[test]
fn empty_variation_reset_has_explicit_empty_state() {
    let mut tiles = BTreeMap::new();
    let report = reset_texture_variations(&mut tiles, 123).unwrap();

    assert_eq!(report.seed, 123);
    assert_eq!(report.tile_count, 0);
    assert_eq!(report.changed_tile_count, 0);
    assert_eq!(report.unchanged_tile_count, 0);
    assert!(report.before_summary.is_empty());
    assert!(report.after_summary.is_empty());
    println!(
        "empty_reset before='{}' after='{}' tiles={}",
        report.before_summary, report.after_summary, report.tile_count
    );
}

fn fixture_tiles() -> BTreeMap<String, TerrainTileVariation> {
    [
        TerrainTileVariation::new("tile_grass_0_0", 0, 0, "tileset.grass", 4, 0, 101),
        TerrainTileVariation::new("tile_grass_1_0", 1, 0, "tileset.grass", 4, 1, 101),
        TerrainTileVariation::new("tile_dirt_0_1", 0, 1, "tileset.dirt", 3, 2, 101),
    ]
    .into_iter()
    .map(|tile| (tile.id.clone(), tile))
    .collect()
}

fn fixture_group() -> TerrainRandomGroup {
    TerrainRandomGroup::new(
        "forest_floor_mix",
        "Forest Floor Mix",
        88_123,
        vec![
            TerrainRandomGroupMember::new(
                "grass_a",
                TerrainRandomGroupMemberKind::Tile,
                "tileset.grass.variant_a",
                5,
            ),
            TerrainRandomGroupMember::new(
                "grass_b",
                TerrainRandomGroupMemberKind::Tile,
                "tileset.grass.variant_b",
                3,
            ),
            TerrainRandomGroupMember::new(
                "fern",
                TerrainRandomGroupMemberKind::Doodad,
                "doodad.fern.low",
                2,
            ),
        ],
    )
}
