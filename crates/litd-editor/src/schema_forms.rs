//! Schema-reflected object/data forms and the editor AI-assist draft bridge.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    history::{EditorDocument, stable_fingerprint},
    mcp_object_tools::{
        UPSERT_GAMEPLAY_TABLES_TOOL, call_mcp_object_tool, read_mcp_object_document,
        read_mcp_object_tool_ledger,
    },
};

#[path = "schema_forms_ai.rs"]
mod schema_forms_ai;
#[path = "schema_forms_values.rs"]
mod schema_forms_values;

pub use schema_forms_ai::{
    AiAssistAbilityApplyReadback, AiAssistAbilityDraftRequest, AiAssistApplyArgs,
    AiAssistAuthoringContext, AiAssistConnection, AiAssistPanel, apply_ai_assist_ability_draft,
    build_ai_assist_panel, issue742_authoring_context,
};
use schema_forms_values::{gameplay_row, validate_form_edit};

pub const SCHEMA_FORMS_SCHEMA: &str = "litd.editor.schema_forms.v1";
pub const SCHEMA_FORMS_SOURCE_ISSUE: u32 = 742;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFormSchema {
    pub schema_id: String,
    pub table_id: String,
    pub category: String,
    pub fields: Vec<SourceFormField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFormField {
    pub field_id: String,
    pub label: String,
    pub source_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(default)]
    pub enum_options: Vec<String>,
    #[serde(default)]
    pub option_fields: Vec<SourceFormField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaFormDescriptor {
    pub schema: String,
    pub source_issue: u32,
    pub schema_id: String,
    pub table_id: String,
    pub category: String,
    pub source_of_truth: String,
    pub fields: Vec<SchemaFieldDescriptor>,
    pub unknown_field_count: usize,
    pub descriptor_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaFieldDescriptor {
    pub field_id: String,
    pub label: String,
    pub required: bool,
    pub kind: SchemaFieldKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SchemaFieldKind {
    Text,
    Integer { min: Option<i64>, max: Option<i64> },
    Boolean,
    Enum { options: Vec<String> },
    OptionsStruct { fields: Vec<SchemaFieldDescriptor> },
    Raw { source_type: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaFormEdit {
    pub table_id: String,
    pub category: String,
    pub rows: Vec<SchemaFormRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaFormRow {
    pub row_id: String,
    pub values: BTreeMap<String, SchemaFormValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaFormValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    Enum(String),
    Options(BTreeMap<String, SchemaFormValue>),
    Raw(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaFormSubmitArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_document: Option<EditorDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaFormSubmitReadback {
    pub table_object_id: String,
    pub descriptor_fingerprint: String,
    pub edit_fingerprint: String,
    pub document: ObjectDataReadbackSummary,
    pub ledger: ObjectDataLedgerSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectDataReadbackSummary {
    pub path: String,
    pub sha256: String,
    pub document_fingerprint: String,
    pub object_count: usize,
    pub gameplay_table_count: usize,
    pub ability_count: usize,
    pub kind_counts: BTreeMap<String, usize>,
    pub object_id: String,
    pub object_kind: String,
    pub object_properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectDataLedgerSummary {
    pub path: String,
    pub sha256: String,
    pub ledger_id: String,
    pub record_count: usize,
}

pub fn reflect_source_form(source: SourceFormSchema) -> Result<SchemaFormDescriptor, EngineError> {
    validate_key_component("source.table_id", &source.table_id)?;
    validate_single_line("source.schema_id", &source.schema_id)?;
    validate_single_line("source.category", &source.category)?;
    if source.fields.is_empty() {
        return invalid(
            "source.fields",
            "source form must expose at least one field",
        );
    }
    let mut seen = BTreeSet::new();
    let mut fields = Vec::with_capacity(source.fields.len());
    for field in source.fields {
        if !seen.insert(field.field_id.clone()) {
            return invalid("source.fields", "duplicate source field id");
        }
        fields.push(reflect_field(field)?);
    }
    let mut descriptor = SchemaFormDescriptor {
        schema: SCHEMA_FORMS_SCHEMA.to_string(),
        source_issue: SCHEMA_FORMS_SOURCE_ISSUE,
        schema_id: source.schema_id,
        table_id: source.table_id,
        category: source.category,
        source_of_truth: "litd/data reflected source forms -> object-data MCP table rows"
            .to_string(),
        unknown_field_count: fields.iter().map(count_raw_fields).sum(),
        fields,
        descriptor_fingerprint: String::new(),
    };
    descriptor.descriptor_fingerprint = stable_fingerprint(&descriptor)?;
    Ok(descriptor)
}

pub fn submit_schema_form_edit(
    descriptor: &SchemaFormDescriptor,
    edit: &SchemaFormEdit,
    args: SchemaFormSubmitArgs,
) -> Result<SchemaFormSubmitReadback, EngineError> {
    validate_form_edit(descriptor, edit)?;
    validate_submit_args(&args)?;
    let rows = edit
        .rows
        .iter()
        .map(|row| gameplay_row(descriptor, row))
        .collect::<Result<Vec<_>, _>>()?;
    call_mcp_object_tool(
        UPSERT_GAMEPLAY_TABLES_TOOL,
        json!({
            "document_path": args.document_path,
            "ledger_path": args.ledger_path,
            "ledger_id": args.ledger_id,
            "request_id": args.request_id,
            "requested_by": args.requested_by,
            "initial_document": args.initial_document,
            "tables": [{
                "table_id": edit.table_id,
                "category": edit.category,
                "rows": rows
            }]
        }),
    )?;
    let object_id = gameplay_table_object_id(&edit.table_id);
    Ok(SchemaFormSubmitReadback {
        table_object_id: object_id.clone(),
        descriptor_fingerprint: descriptor.descriptor_fingerprint.clone(),
        edit_fingerprint: stable_fingerprint(edit)?,
        document: object_summary(&args.document_path, &object_id)?,
        ledger: ledger_summary(&args.ledger_path)?,
    })
}

fn reflect_field(field: SourceFormField) -> Result<SchemaFieldDescriptor, EngineError> {
    validate_key_component("field.field_id", &field.field_id)?;
    validate_single_line("field.label", &field.label)?;
    validate_single_line("field.source_type", &field.source_type)?;
    let source_type = field.source_type.to_ascii_lowercase();
    let kind = match source_type.as_str() {
        "text" | "string" => SchemaFieldKind::Text,
        "u32" => SchemaFieldKind::Integer {
            min: Some(field.min.unwrap_or(0)),
            max: field.max,
        },
        "i32" | "integer" => SchemaFieldKind::Integer {
            min: field.min,
            max: field.max,
        },
        "bool" | "boolean" => SchemaFieldKind::Boolean,
        "enum" => {
            validate_options(&field.enum_options)?;
            SchemaFieldKind::Enum {
                options: field.enum_options,
            }
        }
        "options" | "options_struct" => {
            if field.option_fields.is_empty() {
                return invalid("field.options", "options structs require nested fields");
            }
            let mut seen = BTreeSet::new();
            let mut nested = Vec::new();
            for child in field.option_fields {
                if !seen.insert(child.field_id.clone()) {
                    return invalid("field.options", "duplicate nested field id");
                }
                nested.push(reflect_field(child)?);
            }
            SchemaFieldKind::OptionsStruct { fields: nested }
        }
        _ => SchemaFieldKind::Raw {
            source_type: field.source_type,
        },
    };
    Ok(SchemaFieldDescriptor {
        field_id: field.field_id,
        label: field.label,
        required: field.required,
        kind,
    })
}

pub(super) fn object_summary(
    document_path: &PathBuf,
    object_id: &str,
) -> Result<ObjectDataReadbackSummary, EngineError> {
    let readback = read_mcp_object_document(document_path)?;
    let object = readback.document.objects.get(object_id).ok_or_else(|| {
        schema_error(
            "readback.object",
            format!("missing persisted object {object_id}"),
        )
    })?;
    Ok(ObjectDataReadbackSummary {
        path: readback.path,
        sha256: readback.sha256,
        document_fingerprint: readback.document_fingerprint,
        object_count: readback.object_count,
        gameplay_table_count: readback.gameplay_table_count,
        ability_count: readback.ability_count,
        kind_counts: readback.kind_counts,
        object_id: object_id.to_string(),
        object_kind: object.kind.clone(),
        object_properties: object.properties.clone(),
    })
}

pub(super) fn ledger_summary(
    ledger_path: &PathBuf,
) -> Result<ObjectDataLedgerSummary, EngineError> {
    let readback = read_mcp_object_tool_ledger(ledger_path)?;
    Ok(ObjectDataLedgerSummary {
        path: readback.path,
        sha256: readback.sha256,
        ledger_id: readback.ledger.ledger_id,
        record_count: readback.ledger.record_count,
    })
}

pub(super) fn validate_submit_args(args: &SchemaFormSubmitArgs) -> Result<(), EngineError> {
    validate_single_line("ledger_id", &args.ledger_id)?;
    validate_single_line("request_id", &args.request_id)?;
    validate_single_line("requested_by", &args.requested_by)?;
    if args.document_path.as_os_str().is_empty() || args.ledger_path.as_os_str().is_empty() {
        return invalid("paths", "document and ledger paths must be non-empty");
    }
    Ok(())
}

fn validate_options(options: &[String]) -> Result<(), EngineError> {
    if options.is_empty() {
        return invalid("field.enum", "enum fields require options");
    }
    let mut seen = BTreeSet::new();
    for option in options {
        validate_single_line("field.enum", option)?;
        if !seen.insert(option.as_str()) {
            return invalid("field.enum", "duplicate enum option");
        }
    }
    Ok(())
}

fn count_raw_fields(field: &SchemaFieldDescriptor) -> usize {
    match &field.kind {
        SchemaFieldKind::Raw { .. } => 1,
        SchemaFieldKind::OptionsStruct { fields } => fields.iter().map(count_raw_fields).sum(),
        _ => 0,
    }
}

fn gameplay_table_object_id(table_id: &str) -> String {
    format!("gameplay.table.{table_id}")
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 4096 || value.contains(['\n', '\r', '\t']) {
        return invalid(field, "value must be non-empty bounded single-line text");
    }
    Ok(())
}

pub(super) fn validate_key_component(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return invalid(
            field,
            "key components must use ASCII letters, digits, hyphen, or underscore",
        );
    }
    Ok(())
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(schema_error(field, reason))
}

pub(super) fn schema_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.schema_forms.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "schema_forms_tests.rs"]
mod tests;
