//! Engine/editor database facade backed by Calyx Aster rows.
//!
//! This module keeps durable engine data in Aster while preserving the lower
//! level `calyx_json_store` read-after-write checks.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::calyx_json_store::{
    CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes,
};

pub const ENGINE_CALYX_DATABASE_RECORD_SCHEMA: &str = "litd.engine.calyx_database_record.v1";
pub const ENGINE_CALYX_DATABASE_READBACK_SCHEMA: &str = "litd.engine.calyx_database_readback.v1";
pub const ENGINE_CALYX_DATABASE_VAULT_ID: &str = "01KW55180E4DJ22F61AYQRNZS3";
pub const ENGINE_CALYX_DATABASE_PURPOSE: &[u8] = b"litd-engine-editor-database-issue1115";
pub const ENGINE_CALYX_DATABASE_COLLECTION: &str = "litd_engine_editor_database";
pub const ENGINE_CALYX_DATABASE_COLUMN: &[u8] = b"record_json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineCalyxRecordKind {
    Project,
    World,
    Object,
    AssetMetadata,
    Save,
    Provenance,
    ContentConstellation,
    SemanticIndex,
    AgentRuntimeLedger,
    /// Every admission attempt on a model response — admitted or refused (#1548). Written
    /// unconditionally, so a refusal is provable: the row shows the gate ran on exactly these
    /// bytes, and named the stage that stopped them.
    AgentResponseLedger,
    /// A sim order lowered from a model response (#1548). Written IF AND ONLY IF the response was
    /// admitted. Its ABSENCE is the proof that a refused response produced no order — a return
    /// code proves nothing.
    LoweredAgentOrder,
    WorldActivationLedger,
    WardGateLedger,
    AssetPipelineLedger,
    EditorIntelligenceLedger,
    ObservabilityLedger,
    /// An authored Ward response corpus (#1552): the trusted narrations and calibration OOD texts
    /// for one world/agent-archetype, derived from ingested content, persisted with provenance and
    /// a self-hash so the Ward response policy is sourced from a readable store, not example code.
    WardResponseCorpus,
    FactionRosterPolicy,
    FactionRosterState,
    FactionRosterAudit,
}

impl EngineCalyxRecordKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::World => "world",
            Self::Object => "object",
            Self::AssetMetadata => "asset_metadata",
            Self::Save => "save",
            Self::Provenance => "provenance",
            Self::ContentConstellation => "content_constellation",
            Self::SemanticIndex => "semantic_index",
            Self::AgentRuntimeLedger => "agent_runtime_ledger",
            Self::AgentResponseLedger => "agent_response_ledger",
            Self::LoweredAgentOrder => "lowered_agent_order",
            Self::WorldActivationLedger => "world_activation_ledger",
            Self::WardGateLedger => "ward_gate_ledger",
            Self::AssetPipelineLedger => "asset_pipeline_ledger",
            Self::EditorIntelligenceLedger => "editor_intelligence_ledger",
            Self::ObservabilityLedger => "observability_ledger",
            Self::WardResponseCorpus => "ward_response_corpus",
            Self::FactionRosterPolicy => "faction_roster_policy",
            Self::FactionRosterState => "faction_roster_state",
            Self::FactionRosterAudit => "faction_roster_audit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineCalyxDatabaseConfig {
    pub vault_dir: PathBuf,
    pub vault_id: String,
    pub purpose: Vec<u8>,
}

