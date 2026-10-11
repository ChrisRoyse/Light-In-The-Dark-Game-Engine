use crate::{
    history::stable_fingerprint,
    starter_templates::{
        STARTER_TEMPLATE_KINDS, StarterTemplateKind, StarterTerminalResult,
        new_project_from_template, run_starter_template_headless, starter_template,
        starter_template_catalog, validate_starter_template,
    },
    trigger_wasm_authoring::{
        WASM_GUI_SCHEMA, WasmGuiOperation, WasmGuiProgram, lower_gui_program,
    },
};

#[test]
fn catalog_contains_full_wc3_genre_range() {
    let catalog = starter_template_catalog().unwrap();
    assert_eq!(catalog.len(), 10);
    let mut ids = catalog
        .iter()
        .map(|template| template.template_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "starter.hero_arena.sunspire_duel",
            "starter.hero_defense.last_brazier",
            "starter.maze_escape.gloomroot_path",
            "starter.melee_skirmish.frost_crossing",
            "starter.mini_game_party.crystal_dash",
            "starter.moba_aos.dawnward_crossing",
            "starter.orpg.silverleaf_chronicle",
            "starter.rpg_arena.crypt_ring",
            "starter.survival_chaos.obsidian_lanes",
            "starter.tower_defense.cinder_lane",
        ]
    );
    for template in catalog {
        let validation = validate_starter_template(&template).unwrap();
        assert!(validation.supported_data_only);
        assert!(validation.no_wasm_escape);
        assert_eq!(validation.world_validation.error_count, 0);
        assert!(validation.object_count >= 6);
        assert!(validation.unit_count >= 4);
        assert_eq!(validation.tooltip_count, 3);
        assert!(validation.supported_action_counts.contains_key("sleep"));
    }
}

#[test]
fn new_from_template_clone_is_independent() {
    let template = starter_template(StarterTemplateKind::TowerDefense).unwrap();
    let original_fingerprint = stable_fingerprint(&template.document).unwrap();
    let mut clone = new_project_from_template(
        StarterTemplateKind::TowerDefense,
        "issue741.clone.independent",
        "Issue 741 Clone",
        "issue741-clone-rev-1",
    )
    .unwrap();
    let clone_fingerprint = stable_fingerprint(&clone.project.document).unwrap();
    assert_ne!(original_fingerprint, clone_fingerprint);
    assert!(clone.clone_independent);

    clone
        .project
        .document
        .objects
        .get_mut("td_tower_alpha")
        .unwrap()
        .properties
        .insert("stat.damage".to_string(), "9".to_string());
    let edited_clone_fingerprint = stable_fingerprint(&clone.project.document).unwrap();
    assert_ne!(clone_fingerprint, edited_clone_fingerprint);
    assert_eq!(
        template.document.objects["td_tower_alpha"].properties["stat.damage"],
        "4"
    );
}

#[test]
fn every_template_runs_to_terminal_deterministically() {
    for kind in STARTER_TEMPLATE_KINDS.iter().copied() {
        let clone = new_project_from_template(
            kind,
            &format!("issue741.run.{kind:?}").to_ascii_lowercase(),
            kind.default_title(),
            "issue741-run-rev-1",
        )
        .unwrap();
        let report = run_starter_template_headless(&clone, 512).unwrap();
        assert_eq!(
            report.terminal_result,
            StarterTerminalResult::Win,
            "{kind:?}"
        );
        assert!(report.deterministic);
        assert_eq!(report.final_state_hash, report.replay_state_hash);
        assert!(report.final_tick > 0);
        assert!(report.play_spawned_bodies > 0);
        assert!(report.visible_instance_count > 0);
        assert!(report.draw_batches > 0);
        assert!(
            report
                .executed_trigger_ids
                .iter()
                .any(|id| id.contains("victory"))
        );
    }
}

#[test]
fn wasm_escape_template_fails_closed() {
    let mut template = starter_template(StarterTemplateKind::TowerDefense).unwrap();
    template.trigger_project.graphs[0].wasm_escape = Some(
        lower_gui_program(&WasmGuiProgram {
            schema: WASM_GUI_SCHEMA.to_string(),
            script_id: "bad_escape".to_string(),
            entry_point: "run".to_string(),
            fuel_budget: 1_000,
            approved_reason: "not allowed for starters".to_string(),
            operations: vec![WasmGuiOperation::SetFixed { slot: 0, raw: 1 }],
        })
        .unwrap(),
    );
    let error = validate_starter_template(&template)
        .unwrap_err()
        .to_string();
    assert!(error.contains("editor.starter_templates.eca.wasm_escape"));
}
