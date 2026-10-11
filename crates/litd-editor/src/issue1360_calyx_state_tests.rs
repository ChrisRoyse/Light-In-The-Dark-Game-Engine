use litd_engine::{fsv_artifacts::begin_issue_run, render::vfx::write_effect_json};
use serde_json::json;

use crate::{
    mcp_story_tools::mcp_story_tools_store::{
        McpStoryState, McpStoryToolRecord, read_mcp_story_state, read_mcp_story_tool_ledger,
        write_mcp_story_state, write_mcp_story_tool_ledger,
    },
    mcp_trigger_tools::mcp_trigger_tools_store::{
        McpTriggerRuntimeState, McpTriggerToolRecord, read_mcp_sound_record,
        read_mcp_sound_runtime, read_mcp_trigger_state, read_mcp_trigger_tool_ledger,
        write_mcp_sound_record, write_mcp_sound_runtime, write_mcp_trigger_state,
        write_mcp_trigger_tool_ledger,
    },
    mcp_vfx_tools::{DESCRIBE_EFFECT_TOOL, DescribeEffectAssetArgs, call_mcp_vfx_tool},
    sound_editor::build_sound_editor_runtime,
    temporal_content::{
        build_temporal_content_ledger, read_temporal_content_ledger, score_temporal_content,
        write_temporal_content_ledger,
    },
    vfx_library::{
        VfxLibraryAssetInput, build_vfx_library_index, build_vfx_library_ledger, dedup_vfx_library,
        read_vfx_library_ledger, render_vfx_library_preview, search_vfx_library,
        write_vfx_library_ledger,
    },
    vfx_text_generation::{
        VfxTextGenerationRequest, generate_and_store_vfx_text_effect,
        read_vfx_text_generation_ledger,
    },
};

#[path = "issue1360_calyx_state_fixtures.rs"]
mod fixtures;
use fixtures::*;

