use std::{collections::BTreeMap, fs, path::Path};

use super::*;
use crate::mcp_object_tools::sample_issue919_document;

#[test]
fn generated_form_edits_gameplay_table_and_persists_raw_unknown_field() {
    let (_run, root) = crate::fsv_support::artifact_dir("issue742_schema_forms", "table").unwrap();
    reset_root(&root);
    let document_path = root.join("object-document-aster");
    let ledger_path = root.join("schema-ledger-aster");
    let descriptor = descriptor().unwrap();

    assert_eq!(descriptor.unknown_field_count, 1);
    assert!(matches!(
        descriptor.fields[3].kind,
        SchemaFieldKind::Raw { .. }
    ));

    let readback = submit_schema_form_edit(
        &descriptor,
        &table_edit(),
        SchemaFormSubmitArgs {
            document_path: document_path.clone(),
            ledger_path,
            ledger_id: "issue742-schema-ledger".to_string(),
            request_id: "issue742-schema-edit".to_string(),
            requested_by: "litd-editor-issue742-test".to_string(),
            initial_document: Some(sample_issue919_document()),
        },
    )
    .unwrap();

    assert_eq!(readback.table_object_id, "gameplay.table.hero_xp_curve_742");
    assert_eq!(readback.document.gameplay_table_count, 1);
    assert_eq!(readback.document.object_kind, "GameplayTable");
    assert_eq!(
        readback.document.object_properties["row.level_2.xp_required"],
        "275"
    );
    assert_eq!(
        readback.document.object_properties["row.level_2.reward_tier"],
        "rare"
    );
    assert_eq!(
        readback.document.object_properties["row.level_2.designer_curve"],
        "curve:v2 control point 0.55"
    );
    assert!(
        readback.document.object_properties["row.level_2.tuning"].contains("\"grant_bonus\":true")
    );
}

#[test]
fn ai_assist_applies_valid_ability_and_fails_closed_for_offline_or_invalid() {
    let (_run, root) = crate::fsv_support::artifact_dir("issue742_schema_forms", "ai").unwrap();
    reset_root(&root);
    let document_path = root.join("object-document-aster");
    let context = issue742_authoring_context();
    let panel = build_ai_assist_panel(AiAssistConnection::Online {
        helper: "m11-grounded-nl-ability".to_string(),
    })
    .unwrap();

    let applied = apply_ai_assist_ability_draft(
        &panel,
        AiAssistApplyArgs {
            document_path: document_path.clone(),
            ledger_path: root.join("ai-ledger-aster"),
            ledger_id: "issue742-ai-ledger".to_string(),
            request: happy_request("issue742-ai-good"),
            initial_document: Some(sample_issue919_document()),
        },
        &context,
    )
    .unwrap();
    assert!(applied.abilitycheck_passed);
    assert_eq!(applied.document.ability_count, 1);
    assert_eq!(applied.document.object_kind, "Ability");
    assert_eq!(
        applied.document.object_properties["ai.admission_code"],
        "LITD-AUTHORING-ADMITTED"
    );
    let before = read_mcp_object_document(&document_path)
        .unwrap()
        .document_fingerprint;

    let offline = build_ai_assist_panel(AiAssistConnection::Offline {
        reason: "M11 route unavailable".to_string(),
    })
    .unwrap();
    let offline_error = apply_ai_assist_ability_draft(
        &offline,
        AiAssistApplyArgs {
            document_path: document_path.clone(),
            ledger_path: root.join("offline-ledger-aster"),
            ledger_id: "issue742-ai-offline".to_string(),
            request: happy_request("issue742-ai-offline"),
            initial_document: None,
        },
        &context,
    )
    .unwrap_err()
    .to_string();
    let after_offline = read_mcp_object_document(&document_path)
        .unwrap()
        .document_fingerprint;
    assert_eq!(before, after_offline);
    assert!(offline_error.contains("offline"));

    let invalid_error = apply_ai_assist_ability_draft(
        &panel,
        AiAssistApplyArgs {
            document_path: document_path.clone(),
            ledger_path: root.join("invalid-ledger-aster"),
            ledger_id: "issue742-ai-invalid".to_string(),
            request: AiAssistAbilityDraftRequest {
                request_id: "issue742-ai-invalid".to_string(),
                prompt: "make a spaceship laser stock market ability".to_string(),
                seed: 424244,
                requested_by: "litd-editor-issue742-test".to_string(),
            },
            initial_document: None,
        },
        &context,
    )
    .unwrap_err()
    .to_string();
    let after_invalid = read_mcp_object_document(&document_path)
        .unwrap()
        .document_fingerprint;
    assert_eq!(before, after_invalid);
    assert!(invalid_error.contains("abilitycheck"));
}

