use crate::tower_defense_fsv::{
    assert_issue743_windows_reruns_identical, build_issue743_leak_all_towers_sold_project,
    build_issue743_tower_defense_project, run_issue743_leak_loss,
    run_issue743_tower_defense_victory,
};

#[test]
fn issue743_no_code_tower_defense_reaches_victory_and_loss_edge() {
    let build = build_issue743_tower_defense_project(
        "issue743_td_unit",
        "Issue 743 Tower Defense",
        "issue743-unit-rev-1",
    )
    .unwrap();

    assert!(build.acceptance.no_lua_or_code_escape);
    assert!(build.acceptance.terrain_path_ready);
    assert!(build.acceptance.asset_products_assigned);
    assert_eq!(build.acceptance.object_roles["base"], 1);
    assert_eq!(build.acceptance.object_roles["tower"], 2);
    assert_eq!(build.acceptance.object_roles["creep"], 3);
    assert!(
        build
            .acceptance
            .trigger_ids
            .contains(&"trg_td_defeat".to_string())
    );

    let victory = run_issue743_tower_defense_victory(&build.clone, 512).unwrap();
    assert_eq!(victory.final_state_hash, victory.replay_state_hash);
    assert!(
        victory
            .executed_trigger_ids
            .contains(&"trg_td_victory".to_string())
    );

    let reruns = assert_issue743_windows_reruns_identical(&build.clone, 512, 2).unwrap();
    assert_eq!(reruns[0].final_state_hash, reruns[1].final_state_hash);

    let loss_clone = build_issue743_leak_all_towers_sold_project(&build).unwrap();
    let loss = run_issue743_leak_loss(&loss_clone, 512).unwrap();
    assert_eq!(loss.terminal_reason, "base destroyed");
    assert!(
        loss.executed_trigger_ids
            .contains(&"trg_td_defeat".to_string())
    );
}
