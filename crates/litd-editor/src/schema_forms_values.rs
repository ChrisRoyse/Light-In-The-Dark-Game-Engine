use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde_json::{Value, json};

use super::{
    SCHEMA_FORMS_SCHEMA, SCHEMA_FORMS_SOURCE_ISSUE, SchemaFieldDescriptor, SchemaFieldKind,
    SchemaFormDescriptor, SchemaFormEdit, SchemaFormRow, SchemaFormValue, invalid, schema_error,
    validate_key_component, validate_single_line,
};

pub(super) fn validate_form_edit(
    descriptor: &SchemaFormDescriptor,
    edit: &SchemaFormEdit,
) -> Result<(), EngineError> {
    if descriptor.schema != SCHEMA_FORMS_SCHEMA
        || descriptor.source_issue != SCHEMA_FORMS_SOURCE_ISSUE
        || edit.table_id != descriptor.table_id
        || edit.category != descriptor.category
        || edit.rows.is_empty()
    {
        return invalid("edit", "descriptor identity and edit target must match");
    }
    let fields = descriptor
        .fields
        .iter()
        .map(|field| (&field.field_id, field))
        .collect::<BTreeMap<_, _>>();
    let mut rows = BTreeSet::new();
    for row in &edit.rows {
        validate_key_component("row.row_id", &row.row_id)?;
        if !rows.insert(row.row_id.as_str()) {
            return invalid("row.row_id", "duplicate row id");
        }
        for field in &descriptor.fields {
            if field.required && !row.values.contains_key(&field.field_id) {
                return invalid(
                    "row.required",
                    format!("missing required field {}", field.field_id),
                );
            }
        }
        for (field_id, value) in &row.values {
            let field = fields.get(field_id).ok_or_else(|| {
                schema_error("row.field", format!("unknown form field {field_id}"))
            })?;
            validate_value(&field.kind, value)?;
        }
    }
    Ok(())
}

pub(super) fn gameplay_row(
    descriptor: &SchemaFormDescriptor,
    row: &SchemaFormRow,
) -> Result<Value, EngineError> {
    let fields = descriptor
        .fields
        .iter()
        .map(|field| (&field.field_id, field))
        .collect::<BTreeMap<_, _>>();
    let mut cells = BTreeMap::new();
    for (field_id, value) in &row.values {
        let field = fields
            .get(field_id)
            .ok_or_else(|| schema_error("row.field", format!("unknown form field {field_id}")))?;
        cells.insert(field_id.clone(), value_to_cell(&field.kind, value)?);
    }
    Ok(json!({ "row_id": row.row_id, "fields": cells }))
}

fn validate_value(kind: &SchemaFieldKind, value: &SchemaFormValue) -> Result<(), EngineError> {
    match (kind, value) {
        (SchemaFieldKind::Text, SchemaFormValue::Text(text))
        | (SchemaFieldKind::Raw { .. }, SchemaFormValue::Raw(text)) => {
            validate_single_line("value", text)
        }
        (SchemaFieldKind::Integer { min, max }, SchemaFormValue::Integer(value)) => {
            if min.is_some_and(|min| *value < min) || max.is_some_and(|max| *value > max) {
                return invalid(
                    "value.integer",
                    "integer value is outside descriptor bounds",
                );
            }
            Ok(())
        }
        (SchemaFieldKind::Boolean, SchemaFormValue::Boolean(_)) => Ok(()),
        (SchemaFieldKind::Enum { options }, SchemaFormValue::Enum(value)) => {
            if options.iter().any(|option| option == value) {
                Ok(())
            } else {
                invalid(
                    "value.enum",
                    format!("{value} is not a declared enum option"),
                )
            }
        }
        (SchemaFieldKind::OptionsStruct { fields }, SchemaFormValue::Options(values)) => {
            validate_options_values(fields, values)
        }
        _ => invalid(
            "value.kind",
            "form value type does not match field descriptor",
        ),
    }
}

fn value_to_cell(kind: &SchemaFieldKind, value: &SchemaFormValue) -> Result<String, EngineError> {
    validate_value(kind, value)?;
    match value {
        SchemaFormValue::Text(text) | SchemaFormValue::Raw(text) | SchemaFormValue::Enum(text) => {
            Ok(text.clone())
        }
        SchemaFormValue::Integer(value) => Ok(value.to_string()),
        SchemaFormValue::Boolean(value) => Ok(value.to_string()),
        SchemaFormValue::Options(values) => {
            let SchemaFieldKind::OptionsStruct { fields } = kind else {
                return invalid("value.options", "options value requires options descriptor");
            };
            Ok(
                serde_json::to_string(&options_json(fields, values)?).map_err(|error| {
                    schema_error(
                        "value.options_json",
                        format!("encode options struct: {error}"),
                    )
                })?,
            )
        }
    }
}

fn validate_options_values(
    fields: &[SchemaFieldDescriptor],
    values: &BTreeMap<String, SchemaFormValue>,
) -> Result<(), EngineError> {
    let field_map = fields
        .iter()
        .map(|field| (&field.field_id, field))
        .collect::<BTreeMap<_, _>>();
    for field in fields {
        if field.required && !values.contains_key(&field.field_id) {
            return invalid(
                "value.options",
                format!("missing nested field {}", field.field_id),
            );
        }
    }
    for (id, value) in values {
        let field = field_map
            .get(id)
            .ok_or_else(|| schema_error("value.options", format!("unknown nested field {id}")))?;
        validate_value(&field.kind, value)?;
    }
    Ok(())
}

fn options_json(
    fields: &[SchemaFieldDescriptor],
    values: &BTreeMap<String, SchemaFormValue>,
) -> Result<Value, EngineError> {
    validate_options_values(fields, values)?;
    let field_map = fields
        .iter()
        .map(|field| (&field.field_id, field))
        .collect::<BTreeMap<_, _>>();
    let mut object = serde_json::Map::new();
    for (id, value) in values {
        let field = field_map.get(id).expect("validated nested field");
        object.insert(id.clone(), value_json(&field.kind, value)?);
    }
    Ok(Value::Object(object))
}

fn value_json(kind: &SchemaFieldKind, value: &SchemaFormValue) -> Result<Value, EngineError> {
    validate_value(kind, value)?;
    Ok(match value {
        SchemaFormValue::Text(text) | SchemaFormValue::Raw(text) | SchemaFormValue::Enum(text) => {
            Value::String(text.clone())
        }
        SchemaFormValue::Integer(value) => Value::Number((*value).into()),
        SchemaFormValue::Boolean(value) => Value::Bool(*value),
        SchemaFormValue::Options(values) => {
            let SchemaFieldKind::OptionsStruct { fields } = kind else {
                return invalid("value.options", "options value requires options descriptor");
            };
            options_json(fields, values)?
        }
    })
}
