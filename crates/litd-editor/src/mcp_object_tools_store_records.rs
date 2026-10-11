use std::collections::BTreeMap;

use calyx_core::{Input, Modality};
use litd_engine::EngineError;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::history::{EditorDocument, EditorObject};

use super::mcp_object_tools_store_validation::validate_single_line;
use super::{LEDGER_ROW_PREFIX, OBJECT_ROW_PREFIX, store_error};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct ObjectStructuredRecord {
    entity_type: String,
    fields: BTreeMap<String, ObjectStructuredValue>,
}

impl ObjectStructuredRecord {
    fn new(
        entity_type: impl Into<String>,
        fields: BTreeMap<String, ObjectStructuredValue>,
    ) -> Result<Self, EngineError> {
        let entity_type = entity_type.into();
        validate_single_line("document.structured_record.entity_type", &entity_type)?;
        if fields.is_empty() {
            return Err(store_error(
                "document.structured_record",
                "object record must contain at least one field",
            ));
        }
        if !matches!(
            fields.get("object_id"),
            Some(ObjectStructuredValue::Text(value)) if !value.trim().is_empty()
        ) {
            return Err(store_error(
                "document.structured_record.object_id",
                "object record must include a non-empty object_id text field",
            ));
        }
        Ok(Self {
            entity_type,
            fields,
        })
    }

    pub(super) fn to_input(&self) -> Result<Input, EngineError> {
        let bytes = serde_json::to_vec(self).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-object-calyx-record",
            reason: format!("serialize object record for Calyx ingest: {error}"),
        })?;
        Ok(Input::new(Modality::Code, bytes)
            .with_pointer(format!("litd://editor/object/{}", self.object_id())))
    }

    fn object_id(&self) -> &str {
        match self.fields.get("object_id") {
            Some(ObjectStructuredValue::Text(value)) => value,
            _ => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ObjectStructuredValue {
    Text(String),
    F64(f64),
    I64(i64),
}

pub(super) fn object_records(
    document: &EditorDocument,
) -> Result<Vec<ObjectStructuredRecord>, EngineError> {
    if document.objects.is_empty() {
        return Err(store_error(
            "document.objects",
            "object document must contain at least one object for Calyx ingest",
        ));
    }
    let records = document
        .objects
        .values()
        .filter(|object| object.kind != "World")
        .map(object_record_fields)
        .collect::<Result<Vec<_>, _>>()?;
    if records.is_empty() {
        return Err(store_error(
            "document.objects",
            "object document must contain at least one supported gameplay object for Calyx ingest",
        ));
    }
    Ok(records)
}

fn object_record_fields(object: &EditorObject) -> Result<ObjectStructuredRecord, EngineError> {
    let mut fields = BTreeMap::new();
    fields.insert(
        "object_id".to_string(),
        ObjectStructuredValue::Text(object.id.clone()),
    );
    fields.insert(
        "kind".to_string(),
        ObjectStructuredValue::Text(canonical_structured_object_kind(object)?),
    );
    fields.insert(
        "display_name".to_string(),
        ObjectStructuredValue::Text(
            object
                .properties
                .get("object.display_name")
                .cloned()
                .unwrap_or_else(|| object.id.clone()),
        ),
    );
    fields.insert(
        "stats".to_string(),
        ObjectStructuredValue::Text(stats_signature(object)),
    );
    fields.insert(
        "cost".to_string(),
        ObjectStructuredValue::F64(object_cost(object)),
    );
    fields.insert(
        "tech_tree_refs".to_string(),
        ObjectStructuredValue::Text(tech_refs_signature(object)),
    );
    fields.insert(
        "spawn_time".to_string(),
        ObjectStructuredValue::I64(object_spawn_time(object)?),
    );
    ObjectStructuredRecord::new("game_object", fields)
}

fn canonical_structured_object_kind(object: &EditorObject) -> Result<String, EngineError> {
    let canonical = match object.kind.as_str() {
        "Unit" | "Hero" | "Destructible" | "Doodad" => "unit",
        // #822 commerce records (recipe/shop/drop-table) are item-economy records and map to the
        // Calyx "item" structured bucket alongside item definitions.
        "Item" | "Recipe" | "Shop" | "DropTable" => "item",
        "Ability" | "Buff" => "ability",
        "Upgrade" | "Tech" | "GameInterface" | "GameplayConstant" | "GameplayTable" => "upgrade",
        other => {
            return Err(store_error(
                "document.structured_kind",
                format!(
                    "object {} kind {other} cannot be represented by Calyx structured object buckets [unit,item,ability,upgrade]",
                    object.id
                ),
            ));
        }
    };
    Ok(canonical.to_string())
}

fn stats_signature(object: &EditorObject) -> String {
    let stats = object
        .properties
        .iter()
        .filter(|(key, _)| {
            key.starts_with("stats.") || key.starts_with("combat.") || key.starts_with("ability.")
        })
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    if stats.is_empty() {
        "none".to_string()
    } else {
        stats.join("|")
    }
}

fn object_cost(object: &EditorObject) -> f64 {
    object
        .properties
        .iter()
        .find_map(|(key, value)| {
            (key.contains("cost") || key.contains("gold") || key.contains("lumber"))
                .then(|| value.parse::<f64>().ok())
                .flatten()
        })
        .unwrap_or(0.0)
}

fn tech_refs_signature(object: &EditorObject) -> String {
    let refs = object
        .properties
        .iter()
        .filter(|(key, _)| key.contains("tech") || key.contains("requires"))
        .flat_map(|(_, value)| {
            value
                .split(['|', ',', ';', ' '])
                .map(str::trim)
                .filter(|value| !value.is_empty() && *value != "none")
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    refs.join("|")
}

fn object_spawn_time(object: &EditorObject) -> Result<i64, EngineError> {
    const BASE_SPAWN_TIME: i64 = 1_358_000;
    const SPAWN_TIME_RANGE_SECS: u64 = 10_000_000;
    if let Some(value) = object
        .properties
        .get("spawn_time")
        .or_else(|| object.properties.get("object.spawn_time"))
        .or_else(|| object.properties.get("temporal.spawn_time"))
    {
        return value.parse::<i64>().map_err(|error| {
            store_error(
                "document.structured_spawn_time",
                format!(
                    "object {} spawn_time is not an i64 Unix-seconds value: {error}",
                    object.id
                ),
            )
        });
    }
    let digest = Sha256::digest(object.id.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    let offset = i64::try_from(u64::from_le_bytes(bytes) % SPAWN_TIME_RANGE_SECS)
        .expect("bounded spawn_time offset fits i64");
    Ok(BASE_SPAWN_TIME.saturating_add(offset))
}

pub(super) fn object_row_key(object: &EditorObject) -> Vec<u8> {
    format!(
        "{}{}:{}",
        String::from_utf8_lossy(OBJECT_ROW_PREFIX),
        object.kind,
        object.id
    )
    .into_bytes()
}

pub(super) fn ledger_row_key(ledger_id: &str) -> Vec<u8> {
    format!(
        "{}{}",
        String::from_utf8_lossy(LEDGER_ROW_PREFIX),
        ledger_id
    )
    .into_bytes()
}
