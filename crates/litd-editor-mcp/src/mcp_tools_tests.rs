use super::*;

#[test]
fn default_toolset_covers_mcp_child_domains() {
    let manifest = default_mcp_domain_toolset_manifest().unwrap();
    let report = build_mcp_domain_toolset_report(&manifest).unwrap();
    trace_happy_state(&report);

    assert_eq!(report.parent_issue, 1145);
    assert_eq!(report.root_mcp_epic, 1143);
    assert_eq!(report.source_mcp_epic, 916);
    assert_eq!(report.capability_layer_issue, 1144);
    assert_eq!(report.domain_count, 12);
    // #1455 added the object_game_data domain tool editor.rpg.author_race (88 -> 89, mutating
    // 69 -> 70). #812 added the terrain_world readback tool
    // editor.world.evaluate_region_transitions (89 -> 90, readback 19 -> 20). #862 added the
    // render_materials no-code PBR authoring pair editor.material.author_pbr_material (mutating)
    // + editor.material.read_pbr_material (readback) (90 -> 92, mutating 70 -> 71,
    // readback 20 -> 21). #853 added three project_playtest campaign tools: author_campaign +
    // author_campaign_chapter (mutating 71 -> 73) and read_campaign (readback 21 -> 22),
    // total 92 -> 95. #892/#901 added four terrain_world tools -- author_timeline +
    // clone_template (mutating 73 -> 75) and evaluate_timeline + list_templates
    // (readback 22 -> 24) -- taking tool_count 95 -> 99. #818/#829 added the
    // object_game_data readback tools editor.objects.lower_unit and
    // editor.objects.read_lowered_unit (99 -> 101, readback 24 -> 26) that expose the
    // #829 lowering valve as a scriptable surface. #835 added trigger_runtime tools
    // editor.triggers.set_trigger_organization (mutating, 101 -> 102, mutating 75 -> 76)
    // and editor.triggers.query_cross_ref (readback, 102 -> 103, readback 26 -> 27),
    // total 101 -> 103. #819/#820 added four object_game_data readback tools --
    // editor.objects.lower_ability + editor.objects.read_lowered_ability (#819) and
    // editor.objects.lower_hero + editor.objects.read_lowered_hero (#820) -- exposing the #829
    // valve for the ability/hero layers (103 -> 107, readback 27 -> 31), mutating unchanged 76.
    // #821/#822 added FOUR more object_game_data readback tools --
    // editor.objects.lower_item + editor.objects.read_lowered_item (#821 item lowering valve)
    // and editor.objects.lower_shop + editor.objects.read_lowered_shop (#822 recipe/shop/
    // drop-table commerce lowering valve) -- taking tool_count 107 -> 111 and readback 31 -> 35.
    // #825/#827 added FOUR more object_game_data readback tools (lower_tech_requirements,
    // read_lowered_tech_requirements, lower_gameplay_constants, read_lowered_gameplay_constants)
    // -- taking tool_count 111 -> 115 and readback 35 -> 39 (mutating unchanged at 76). Derived
    // from the registry at the final rebase: base 111 / 76 / 35 + 4 readback = 115 / 76 / 39.
    // #823/#824/#826/#828 added EIGHT more object_game_data readback tools -- four
    // lower_*/read_lowered_* #829-valve pairs for buffs, upgrades, destructible+doodad types,
    // and the Game Interface record (all readback, non-mutating): 115 -> 123, readback 39 -> 47,
    // mutating unchanged at 76. Derived from the registry at the final rebase:
    // base 115 / 76 / 39 + 8 readback = 123 / 76 / 47.
    assert_eq!(report.tool_count, 123);
    assert_eq!(report.mutating_tool_count, 76);
    assert_eq!(report.readback_tool_count, 47);
    assert_eq!(report.fsv_tool_count, 5);
    assert_eq!(
        report.child_issues,
        vec![918, 919, 920, 921, 922, 923, 924, 925, 1467, 926, 927, 1178]
    );
    assert_eq!(report.shared_capability_coverage_count, 9);
    assert_eq!(report.local_only_count, 123);
    assert_eq!(report.structured_result_count, 123);
    assert_eq!(report.lowering_guarded_mutation_count, 76);
    assert_eq!(report.ward_guarded_mutation_count, 76);
    assert_eq!(report.ledger_audited_count, 123);
}

#[test]
fn mutations_have_matching_readback_tools() {
    let manifest = default_mcp_domain_toolset_manifest().unwrap();
    let tools = manifest
        .domains
        .iter()
        .flat_map(|domain| domain.tools.iter().map(|tool| tool.tool_name.as_str()))
        .collect::<BTreeSet<_>>();
    let mutating_tools = manifest
        .domains
        .iter()
        .flat_map(|domain| domain.tools.iter())
        .filter(|tool| tool.mutates_state)
        .collect::<Vec<_>>();
    assert_eq!(
        mutating_tools.len(),
        76,
        "mutating tool set must be non-vacuous (#819/#820 added readback-only tools)"
    );

    for tool in mutating_tools {
        assert!(tools.contains(tool.fsv_readback_tool_name.as_str()));
        assert!(tool.requires_lowering);
        assert!(tool.requires_ward);
    }
}