fn descriptor() -> Result<SchemaFormDescriptor, EngineError> {
    reflect_source_form(SourceFormSchema {
        schema_id: "litd.data.hero_xp_curve.issue742".to_string(),
        table_id: "hero_xp_curve_742".to_string(),
        category: "progression".to_string(),
        fields: vec![
            SourceFormField {
                field_id: "xp_required".to_string(),
                label: "XP Required".to_string(),
                source_type: "u32".to_string(),
                required: true,
                min: Some(0),
                max: Some(10_000),
                enum_options: Vec::new(),
                option_fields: Vec::new(),
            },
            SourceFormField {
                field_id: "reward_tier".to_string(),
                label: "Reward Tier".to_string(),
                source_type: "enum".to_string(),
                required: true,
                min: None,
                max: None,
                enum_options: vec!["common".to_string(), "rare".to_string()],
                option_fields: Vec::new(),
            },
            SourceFormField {
                field_id: "tuning".to_string(),
                label: "Tuning".to_string(),
                source_type: "options".to_string(),
                required: false,
                min: None,
                max: None,
                enum_options: Vec::new(),
                option_fields: vec![
                    SourceFormField {
                        field_id: "curve".to_string(),
                        label: "Curve".to_string(),
                        source_type: "text".to_string(),
                        required: true,
                        min: None,
                        max: None,
                        enum_options: Vec::new(),
                        option_fields: Vec::new(),
                    },
                    SourceFormField {
                        field_id: "grant_bonus".to_string(),
                        label: "Grant Bonus".to_string(),
                        source_type: "bool".to_string(),
                        required: true,
                        min: None,
                        max: None,
                        enum_options: Vec::new(),
                        option_fields: Vec::new(),
                    },
                ],
            },
            SourceFormField {
                field_id: "designer_curve".to_string(),
                label: "Designer Curve".to_string(),
                source_type: "piecewise_curve".to_string(),
                required: false,
                min: None,
                max: None,
                enum_options: Vec::new(),
                option_fields: Vec::new(),
            },
        ],
    })
}

fn table_edit() -> SchemaFormEdit {
    let mut values = BTreeMap::new();
    values.insert("xp_required".to_string(), SchemaFormValue::Integer(275));
    values.insert(
        "reward_tier".to_string(),
        SchemaFormValue::Enum("rare".to_string()),
    );
    values.insert(
        "designer_curve".to_string(),
        SchemaFormValue::Raw("curve:v2 control point 0.55".to_string()),
    );
    values.insert("tuning".to_string(), SchemaFormValue::Options(nested()));
    SchemaFormEdit {
        table_id: "hero_xp_curve_742".to_string(),
        category: "progression".to_string(),
        rows: vec![SchemaFormRow {
            row_id: "level_2".to_string(),
            values,
        }],
    }
}

fn nested() -> BTreeMap<String, SchemaFormValue> {
    BTreeMap::from([
        (
            "curve".to_string(),
            SchemaFormValue::Text("soft-cap".to_string()),
        ),
        ("grant_bonus".to_string(), SchemaFormValue::Boolean(true)),
    ])
}

fn happy_request(request_id: &str) -> AiAssistAbilityDraftRequest {
    AiAssistAbilityDraftRequest {
        request_id: request_id.to_string(),
        prompt: "Create a Dawnward cinder bloom ward flame ability for Ashen Reach dusk forest."
            .to_string(),
        seed: 424242,
        requested_by: "litd-editor-issue742-test".to_string(),
    }
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
