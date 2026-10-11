//! #839 quest authoring -> Calyx lowering -> runtime load.
//!
//! Quest DEFINITIONS are editor-authored content, so they follow the doctrine spine
//! (#1312/#1783 generic host consumes lowered data): a [`QuestBook`] validates into a
//! deterministic INTEGER-only JSON payload, persists through the REAL content path
//! (`EngineCalyxContentStore::put_content`, #1791 `pending_measurement` with no ORT,
//! no side stores #1348), lowers to a frozen fingerprinted sim package, and is loaded
//! into a [`QuestLog`] by reading the lowered fields back. The precedent is
//! `object_editor_unit_runtime::load_lowered_unit_toml_into_sim`.

use serde_json::{Value, json};

use crate::EngineError;
use crate::calyx_content::{
    EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
};
use crate::calyx_lowering::{
    EngineLoweredSimFieldValue, EngineLoweredSimPackage, EngineLoweringValve,
};

use super::invalid;
use super::quests::{FlagId, QuestId, QuestLog};

/// Authoring schema tag stamped into every quest-book payload.
pub const QUEST_BOOK_SCHEMA: &str = "litd.game_runtime.quest_book.v1";

/// One authored quest definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestAuthoring {
    pub id: QuestId,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub required: bool,
    pub required_flags: Vec<FlagId>,
}

impl QuestAuthoring {
    fn validate(&self) -> Result<(), EngineError> {
        if self.title.trim().is_empty() {
            return Err(invalid("quest_authoring.title", "title must be non-empty"));
        }
        if self.description.trim().is_empty() {
            return Err(invalid(
                "quest_authoring.description",
                "description must be non-empty",
            ));
        }
        Ok(())
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "title": self.title,
            "description": self.description,
            "icon": self.icon,
            "required": self.required,
            "flag_count": self.required_flags.len(),
            "required_flags": self
                .required_flags
                .iter()
                .map(|flag| json!(flag))
                .collect::<Vec<_>>(),
        })
    }
}

/// A validated set of authored quests ready to persist + lower.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestBook {
    pub id: String,
    pub quests: Vec<QuestAuthoring>,
}

impl QuestBook {
    pub fn new(id: &str, quests: Vec<QuestAuthoring>) -> Result<Self, EngineError> {
        if id.trim().is_empty() {
            return Err(invalid("quest_book.id", "quest book id must be non-empty"));
        }
        if quests.is_empty() {
            return Err(invalid("quest_book.quests", "quest book must not be empty"));
        }
        for quest in &quests {
            quest.validate()?;
        }
        // Duplicate quest ids would collide on load; fail loud at authoring.
        let mut ids: Vec<QuestId> = quests.iter().map(|q| q.id).collect();
        ids.sort_unstable();
        if ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(invalid("quest_book.quests", "duplicate quest id in book"));
        }
        Ok(Self {
            id: id.to_string(),
            quests,
        })
    }

    /// The deterministic integer-only JSON body (schema + count + quests).
    pub fn to_payload(&self) -> Value {
        json!({
            "schema": QUEST_BOOK_SCHEMA,
            "id": self.id,
            "quest_count": self.quests.len(),
            "quests": self.quests.iter().map(QuestAuthoring::to_json).collect::<Vec<_>>(),
        })
    }
}

/// The readback of a persisted + lowered quest book.
#[derive(Debug, Clone)]
pub struct QuestBookReadback {
    pub content_id: String,
    pub cx_id: String,
    pub input_sha256: String,
    pub row_sha256: String,
    pub measurement_state: String,
    pub lowered: EngineLoweredSimPackage,
}

impl QuestBookReadback {
    pub fn evidence(&self) -> Value {
        json!({
            "content_id": self.content_id,
            "cx_id": self.cx_id,
            "input_sha256": self.input_sha256,
            "row_sha256": self.row_sha256,
            "measurement_state": self.measurement_state,
            "lowered_field_count": self.lowered.fields.len(),
            "lowered_package_fingerprint": self.lowered.package_fingerprint,
            "lowered_source_fingerprint": self.lowered.source_fingerprint,
        })
    }
}