#[test]
fn duplicate_tool_name_fails_loud() {
    let capability_layer = default_capability_layer_manifest().unwrap();
    let mut manifest = default_mcp_domain_toolset_manifest().unwrap();
    manifest.domains[1].tools[0].tool_name = manifest.domains[0].tools[0].tool_name.clone();
    trace_edge_state("duplicate_tool_name", "before_validate", &manifest, None);

    match validate_mcp_domain_toolset_manifest(&manifest, &capability_layer) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            trace_edge_state(
                "duplicate_tool_name",
                "after_validate",
                &manifest,
                Some((&field, &reason)),
            );
            assert_eq!(field, "editor.mcp_tools.tool.name");
            assert!(reason.contains("duplicate"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn mutation_without_readback_fails_loud() {
    let capability_layer = default_capability_layer_manifest().unwrap();
    let mut manifest = default_mcp_domain_toolset_manifest().unwrap();
    manifest.domains[0].tools[0].fsv_readback_tool_name = "litd.editor.world.missing".to_string();
    trace_edge_state(
        "mutation_without_readback",
        "before_validate",
        &manifest,
        None,
    );

    match validate_mcp_domain_toolset_manifest(&manifest, &capability_layer) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            trace_edge_state(
                "mutation_without_readback",
                "after_validate",
                &manifest,
                Some((&field, &reason)),
            );
            assert_eq!(field, "editor.mcp_tools.tool.readback");
            assert!(reason.contains("readback"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn non_local_tool_fails_loud() {
    let capability_layer = default_capability_layer_manifest().unwrap();
    let mut manifest = default_mcp_domain_toolset_manifest().unwrap();
    manifest.domains[0].tools[0].local_only = false;
    trace_edge_state("non_local_tool", "before_validate", &manifest, None);

    match validate_mcp_domain_toolset_manifest(&manifest, &capability_layer) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            trace_edge_state(
                "non_local_tool",
                "after_validate",
                &manifest,
                Some((&field, &reason)),
            );
            assert_eq!(field, "editor.mcp_tools.tool.safety");
            assert!(reason.contains("local-only"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn legacy_render_reference_fails_loud() {
    let capability_layer = default_capability_layer_manifest().unwrap();
    let mut manifest = default_mcp_domain_toolset_manifest().unwrap();
    manifest.domains[5].title = "g3n render controls".to_string();
    trace_edge_state(
        "legacy_render_reference",
        "before_validate",
        &manifest,
        None,
    );

    match validate_mcp_domain_toolset_manifest(&manifest, &capability_layer) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            trace_edge_state(
                "legacy_render_reference",
                "after_validate",
                &manifest,
                Some((&field, &reason)),
            );
            assert_eq!(field, "editor.mcp_tools.domain.legacy");
            assert!(reason.contains("legacy engine"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn trace_happy_state(report: &McpDomainToolsetReport) {
    if !trace_enabled() {
        return;
    }
    println!(
        "FSV_HAPPY_STATE status=validated parent_issue={} domain_count={} tool_count={} mutating_tool_count={} readback_tool_count={} local_only_count={} structured_result_count={} lowering_guarded_mutation_count={} ward_guarded_mutation_count={} ledger_audited_count={} child_issues={:?}",
        report.parent_issue,
        report.domain_count,
        report.tool_count,
        report.mutating_tool_count,
        report.readback_tool_count,
        report.local_only_count,
        report.structured_result_count,
        report.lowering_guarded_mutation_count,
        report.ward_guarded_mutation_count,
        report.ledger_audited_count,
        report.child_issues
    );
}

fn trace_edge_state(
    case: &str,
    phase: &str,
    manifest: &McpDomainToolsetManifest,
    outcome: Option<(&str, &str)>,
) {
    if !trace_enabled() {
        return;
    }
    let first_tool = &manifest.domains[0].tools[0];
    let second_tool = &manifest.domains[1].tools[0];
    let render_domain = &manifest.domains[5];
    let (error_field, error_reason) = outcome.unwrap_or(("", ""));
    println!(
        "FSV_EDGE_STATE case={} phase={} domain0_tool={} domain1_tool={} domain0_readback={} domain0_local_only={} render_domain_title={:?} error_field={:?} error_reason={:?}",
        case,
        phase,
        first_tool.tool_name,
        second_tool.tool_name,
        first_tool.fsv_readback_tool_name,
        first_tool.local_only,
        render_domain.title,
        error_field,
        error_reason
    );
}

fn trace_enabled() -> bool {
    std::env::var_os("LITD_MCP_TEST_STATE_TRACE").is_some()
}