#[test]
fn issue1360_trigger_story_vfx_sound_defs_live_in_calyx_aster_fsv() {
    let run = begin_issue_run("issue1360_trigger_story_vfx_sound_calyx").unwrap();
    let root = run.run_dir();
    let story_state_dir = root.join("story-state-aster");
    let story_ledger_dir = root.join("story-ledger-aster");
    let trigger_state_dir = root.join("trigger-state-aster");
    let trigger_ledger_dir = root.join("trigger-ledger-aster");
    let sound_record_dir = root.join("sound-record-aster");
    let sound_runtime_dir = root.join("sound-runtime-aster");
    let sound_ledger_dir = root.join("sound-ledger-aster");
    let mcp_vfx_ledger_dir = root.join("mcp-vfx-ledger-aster");
    let vfx_effect_dir = root.join("vfx-effect-aster");
    let vfx_text_legacy = root.join("vfx-text-legacy.json");
    let vfx_text_aster = root.join("vfx-text-aster");
    let vfx_library_legacy = root.join("vfx-library-legacy.json");
    let vfx_library_aster = root.join("vfx-library-aster");
    let temporal_legacy = root.join("temporal-legacy.json");
    let temporal_aster = root.join("temporal-aster");
    let vfx_store = root.join("vfx-content-store");

    let before = json!({
        "story_state": file_state(&story_state_dir),
        "trigger_state": file_state(&trigger_state_dir),
        "sound_record": file_state(&sound_record_dir),
        "mcp_vfx_ledger": file_state(&mcp_vfx_ledger_dir),
        "vfx_text": file_state(&vfx_text_aster),
        "vfx_library": file_state(&vfx_library_aster),
        "temporal": file_state(&temporal_aster),
    });

    let mut story = McpStoryState::new("issue1360-story-state").unwrap();
    story.characters = vec![story_character()];
    story.dialogues = vec![dialogue_tree()];
    story.quests = vec![quest_arc()];
    story.cinematics = vec![cinematic_stage()];
    story.lore_contexts = vec![lore_context()];
    story.update_count = 4;
    let story_write = write_mcp_story_state(&story_state_dir, &story).unwrap();
    let story_read = read_mcp_story_state(&story_state_dir).unwrap();
    assert_eq!(story_read.state, story);
    let story_ledger = write_mcp_story_tool_ledger(
        &story_ledger_dir,
        "issue1360-story-ledger",
        McpStoryToolRecord {
            request_id: "issue1360-story-request".to_string(),
            requested_by: "issue1360-fsv".to_string(),
            tool_name: "litd.editor.story.upsert_dialogue_quest".to_string(),
            operation_id: "editor.story.upsert_dialogue_quest".to_string(),
            state_path: story_state_dir.display().to_string(),
            state_sha256: story_read.sha256.clone(),
            state_bytes: story_read.bytes,
            action_summary: "upserted story/dialogue/quest/cinematic/lore records".to_string(),
            affected_ids: vec![
                "char_elara".to_string(),
                "dlg_gate".to_string(),
                "quest_gate".to_string(),
                "cin_gate".to_string(),
                "lore_gate".to_string(),
            ],
            character_count: story_read.character_count,
            dialogue_count: story_read.dialogue_count,
            quest_count: story_read.quest_count,
            cinematic_count: story_read.cinematic_count,
            lore_context_count: story_read.lore_context_count,
            lore_entry_count: story_read.lore_entry_count,
            lore_entry_link_count: story_read.lore_entry_link_count,
            lore_timeline_marker_count: story_read.lore_timeline_marker_count,
            lore_surface_count: story_read.lore_surface_count,
            lore_surface_lore_ref_count: story_read.lore_surface_lore_ref_count,
            lore_surface_codex_unlock_count: story_read.lore_surface_codex_unlock_count,
        },
    )
    .unwrap();
    assert_eq!(
        read_mcp_story_tool_ledger(&story_ledger_dir)
            .unwrap()
            .ledger
            .record_count,
        1
    );

    let mut trigger = McpTriggerRuntimeState::new("issue1360-trigger-state").unwrap();
    trigger.variables = vec![trigger_variable()];
    trigger.graphs = vec![eca_graph()];
    trigger.runtime_cues = vec![runtime_cue()];
    trigger.update_count = 3;
    let trigger_write = write_mcp_trigger_state(&trigger_state_dir, &trigger).unwrap();
    let trigger_read = read_mcp_trigger_state(&trigger_state_dir).unwrap();
    assert_eq!(trigger_read.state, trigger);
    let trigger_ledger = write_mcp_trigger_tool_ledger(
        &trigger_ledger_dir,
        "issue1360-trigger-ledger",
        McpTriggerToolRecord {
            request_id: "issue1360-trigger-request".to_string(),
            requested_by: "issue1360-fsv".to_string(),
            tool_name: "litd.editor.triggers.upsert_eca_graph".to_string(),
            operation_id: "editor.triggers.upsert_eca_graph".to_string(),
            primary_path: trigger_state_dir.display().to_string(),
            primary_sha256: trigger_read.sha256.clone(),
            primary_bytes: trigger_read.bytes,
            action_summary: "upserted trigger graph and runtime cue".to_string(),
            affected_ids: vec!["trg_gate_open".to_string()],
            variable_count: trigger_read.variable_count,
            graph_count: trigger_read.graph_count,
            runtime_cue_count: trigger_read.runtime_cue_count,
            sound_library_count: 0,
            named_sound_count: 0,
            sound_binding_count: 0,
        },
    )
    .unwrap();
    assert_eq!(
        read_mcp_trigger_tool_ledger(&trigger_ledger_dir)
            .unwrap()
            .ledger
            .record_count,
        1
    );

    let sound_record = sound_record();
    let sound_write = write_mcp_sound_record(&sound_record_dir, &sound_record).unwrap();
    let sound_runtime =
        build_sound_editor_runtime(&sound_record, Some("snd_gate_open"), 120).unwrap();
    let sound_runtime_write = write_mcp_sound_runtime(&sound_runtime_dir, &sound_runtime).unwrap();
    let sound_read = read_mcp_sound_record(&sound_record_dir).unwrap();
    let sound_runtime_read = read_mcp_sound_runtime(&sound_runtime_dir).unwrap();
    assert_eq!(sound_read.record, sound_record);
    assert_eq!(sound_runtime_read.runtime, sound_runtime);
    let sound_ledger = write_mcp_trigger_tool_ledger(
        &sound_ledger_dir,
        "issue1360-sound-ledger",
        McpTriggerToolRecord {
            request_id: "issue1360-sound-request".to_string(),
            requested_by: "issue1360-fsv".to_string(),
            tool_name: "litd.editor.runtime.upsert_sound_variable".to_string(),
            operation_id: "editor.runtime.upsert_sound_variable".to_string(),
            primary_path: sound_record_dir.display().to_string(),
            primary_sha256: sound_read.sha256.clone(),
            primary_bytes: sound_read.bytes,
            action_summary: "persisted sound editor record and preview runtime".to_string(),
            affected_ids: vec!["snd_gate_open".to_string()],
            variable_count: 0,
            graph_count: 0,
            runtime_cue_count: 0,
            sound_library_count: sound_read.library_count,
            named_sound_count: sound_read.named_sound_count,
            sound_binding_count: sound_read.trigger_binding_count,
        },
    )
    .unwrap();

    let vfx_text = generate_and_store_vfx_text_effect(
        &vfx_text_legacy,
        &vfx_text_aster,
        &vfx_store,
        "issue1360-vfx-text-ledger",
        VfxTextGenerationRequest {
            request_id: "issue1360-vfx-text".to_string(),
            prompt: "a fiery explosion with rising embers".to_string(),
            seed: 13_600,
            requested_by: "issue1360-fsv".to_string(),
        },
    )
    .unwrap();
    let vfx_text_read =
        read_vfx_text_generation_ledger(&vfx_text_aster, "issue1360-vfx-text-ledger")
            .unwrap()
            .unwrap();
    assert!(!vfx_text_legacy.exists());

    let mcp_vfx_output = call_mcp_vfx_tool(
        DESCRIBE_EFFECT_TOOL,
        serde_json::to_value(DescribeEffectAssetArgs {
            store_root: vfx_store.clone(),
            ledger_path: mcp_vfx_ledger_dir.clone(),
            ledger_id: "issue1360-mcp-vfx-ledger".to_string(),
            request_id: "issue1360-mcp-vfx".to_string(),
            requested_by: "issue1360-fsv".to_string(),
            prompt: "a lightning bolt strike".to_string(),
            seed: 13_601,
        })
        .unwrap(),
    )
    .unwrap();

    let effect_write = write_effect_json(&vfx_effect_dir, &vfx_text.proposal.effect).unwrap();
    let preview = render_vfx_library_preview(
        &vfx_text.proposal.effect,
        root.join("vfx-library-preview.bmp"),
    )
    .unwrap();
    let library_index = build_vfx_library_index(
        &vfx_store,
        "issue1360-vfx-library",
        vec![VfxLibraryAssetInput {
            asset_id: "issue1360_fire".to_string(),
            descriptor: vfx_text.record.descriptor.clone(),
            preview,
        }],
    )
    .unwrap();
    let library_search = search_vfx_library(
        &library_index,
        &crate::vfx_library::VfxLibrarySearchRequest {
            query_id: "issue1360-search".to_string(),
            query: "fire embers".to_string(),
            limit: 4,
            requested_by: "issue1360-fsv".to_string(),
        },
    )
    .unwrap();
    let library_ledger = build_vfx_library_ledger(
        "issue1360-vfx-library-ledger",
        library_index,
        vec![library_search],
        dedup_vfx_library(
            &build_vfx_library_index(
                &vfx_store,
                "issue1360-vfx-library-dedup",
                vec![VfxLibraryAssetInput {
                    asset_id: "issue1360_fire_dedup".to_string(),
                    descriptor: vfx_text.record.descriptor.clone(),
                    preview: render_vfx_library_preview(
                        &vfx_text.proposal.effect,
                        root.join("vfx-library-preview-dedup.bmp"),
                    )
                    .unwrap(),
                }],
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let library_write =
        write_vfx_library_ledger(&vfx_library_legacy, &vfx_library_aster, &library_ledger).unwrap();
    let library_read = read_vfx_library_ledger(&vfx_library_aster, "issue1360-vfx-library-ledger")
        .unwrap()
        .unwrap();
    assert!(!vfx_library_legacy.exists());

    let temporal_report = score_temporal_content(
        "Calyx Aster temporal-content #1360",
        &[temporal_item("event_gate", 800, Some(1_704_204_000), 1, 4)],
        &temporal_query(),
    )
    .unwrap();
    let temporal_ledger =
        build_temporal_content_ledger("issue1360-temporal-ledger", vec![temporal_report]).unwrap();
    let temporal_write =
        write_temporal_content_ledger(&temporal_legacy, &temporal_aster, &temporal_ledger).unwrap();
    let temporal_read = read_temporal_content_ledger(&temporal_aster, "issue1360-temporal-ledger")
        .unwrap()
        .unwrap();
    assert!(!temporal_legacy.exists());

    let edges = json!({
        "file_path_rejected_as_aster_vault": file_path_edge(root, &story),
        "invalid_sound_record_no_write": invalid_sound_edge(root),
        "ungrounded_vfx_prompt_no_write": ungrounded_vfx_edge(root),
    });
    let aster_dirs = vec![
        ("story_state", story_state_dir.clone()),
        ("story_ledger", story_ledger_dir.clone()),
        ("trigger_state", trigger_state_dir.clone()),
        ("trigger_ledger", trigger_ledger_dir.clone()),
        ("sound_record", sound_record_dir.clone()),
        ("sound_runtime", sound_runtime_dir.clone()),
        ("sound_ledger", sound_ledger_dir.clone()),
        ("mcp_vfx_ledger", mcp_vfx_ledger_dir.clone()),
        ("vfx_effect", vfx_effect_dir.clone()),
        ("vfx_text", vfx_text_aster.clone()),
        ("vfx_library", vfx_library_aster.clone()),
        ("temporal", temporal_aster.clone()),
    ];
    let physical = physical_summary(&aster_dirs);
    let legacy_outputs = legacy_json_outputs(root);
    assert!(legacy_outputs.is_empty(), "{legacy_outputs:?}");

    let report = json!({
        "schema_version": 1,
        "issue": 1360,
        "source_of_truth": {
            "store": "Calyx Aster Graph/Base rows for story, trigger, sound, VFX, and temporal definitions",
            "latest_manifest_path": run.latest_manifest_path(),
            "physical_aster_files": physical,
            "legacy_json_outputs": legacy_outputs,
        },
        "happy_path": {
            "before": before,
            "story": {
                "state_sha256": story_read.sha256,
                "state_bytes": story_read.bytes,
                "character_count": story_read.character_count,
                "dialogue_count": story_read.dialogue_count,
                "quest_count": story_read.quest_count,
                "cinematic_count": story_read.cinematic_count,
                "lore_context_count": story_read.lore_context_count,
                "lore_entry_count": story_read.lore_entry_count,
                "ledger_sha256": story_ledger.sha256,
            },
            "trigger": {
                "state_sha256": trigger_read.sha256,
                "variable_count": trigger_read.variable_count,
                "graph_count": trigger_read.graph_count,
                "runtime_cue_count": trigger_read.runtime_cue_count,
                "ledger_sha256": trigger_ledger.sha256,
            },
            "sound": {
                "record_sha256": sound_write.sha256,
                "runtime_sha256": sound_runtime_write.sha256,
                "ledger_sha256": sound_ledger.sha256,
                "library_count": sound_read.library_count,
                "named_sound_count": sound_read.named_sound_count,
                "trigger_binding_count": sound_read.trigger_binding_count,
                "preview_active": sound_runtime_read.preview_active,
            },
            "vfx": {
                "mcp_output": mcp_vfx_output.result,
                "effect_sha256": effect_write.sha256,
                "text_ledger_sha256": vfx_text_read.sha256,
                "library_ledger_sha256": library_read.sha256,
                "library_asset_count": library_write.asset_count,
            },
            "temporal": {
                "sha256": temporal_read.sha256,
                "report_count": temporal_write.report_count,
                "graph_rows": temporal_read.graph_rows,
            }
        },
        "edges": edges,
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["story"]["dialogue_count"],
        1
    );
    assert_eq!(
        report_readback.json["source_of_truth"]["legacy_json_outputs"],
        json!([])
    );
    println!(
        "ISSUE1360_CALYX_FSV_REPORT path={} bytes={} sha256={} physical_files={} story_sha={} trigger_sha={} sound_sha={} vfx_sha={} temporal_sha={} edge_file={} edge_sound={} edge_vfx={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        report_readback.json["source_of_truth"]["physical_aster_files"]["total_files"],
        story_write.sha256,
        trigger_write.sha256,
        sound_write.sha256,
        effect_write.sha256,
        temporal_read.sha256,
        report_readback.json["edges"]["file_path_rejected_as_aster_vault"]["error"]["code"],
        report_readback.json["edges"]["invalid_sound_record_no_write"]["error"]["code"],
        report_readback.json["edges"]["ungrounded_vfx_prompt_no_write"]["error"]["code"],
    );
}
