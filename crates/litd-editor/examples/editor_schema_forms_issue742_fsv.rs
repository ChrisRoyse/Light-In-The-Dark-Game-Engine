use std::{collections::BTreeMap, fs, path::PathBuf};

use litd_editor::{
    mcp_object_tools::{read_mcp_object_document, sample_issue919_document},
    schema_forms::{
        AiAssistAbilityDraftRequest, AiAssistApplyArgs, AiAssistConnection, SchemaFieldKind,
        SchemaFormEdit, SchemaFormRow, SchemaFormSubmitArgs, SchemaFormValue, SourceFormField,
        SourceFormSchema, apply_ai_assist_ability_draft, build_ai_assist_panel,
        issue742_authoring_context, reflect_source_form, submit_schema_form_edit,
    },
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE742_FSV_ROOT",
        "issue742_schema_forms_ai_assist",
        "manual",
    )?;
    let paths = Paths::new(root);
    let descriptor = reflect_source_form(source_schema())?;
    let unknown_degraded_to_raw = matches!(descriptor.fields[3].kind, SchemaFieldKind::Raw { .. })
        && descriptor.unknown_field_count == 1;
    let table_readback = submit_schema_form_edit(
        &descriptor,
        &table_edit(),
        SchemaFormSubmitArgs {
            document_path: paths.document.clone(),
            ledger_path: paths.table_ledger.clone(),
            ledger_id: "issue742-schema-table-ledger".to_string(),
            request_id: "issue742-schema-table-edit".to_string(),
            requested_by: "litd-editor-issue742-fsv".to_string(),
            initial_document: Some(sample_issue919_document()),
        },
    )?;
    let table_persisted = table_readback.document.gameplay_table_count == 1
        && table_readback.document.object_properties["row.level_2.xp_required"] == "275"
        && table_readback.document.object_properties["row.level_2.reward_tier"] == "rare";

    let context = issue742_authoring_context();
    let panel = build_ai_assist_panel(AiAssistConnection::Online {
        helper: "m11-grounded-nl-ability".to_string(),
    })?;
    let ai_readback = apply_ai_assist_ability_draft(
        &panel,
        AiAssistApplyArgs {
            document_path: paths.document.clone(),
            ledger_path: paths.ai_ledger.clone(),
            ledger_id: "issue742-ai-ledger".to_string(),
            request: happy_request("issue742-ai-good"),
            initial_document: None,
        },
        &context,
    )?;
    let ai_landed = ai_readback.abilitycheck_passed
        && ai_readback.document.ability_count == 1
        && ai_readback.document.object_kind == "Ability"
        && ai_readback.document.object_properties["ai.admission_code"] == "LITD-AUTHORING-ADMITTED";

    let before_edges = read_mcp_object_document(&paths.document)?.document_fingerprint;
    let offline_panel = build_ai_assist_panel(AiAssistConnection::Offline {
        reason: "M11 route unavailable".to_string(),
    })?;
    let offline_error = apply_ai_assist_ability_draft(
        &offline_panel,
        AiAssistApplyArgs {
            document_path: paths.document.clone(),
            ledger_path: paths.offline_ledger.clone(),
            ledger_id: "issue742-ai-offline".to_string(),
            request: happy_request("issue742-ai-offline"),
            initial_document: None,
        },
        &context,
    )
    .unwrap_err()
    .to_string();
    let after_offline = read_mcp_object_document(&paths.document)?.document_fingerprint;

    let invalid_error = apply_ai_assist_ability_draft(
        &panel,
        AiAssistApplyArgs {
            document_path: paths.document.clone(),
            ledger_path: paths.invalid_ledger.clone(),
            ledger_id: "issue742-ai-invalid".to_string(),
            request: AiAssistAbilityDraftRequest {
                request_id: "issue742-ai-invalid".to_string(),
                prompt: "make a spaceship laser stock market ability".to_string(),
                seed: 424244,
                requested_by: "litd-editor-issue742-fsv".to_string(),
            },
            initial_document: None,
        },
        &context,
    )
    .unwrap_err()
    .to_string();
    let after_invalid = read_mcp_object_document(&paths.document)?.document_fingerprint;
    let edges_passed = unknown_degraded_to_raw
        && before_edges == after_offline
        && before_edges == after_invalid
        && offline_error.contains("offline")
        && invalid_error.contains("abilitycheck");
    let accepted = table_persisted && ai_landed && edges_passed;
    if !accepted {
        return Err("issue742 FSV acceptance predicates failed".into());
    }

    let report = json!({
        "schema": "litd.issue742.schema_forms_ai_assist_fsv.v1",
        "issue": 742,
        "source_of_truth": "object-data MCP Aster document rows, schema descriptor fingerprint, AI lowered ability TOML abilitycheck, and object-editor readback",
        "readme_references_inspected": [
            "docs/readme/ED03-object-editor.png",
            "docs/readme/ED05-ai-coauthor.png"
        ],
        "paths": paths.to_json(),
        "acceptance": {
            "table_persisted": table_persisted,
            "ai_landed": ai_landed,
            "edges_passed": edges_passed,
            "unknown_degraded_to_raw": unknown_degraded_to_raw,
            "offline_disabled": before_edges == after_offline,
            "invalid_ai_rejected_by_abilitycheck": before_edges == after_invalid
        },
        "schema_form": {
            "descriptor_fingerprint": descriptor.descriptor_fingerprint,
            "unknown_field_count": descriptor.unknown_field_count,
            "table_object_id": table_readback.table_object_id,
            "document_sha256": table_readback.document.sha256,
            "document_fingerprint": table_readback.document.document_fingerprint,
            "table_properties": table_readback.document.object_properties
        },
        "ai_assist": {
            "panel": ai_readback.panel,
            "object_id": ai_readback.object_id,
            "lowered_fingerprint": ai_readback.lowered_fingerprint,
            "admission_code": ai_readback.outcome.admission_code,
            "document_sha256": ai_readback.document.sha256,
            "object_properties": ai_readback.document.object_properties
        },
        "edges": [
            {
                "edge": "unknown_field_type_degrades_to_raw",
                "passed": unknown_degraded_to_raw,
                "source_type": "piecewise_curve"
            },
            {
                "edge": "ai_offline_disables_panel_without_mutation",
                "passed": before_edges == after_offline,
                "error": offline_error
            },
            {
                "edge": "invalid_ai_draft_rejected_by_abilitycheck",
                "passed": before_edges == after_invalid,
                "error": invalid_error
            }
        ]
    });
    let readback = run.write_json_artifact("manual/issue742-schema-forms-fsv.json", &report)?;
    println!(
        "ISSUE742_SCHEMA_FORMS_FSV report={} sha256={} table_persisted={} ai_landed={} edges_passed={}",
        readback.path.display(),
        readback.sha256,
        table_persisted,
        ai_landed,
        edges_passed
    );
    Ok(())
}