impl EngineCalyxDatabaseConfig {
    pub fn for_vault(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            vault_dir: vault_dir.into(),
            vault_id: ENGINE_CALYX_DATABASE_VAULT_ID.to_string(),
            purpose: ENGINE_CALYX_DATABASE_PURPOSE.to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxRecord {
    pub kind: EngineCalyxRecordKind,
    pub id: String,
    pub payload: Value,
    pub provenance: Value,
}

impl EngineCalyxRecord {
    pub fn new(
        kind: EngineCalyxRecordKind,
        id: impl Into<String>,
        payload: Value,
        provenance: Value,
    ) -> Self {
        Self {
            kind,
            id: id.into(),
            payload,
            provenance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredEngineCalyxRecord {
    pub schema: String,
    pub kind: EngineCalyxRecordKind,
    pub id: String,
    pub payload: Value,
    pub provenance: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCalyxDatabaseReadback {
    pub schema: String,
    pub vault_dir: PathBuf,
    pub vault_id: String,
    pub kind: EngineCalyxRecordKind,
    pub id: String,
    pub collection: String,
    pub row_key: String,
    pub column: String,
    pub bytes: usize,
    pub sha256: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub payload: Value,
    pub provenance: Value,
}

#[derive(Debug, Clone)]
pub struct EngineCalyxDatabase {
    config: EngineCalyxDatabaseConfig,
}

impl EngineCalyxDatabase {
    pub fn open(config: EngineCalyxDatabaseConfig) -> Result<Self, EngineCalyxPersistenceError> {
        validate_config(&config)?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &EngineCalyxDatabaseConfig {
        &self.config
    }

    /// Runs a database read/validate/write critical section under the same
    /// durable-vault file-lock mechanism used by Aster. Plain-column JSON
    /// writes are otherwise last-writer-wins, so callers enforcing uniqueness
    /// must keep their source-of-truth read and subsequent writes inside this
    /// boundary. The lock is cross-process and bounded; a timeout is an error.
    pub fn with_exclusive_transaction<T, E>(
        &self,
        operation: &str,
        f: impl FnOnce(&Self) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<EngineCalyxPersistenceError>,
    {
        let lock_path = self
            .config
            .vault_dir
            .join("locks")
            .join("litd-engine-exclusive-transaction.lock");
        let _guard =
            calyx_aster::file_lock::FileLockGuard::acquire(&lock_path).map_err(|error| {
                E::from(EngineCalyxPersistenceError::Store {
                    op: format!("acquire_exclusive_transaction:{operation}"),
                    reason: format!("{}: {}", error.code, error.message),
                })
            })?;
        f(self)
    }

    pub fn put_record(
        &self,
        record: &EngineCalyxRecord,
    ) -> Result<EngineCalyxDatabaseReadback, EngineCalyxPersistenceError> {
        validate_record(record)?;
        let stored = StoredEngineCalyxRecord {
            schema: ENGINE_CALYX_DATABASE_RECORD_SCHEMA.to_string(),
            kind: record.kind,
            id: record.id.clone(),
            payload: record.payload.clone(),
            provenance: record.provenance.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&stored).map_err(|error| {
            EngineCalyxPersistenceError::Store {
                op: "serialize_record".to_string(),
                reason: error.to_string(),
            }
        })?;
        let expected_payload = json_roundtrip_value("payload", &record.payload)?;
        let expected_provenance = json_roundtrip_value("provenance", &record.provenance)?;
        let row_key = row_key(record.kind, &record.id);
        let spec = self.spec(&row_key);
        let store_readback = write_calyx_json_bytes(&spec, &bytes).map_err(|reason| {
            EngineCalyxPersistenceError::Store {
                op: "write_record".to_string(),
                reason,
            }
        })?;
        let readback = parse_readback(&self.config, record.kind, &record.id, store_readback)?;
        if readback.payload != expected_payload {
            return Err(EngineCalyxPersistenceError::ReadbackMismatch {
                field: "payload".to_string(),
                expected: expected_payload.to_string(),
                actual: readback.payload.to_string(),
            });
        }
        if readback.provenance != expected_provenance {
            return Err(EngineCalyxPersistenceError::ReadbackMismatch {
                field: "provenance".to_string(),
                expected: expected_provenance.to_string(),
                actual: readback.provenance.to_string(),
            });
        }
        Ok(readback)
    }

    pub fn get_record(
        &self,
        kind: EngineCalyxRecordKind,
        id: &str,
    ) -> Result<EngineCalyxDatabaseReadback, EngineCalyxPersistenceError> {
        validate_id(id)?;
        let row_key = row_key(kind, id);
        let spec = self.spec(&row_key);
        let readback = read_calyx_json_bytes(&spec)
            .map_err(|reason| EngineCalyxPersistenceError::Store {
                op: "read_record".to_string(),
                reason,
            })?
            .ok_or_else(|| EngineCalyxPersistenceError::MissingRecord {
                kind,
                id: id.to_string(),
            })?;
        parse_readback(&self.config, kind, id, readback)
    }

    fn spec<'a>(&'a self, row_key: &'a [u8]) -> CalyxJsonStoreSpec<'a> {
        CalyxJsonStoreSpec {
            vault_dir: &self.config.vault_dir,
            vault_id: &self.config.vault_id,
            purpose: &self.config.purpose,
            collection: ENGINE_CALYX_DATABASE_COLLECTION,
            row_key,
            column: ENGINE_CALYX_DATABASE_COLUMN,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineCalyxPersistenceError {
    #[error("engine Calyx database config field {field} is invalid: {reason}")]
    InvalidConfig { field: String, reason: String },
    #[error("engine Calyx database record field {field} is invalid: {reason}")]
    InvalidRecord { field: String, reason: String },
    #[error("engine Calyx database operation {op} failed: {reason}")]
    Store { op: String, reason: String },
    #[error("engine Calyx database record {kind:?}/{id} is missing")]
    MissingRecord {
        kind: EngineCalyxRecordKind,
        id: String,
    },
    #[error(
        "engine Calyx database readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    ReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },
}

fn validate_config(config: &EngineCalyxDatabaseConfig) -> Result<(), EngineCalyxPersistenceError> {
    if config.vault_dir.as_os_str().is_empty() {
        return invalid_config("vault_dir", "Aster vault directory must be non-empty");
    }
    if config.vault_dir.is_file() {
        return invalid_config(
            "vault_dir",
            format!(
                "{} is a file; expected a Calyx Aster vault directory",
                config.vault_dir.display()
            ),
        );
    }
    if config.vault_id.trim().is_empty() {
        return invalid_config("vault_id", "Aster vault id must be non-empty");
    }
    if config.purpose.is_empty() {
        return invalid_config("purpose", "Aster vault purpose must be non-empty");
    }
    Ok(())
}

fn validate_record(record: &EngineCalyxRecord) -> Result<(), EngineCalyxPersistenceError> {
    validate_id(&record.id)?;
    if record.payload.is_null() {
        return invalid_record("payload", "payload must be non-null JSON");
    }
    match record.provenance.as_object() {
        Some(map) if !map.is_empty() => Ok(()),
        _ => invalid_record(
            "provenance",
            "provenance must be a non-empty JSON object stored with the record",
        ),
    }
}

fn validate_id(id: &str) -> Result<(), EngineCalyxPersistenceError> {
    if id.trim().is_empty() {
        return invalid_record("id", "id must be non-empty");
    }
    if id.chars().any(char::is_control) {
        return invalid_record("id", "id must not contain control characters");
    }
    Ok(())
}

fn json_roundtrip_value(
    field: &'static str,
    value: &Value,
) -> Result<Value, EngineCalyxPersistenceError> {
    let bytes = serde_json::to_vec(value).map_err(|error| EngineCalyxPersistenceError::Store {
        op: format!("normalize_{field}"),
        reason: error.to_string(),
    })?;
    serde_json::from_slice(&bytes).map_err(|error| EngineCalyxPersistenceError::Store {
        op: format!("normalize_{field}"),
        reason: error.to_string(),
    })
}

fn parse_readback(
    config: &EngineCalyxDatabaseConfig,
    expected_kind: EngineCalyxRecordKind,
    expected_id: &str,
    store_readback: CalyxJsonStoreReadback,
) -> Result<EngineCalyxDatabaseReadback, EngineCalyxPersistenceError> {
    let stored = serde_json::from_slice::<StoredEngineCalyxRecord>(&store_readback.bytes).map_err(
        |error| EngineCalyxPersistenceError::Store {
            op: "deserialize_record".to_string(),
            reason: format!("{}: {error}", store_readback.vault_dir.display()),
        },
    )?;
    if stored.schema != ENGINE_CALYX_DATABASE_RECORD_SCHEMA {
        return Err(EngineCalyxPersistenceError::ReadbackMismatch {
            field: "schema".to_string(),
            expected: ENGINE_CALYX_DATABASE_RECORD_SCHEMA.to_string(),
            actual: stored.schema,
        });
    }
    if stored.kind != expected_kind {
        return Err(EngineCalyxPersistenceError::ReadbackMismatch {
            field: "kind".to_string(),
            expected: expected_kind.as_str().to_string(),
            actual: stored.kind.as_str().to_string(),
        });
    }
    if stored.id != expected_id {
        return Err(EngineCalyxPersistenceError::ReadbackMismatch {
            field: "id".to_string(),
            expected: expected_id.to_string(),
            actual: stored.id,
        });
    }
    let record = EngineCalyxRecord::new(
        stored.kind,
        stored.id.clone(),
        stored.payload.clone(),
        stored.provenance.clone(),
    );
    validate_record(&record)?;
    Ok(EngineCalyxDatabaseReadback {
        schema: ENGINE_CALYX_DATABASE_READBACK_SCHEMA.to_string(),
        vault_dir: store_readback.vault_dir,
        vault_id: config.vault_id.clone(),
        kind: stored.kind,
        id: stored.id,
        collection: store_readback.collection,
        row_key: store_readback.row_key,
        column: store_readback.column,
        bytes: store_readback.byte_len,
        sha256: store_readback.sha256,
        seq: store_readback.seq,
        graph_rows: store_readback.graph_rows,
        payload: stored.payload,
        provenance: stored.provenance,
    })
}

fn row_key(kind: EngineCalyxRecordKind, id: &str) -> Vec<u8> {
    format!("{}/{}", kind.as_str(), id).into_bytes()
}

fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxPersistenceError> {
    Err(EngineCalyxPersistenceError::InvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

fn invalid_record<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxPersistenceError> {
    Err(EngineCalyxPersistenceError::InvalidRecord {
        field: field.into(),
        reason: reason.into(),
    })
}

pub fn default_database_config(vault_dir: impl AsRef<Path>) -> EngineCalyxDatabaseConfig {
    EngineCalyxDatabaseConfig::for_vault(vault_dir.as_ref())
}
