use super::*;

#[test]
fn default_manifest_rebases_all_referenced_module_families() {
    let manifest = default_module_rebase_manifest().unwrap();
    let report = build_module_rebase_report(&manifest).unwrap();

    assert_eq!(report.parent_issue, 1142);
    assert_eq!(report.root_editor_epic, 1137);
    assert_eq!(report.module_count, REQUIRED_MODULE_IDS.len());
    assert_eq!(
        report.source_issues,
        vec![801, 802, 803, 804, 805, 886, 933, 972, 1048]
    );
    assert_eq!(report.gui_surface_count, report.module_count);
    assert_eq!(report.mcp_tool_group_count, report.module_count);
    assert_eq!(report.rust_engine_bound_count, report.module_count);
    assert_eq!(report.calyx_bound_count, report.module_count);
    assert_eq!(report.lowering_valve_count, report.module_count);
    assert_eq!(report.shared_capability_count, report.module_count);
}

#[test]
fn default_manifest_carries_visual_reference_images() {
    let report = build_module_rebase_report(&default_module_rebase_manifest().unwrap()).unwrap();

    for expected in [
        "docs/readme/ED02-terrain-editor.png",
        "docs/readme/ED03-object-editor.png",
        "docs/readme/ED04-trigger-editor.png",
        "docs/readme/ED07-cinematic-director.png",
        "docs/readme/ED09-test-in-editor.png",
        "docs/readme/ED11-world-view.png",
        "docs/readme/ED12-play-topdown.png",
        "docs/readme/ED13-play-firstperson.png",
        "docs/readme/GP09-inventory.png",
        "docs/readme/GP10-dialogue.png",
        "docs/readme/GP11-quest.png",
        "docs/readme/S16-quest-designer.png",
        "docs/readme/S18-lore-authoring.png",
    ] {
        assert!(report.readme_refs.iter().any(|path| path == expected));
    }
}

#[test]
fn missing_required_module_fails_loud() {
    let mut manifest = default_module_rebase_manifest().unwrap();
    manifest
        .modules
        .retain(|module| module.id != "lore.worldbuilding");

    match validate_module_rebase_manifest(&manifest) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.modules.modules.required");
            assert!(reason.contains("lore.worldbuilding"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn duplicate_module_id_fails_loud() {
    let mut manifest = default_module_rebase_manifest().unwrap();
    manifest.modules[1].id = manifest.modules[0].id.clone();

    match validate_module_rebase_manifest(&manifest) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.modules.modules.id");
            assert!(reason.contains("duplicate"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn module_without_calyx_lowering_fails_loud() {
    let mut manifest = default_module_rebase_manifest().unwrap();
    manifest.modules[0]
        .calyx_bindings
        .retain(|binding| *binding != CalyxBinding::LoweringValve);

    match validate_module_rebase_manifest(&manifest) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.modules.module.calyx_bindings");
            assert!(reason.contains("lowering valve"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

#[test]
fn module_outside_shared_editor_namespace_fails_loud() {
    let mut manifest = default_module_rebase_manifest().unwrap();
    manifest.modules[0].mcp_tool_group = "legacy.world.*".to_string();

    match validate_module_rebase_manifest(&manifest) {
        Err(EditorMcpError::InvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.modules.module.capability_layer");
            assert!(reason.contains("shared editor capability"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}