/// Persist a quest book through the real content path and lower it to a frozen sim
/// package. Works with NO ORT (pending_measurement per #1791).
pub fn persist_and_lower_quest_book(
    store: &EngineCalyxContentStore,
    author: &str,
    editor_path: &str,
    display_name: &str,
    book: &QuestBook,
) -> Result<QuestBookReadback, EngineError> {
    let payload = book.to_payload();
    let canonical = serde_json::to_vec(&payload)
        .map_err(|error| invalid("quest_book.payload", error.to_string()))?;
    let record = EngineContentRecord {
        kind: EngineContentKind::Trigger,
        id: book.id.clone(),
        display_name: display_name.to_string(),
        payload,
        source: EngineContentSource {
            path: editor_path.to_string(),
            author: author.to_string(),
            source_hash: litd_hash::sha256_hex(&canonical),
        },
        anchors: Vec::new(),
    };
    let row = store
        .put_content(&record)
        .map_err(|error| invalid("quest_book.put_content", error.to_string()))?;
    let package = EngineLoweringValve::lower_content_for_sim(&row)
        .map_err(|error| invalid("quest_book.lower", error.to_string()))?;
    EngineLoweringValve::verify_lowered_package(&package)
        .map_err(|error| invalid("quest_book.verify", error.to_string()))?;
    Ok(QuestBookReadback {
        content_id: row.id.clone(),
        cx_id: row.cx_id.clone(),
        input_sha256: row.input_sha256.clone(),
        row_sha256: row.database.sha256.clone(),
        measurement_state: row.measurement.measurement_state.clone(),
        lowered: package,
    })
}

fn field(package: &EngineLoweredSimPackage, path: &str) -> Result<Value, EngineError> {
    let value = package
        .fields
        .iter()
        .find(|field| field.path == path)
        .ok_or_else(|| invalid("quest_book.lowered", format!("field {path} missing")))?;
    Ok(match &value.value {
        EngineLoweredSimFieldValue::Bool(v) => json!(v),
        EngineLoweredSimFieldValue::I64(v) => json!(v),
        EngineLoweredSimFieldValue::U64(v) => json!(v),
        EngineLoweredSimFieldValue::String(v) => json!(v),
        EngineLoweredSimFieldValue::EmptyArray => json!([]),
        EngineLoweredSimFieldValue::EmptyObject => json!({}),
    })
}

fn field_u64(package: &EngineLoweredSimPackage, path: &str) -> Result<u64, EngineError> {
    match field(package, path)? {
        Value::Number(n) if n.is_u64() => Ok(n.as_u64().unwrap()),
        Value::Number(n) if n.is_i64() && n.as_i64().unwrap() >= 0 => {
            Ok(n.as_i64().unwrap() as u64)
        }
        other => Err(invalid(
            "quest_book.lowered",
            format!("field {path} expected unsigned int, found {other}"),
        )),
    }
}

fn field_str(package: &EngineLoweredSimPackage, path: &str) -> Result<String, EngineError> {
    match field(package, path)? {
        Value::String(s) => Ok(s),
        other => Err(invalid(
            "quest_book.lowered",
            format!("field {path} expected string, found {other}"),
        )),
    }
}

fn field_bool(package: &EngineLoweredSimPackage, path: &str) -> Result<bool, EngineError> {
    match field(package, path)? {
        Value::Bool(b) => Ok(b),
        other => Err(invalid(
            "quest_book.lowered",
            format!("field {path} expected bool, found {other}"),
        )),
    }
}

/// Load every quest in a lowered quest-book package into `log`, reading the lowered
/// fields back by path. Fails loud on any missing/mistyped field (the negative control
/// -- a sabotaged/truncated package cannot silently load an incomplete quest).
pub fn load_quest_book_into_log(
    package: &EngineLoweredSimPackage,
    log: &mut QuestLog,
) -> Result<usize, EngineError> {
    if field_str(package, "/schema")? != QUEST_BOOK_SCHEMA {
        return Err(invalid("quest_book.schema", "unexpected quest-book schema"));
    }
    let quest_count = field_u64(package, "/quest_count")?;
    for i in 0..quest_count {
        let base = format!("/quests/{i}");
        let id = field_u64(package, &format!("{base}/id"))? as QuestId;
        let title = field_str(package, &format!("{base}/title"))?;
        let description = field_str(package, &format!("{base}/description"))?;
        let icon = field_str(package, &format!("{base}/icon"))?;
        let required = field_bool(package, &format!("{base}/required"))?;
        let flag_count = field_u64(package, &format!("{base}/flag_count"))?;
        let mut flags = Vec::new();
        for j in 0..flag_count {
            flags.push(field_u64(package, &format!("{base}/required_flags/{j}"))? as FlagId);
        }
        log.create_quest(id, &title, &description, &icon, required, &flags)?;
    }
    Ok(quest_count as usize)
}