fn source_schema() -> SourceFormSchema {
    SourceFormSchema {
        schema_id: "litd.data.hero_xp_curve.issue742".to_string(),
        table_id: "hero_xp_curve_742".to_string(),
        category: "progression".to_string(),
        fields: vec![
            source_field(
                "xp_required",
                "XP Required",
                "u32",
                true,
                Some(0),
                Some(10_000),
            ),
            enum_field("reward_tier", "Reward Tier", &["common", "rare"]),
            options_field(),
            source_field(
                "designer_curve",
                "Designer Curve",
                "piecewise_curve",
                false,
                None,
                None,
            ),
        ],
    }
}

fn source_field(
    field_id: &str,
    label: &str,
    source_type: &str,
    required: bool,
    min: Option<i64>,
    max: Option<i64>,
) -> SourceFormField {
    SourceFormField {
        field_id: field_id.to_string(),
        label: label.to_string(),
        source_type: source_type.to_string(),
        required,
        min,
        max,
        enum_options: Vec::new(),
        option_fields: Vec::new(),
    }
}

fn enum_field(field_id: &str, label: &str, options: &[&str]) -> SourceFormField {
    SourceFormField {
        field_id: field_id.to_string(),
        label: label.to_string(),
        source_type: "enum".to_string(),
        required: true,
        min: None,
        max: None,
        enum_options: options.iter().map(|option| option.to_string()).collect(),
        option_fields: Vec::new(),
    }
}

fn options_field() -> SourceFormField {
    SourceFormField {
        field_id: "tuning".to_string(),
        label: "Tuning".to_string(),
        source_type: "options".to_string(),
        required: false,
        min: None,
        max: None,
        enum_options: Vec::new(),
        option_fields: vec![
            source_field("curve", "Curve", "text", true, None, None),
            source_field("grant_bonus", "Grant Bonus", "bool", true, None, None),
        ],
    }
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
    values.insert(
        "tuning".to_string(),
        SchemaFormValue::Options(BTreeMap::from([
            (
                "curve".to_string(),
                SchemaFormValue::Text("soft-cap".to_string()),
            ),
            ("grant_bonus".to_string(), SchemaFormValue::Boolean(true)),
        ])),
    );
    SchemaFormEdit {
        table_id: "hero_xp_curve_742".to_string(),
        category: "progression".to_string(),
        rows: vec![SchemaFormRow {
            row_id: "level_2".to_string(),
            values,
        }],
    }
}

fn happy_request(request_id: &str) -> AiAssistAbilityDraftRequest {
    AiAssistAbilityDraftRequest {
        request_id: request_id.to_string(),
        prompt: "Create a Dawnward cinder bloom ward flame ability for Ashen Reach dusk forest."
            .to_string(),
        seed: 424242,
        requested_by: "litd-editor-issue742-fsv".to_string(),
    }
}

#[derive(Debug, Clone)]
struct Paths {
    root: PathBuf,
    document: PathBuf,
    table_ledger: PathBuf,
    ai_ledger: PathBuf,
    offline_ledger: PathBuf,
    invalid_ledger: PathBuf,
}

impl Paths {
    fn new(root: PathBuf) -> Self {
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale issue742 FSV root");
        }
        fs::create_dir_all(&root).expect("create issue742 FSV root");
        Self {
            document: root.join("object-document-aster"),
            table_ledger: root.join("table-ledger-aster"),
            ai_ledger: root.join("ai-ledger-aster"),
            offline_ledger: root.join("offline-ledger-aster"),
            invalid_ledger: root.join("invalid-ledger-aster"),
            root,
        }
    }

    fn to_json(&self) -> serde_json::Value {
        json!({
            "root": self.root,
            "document": self.document,
            "table_ledger": self.table_ledger,
            "ai_ledger": self.ai_ledger,
            "offline_ledger": self.offline_ledger,
            "invalid_ledger": self.invalid_ledger
        })
    }
}
