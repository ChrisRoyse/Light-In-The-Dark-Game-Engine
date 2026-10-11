//! Calyx constellation-backed world/content/asset data.
//!
//! Editor tools write full Calyx-shaped records here; runtime systems consume
//! frozen lowering metadata outside the deterministic hot path.

use std::collections::BTreeMap;

use calyx_core::{Anchor, Constellation, Modality};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub use crate::calyx_content_a37_admission::{
    EngineContentA37AdmissionReadback, EngineContentA37AdmissionRef,
    EngineContentA37AdmissionReport, read_engine_content_a37_admission,
    write_engine_content_a37_admission,
};
pub use crate::calyx_content_a37_identity::{
    ENGINE_CONTENT_A37_ROSTER_SCHEMA, ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE,
    ENGINE_CONTENT_MEASUREMENT_STATE_PENDING, EngineContentA37FrozenLens,
    EngineContentA37FrozenRoster, EngineContentMeasurementReadback, EngineContentPendingLens,
    frozen_roster_hash, measured_frozen_roster, verify_admission_against_measured_roster,
};
pub use crate::calyx_content_a37_multi_anchor::{
    EngineContentA37InputReport, EngineContentA37MultiAnchorConfig,
    build_engine_content_a37_admission_report,
};
pub use crate::calyx_content_assay::{
    ENGINE_CALYX_CONTENT_ASSAY_SCHEMA, EngineContentAssayReadback, EngineContentAssayReport,
    measure_engine_content_assay_report, read_engine_content_assay, store_engine_content_assay,
    store_engine_content_assay_with_a37_admission,
};
pub use crate::calyx_content_measurement::{
    ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA, EngineContentMeasurementConfig,
};
pub use crate::calyx_paths::{
    ENGINE_CONTENT_MEASUREMENT_VAULT_ENV, ENGINE_CONTENT_PANEL_VAULT_NAME, GAMEEDITOR_CALYX_ROOT,
    canonical_gameeditor_calyx_root, resolve_engine_content_panel_vault,
    validate_existing_gameeditor_calyx_path, validate_gameeditor_calyx_home,
};
use crate::{
    calyx_content_a37_identity::validate_measurement_state,
    calyx_content_measurement::measure_content_record,
    calyx_content_vectors::compact_non_dense_vectors_for_content_row,
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxPersistenceError,
        EngineCalyxRecord, EngineCalyxRecordKind,
    },
};

pub const ENGINE_CALYX_CONTENT_SCHEMA: &str = "litd.engine.calyx_content_constellation.v2";
pub const ENGINE_CALYX_CONTENT_READBACK_SCHEMA: &str =
    "litd.engine.calyx_content_constellation_readback.v1";
pub const ENGINE_LOWERED_CONTENT_SCHEMA: &str = "litd.engine.lowered_content_artifact.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineContentKind {
    Unit,
    Ability,
    Item,
    Trigger,
    Terrain,
    Story,
    Lore,
    Asset,
    Agent,
}

impl EngineContentKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Ability => "ability",
            Self::Item => "item",
            Self::Trigger => "trigger",
            Self::Terrain => "terrain",
            Self::Story => "story",
            Self::Lore => "lore",
            Self::Asset => "asset",
            Self::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentSource {
    pub path: String,
    pub author: String,
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentRecord {
    pub kind: EngineContentKind,
    pub id: String,
    pub display_name: String,
    pub payload: Value,
    pub source: EngineContentSource,
    pub anchors: Vec<Anchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentConstellationPayload {
    pub schema: String,
    pub kind: EngineContentKind,
    pub id: String,
    pub display_name: String,
    pub source: EngineContentSource,
    pub payload: Value,
    pub constellation: Constellation,
    pub measurement: EngineContentMeasurementReadback,
    pub lowering: EngineLoweredContentArtifact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContentConstellationReadback {
    pub schema: String,
    pub database: EngineCalyxDatabaseReadback,
    pub kind: EngineContentKind,
    pub id: String,
    pub cx_id: String,
    pub display_name: String,
    pub source_path: String,
    pub input_sha256: String,
    pub lowering: EngineLoweredContentArtifact,
    pub measurement: EngineContentMeasurementReadback,
    pub constellation: Constellation,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineLoweredContentArtifact {
    pub schema: String,
    pub artifact_id: String,
    pub source_cx_id: String,
    pub source_kind: EngineContentKind,
    pub source_id: String,
    pub fingerprint: String,
    pub hot_path_policy: String,
}

#[derive(Debug, Clone)]
pub struct EngineCalyxContentStore {
    database: EngineCalyxDatabase,
    measurement: EngineContentMeasurementConfig,
}

impl EngineCalyxContentStore {
    pub fn new(database: EngineCalyxDatabase) -> Self {
        Self::with_measurement_config(
            database,
            EngineContentMeasurementConfig::default_panel_vault(),
        )
    }

    pub fn with_measurement_config(
        database: EngineCalyxDatabase,
        measurement: EngineContentMeasurementConfig,
    ) -> Self {
        Self {
            database,
            measurement,
        }
    }

    pub fn put_content(
        &self,
        record: &EngineContentRecord,
    ) -> Result<EngineContentConstellationReadback, EngineCalyxContentError> {
        validate_content_record(record)?;
        let payload = constellation_payload(&self.database, &self.measurement, record)?;
        let provenance = json!({
            "source_path": record.source.path,
            "source_hash": record.source.source_hash,
            "author": record.source.author,
            "source_issue": 1456,
            "constellation_id": payload.constellation.cx_id.to_string(),
            "measurement_state": payload.measurement.measurement_state,
            "pending_learned_lenses": payload.measurement.pending_learned_lenses.len(),
        });
        self.persist_content_payload(record, payload, provenance)
    }

    /// Re-measure a `pending_measurement` row now that the AI addon is available
    /// and persist the completed constellation with two-phase provenance (#1791).
    /// A row that is already `complete` is returned unchanged (idempotent).
    pub fn backfill_pending_content(
        &self,
        kind: EngineContentKind,
        id: &str,
    ) -> Result<crate::calyx_content_backfill::EngineContentBackfillOutcome, EngineCalyxContentError>
    {
        crate::calyx_content_backfill::backfill_pending_content(self, kind, id)
    }

    pub(crate) fn database(&self) -> &EngineCalyxDatabase {
        &self.database
    }

    pub(crate) fn measurement(&self) -> &EngineContentMeasurementConfig {
        &self.measurement
    }

    /// Read the raw stored constellation payload (full source, constellation,
    /// measurement) for a row. Unlike [`Self::get_content_for_editor`] this keeps
    /// the complete [`EngineContentSource`] so a backfill can reproduce the exact
    /// canonical input (and therefore the same `cx_id`).
    pub(crate) fn read_content_payload(
        &self,
        kind: EngineContentKind,
        id: &str,
    ) -> Result<EngineContentConstellationPayload, EngineCalyxContentError> {
        validate_content_id(id)?;
        let database = self.database.get_record(
            EngineCalyxRecordKind::ContentConstellation,
            &content_row_id(kind, id),
        )?;
        serde_json::from_value::<EngineContentConstellationPayload>(database.payload)
            .map_err(content_store_failed)
    }

    /// Persist a constellation payload with the given provenance JSON, verifying
    /// the read-back the same way [`Self::put_content`] does.
    pub(crate) fn persist_content_payload(
        &self,
        record: &EngineContentRecord,
        payload: EngineContentConstellationPayload,
        provenance: Value,
    ) -> Result<EngineContentConstellationReadback, EngineCalyxContentError> {
        let db_record = EngineCalyxRecord::new(
            EngineCalyxRecordKind::ContentConstellation,
            content_row_id(record.kind, &record.id),
            serde_json::to_value(&payload).map_err(content_store_failed)?,
            provenance,
        );
        let database = self.database.put_record(&db_record)?;
        parse_content_readback(database)
    }

    pub fn get_content_for_editor(
        &self,
        kind: EngineContentKind,
        id: &str,
    ) -> Result<EngineContentConstellationReadback, EngineCalyxContentError> {
        validate_content_id(id)?;
        let database = self.database.get_record(
            EngineCalyxRecordKind::ContentConstellation,
            &content_row_id(kind, id),
        )?;
        parse_content_readback(database)
    }

    pub fn lowered_content_for_engine(
        &self,
        kind: EngineContentKind,
        id: &str,
    ) -> Result<EngineLoweredContentArtifact, EngineCalyxContentError> {
        Ok(self.get_content_for_editor(kind, id)?.lowering)
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EngineCalyxContentError {
    #[error("engine Calyx content field {field} is invalid: {reason}")]
    InvalidContent { field: String, reason: String },
    #[error("engine Calyx content persistence failed: {0}")]
    Persistence(#[from] EngineCalyxPersistenceError),
    #[error("engine Calyx content codec failed: {0}")]
    Codec(String),
    #[error("engine Calyx content measurement phase {phase} failed: {reason}")]
    Measurement { phase: String, reason: String },
    #[error(
        "engine Calyx content readback mismatch for {field}: expected {expected}, actual {actual}"
    )]
    ReadbackMismatch {
        field: String,
        expected: String,
        actual: String,
    },
}

pub(crate) fn constellation_payload(
    database: &EngineCalyxDatabase,
    measurement: &EngineContentMeasurementConfig,
    record: &EngineContentRecord,
) -> Result<EngineContentConstellationPayload, EngineCalyxContentError> {
    let input = canonical_input(record)?;
    let input_hash = sha256_bytes(&input);
    let input_sha256 = sha256_hex(&input);
    let source_pointer = format!(
        "calyx://engine-content/{}/{}",
        record.kind.as_str(),
        record.id
    );
    let (constellation, measurement_readback) = measure_content_record(
        database,
        measurement,
        record,
        input.clone(),
        source_pointer,
        input_hash,
    )?;
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "engine_content_kind".to_string(),
        record.kind.as_str().to_string(),
    );
    metadata.insert("engine_content_id".to_string(), record.id.clone());
    metadata.insert("display_name".to_string(), record.display_name.clone());
    metadata.insert("source_path".to_string(), record.source.path.clone());
    metadata.insert("source_hash".to_string(), record.source.source_hash.clone());
    metadata.insert(
        "lowering_policy".to_string(),
        "frozen_fingerprinted".to_string(),
    );
    let mut constellation = constellation;
    constellation.metadata.extend(metadata);
    compact_non_dense_vectors_for_content_row(
        &mut constellation.slots,
        &mut constellation.metadata,
    )?;
    constellation
        .validate_schema()
        .map_err(|error| EngineCalyxContentError::InvalidContent {
            field: "constellation".to_string(),
            reason: error.to_string(),
        })?;
    let lowering = EngineLoweredContentArtifact {
        schema: ENGINE_LOWERED_CONTENT_SCHEMA.to_string(),
        artifact_id: format!(
            "lowered://engine-content/{}/{}",
            record.kind.as_str(),
            constellation.cx_id
        ),
        source_cx_id: constellation.cx_id.to_string(),
        source_kind: record.kind,
        source_id: record.id.clone(),
        fingerprint: input_sha256,
        hot_path_policy: "frozen_fingerprinted_data_only".to_string(),
    };
    Ok(EngineContentConstellationPayload {
        schema: ENGINE_CALYX_CONTENT_SCHEMA.to_string(),
        kind: record.kind,
        id: record.id.clone(),
        display_name: record.display_name.clone(),
        source: record.source.clone(),
        payload: record.payload.clone(),
        constellation,
        measurement: measurement_readback,
        lowering,
    })
}

fn parse_content_readback(
    database: EngineCalyxDatabaseReadback,
) -> Result<EngineContentConstellationReadback, EngineCalyxContentError> {
    let stored =
        serde_json::from_value::<EngineContentConstellationPayload>(database.payload.clone())
            .map_err(content_store_failed)?;
    if stored.schema != ENGINE_CALYX_CONTENT_SCHEMA {
        return mismatch(
            "schema",
            ENGINE_CALYX_CONTENT_SCHEMA,
            stored.schema.as_str(),
        );
    }
    if stored.constellation.modality != Modality::Text {
        return mismatch("constellation.modality", "text", "non_text");
    }
    if stored.measurement.schema != ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA {
        return mismatch(
            "measurement.schema",
            ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA,
            stored.measurement.schema.as_str(),
        );
    }
    if stored.measurement.association_family_count < stored.measurement.min_association_families {
        return mismatch(
            "measurement.association_family_count",
            stored
                .measurement
                .min_association_families
                .to_string()
                .as_str(),
            stored
                .measurement
                .association_family_count
                .to_string()
                .as_str(),
        );
    }
    validate_measurement_state(&stored.measurement, &stored.constellation)?;
    let expected_row = content_row_id(stored.kind, &stored.id);
    if database.id != expected_row {
        return mismatch("database.id", expected_row.as_str(), database.id.as_str());
    }
    if stored.lowering.source_cx_id != stored.constellation.cx_id.to_string() {
        return mismatch(
            "lowering.source_cx_id",
            stored.constellation.cx_id.to_string().as_str(),
            stored.lowering.source_cx_id.as_str(),
        );
    }
    Ok(EngineContentConstellationReadback {
        schema: ENGINE_CALYX_CONTENT_READBACK_SCHEMA.to_string(),
        kind: stored.kind,
        id: stored.id,
        cx_id: stored.constellation.cx_id.to_string(),
        display_name: stored.display_name,
        source_path: stored.source.path,
        input_sha256: stored.lowering.fingerprint.clone(),
        lowering: stored.lowering,
        measurement: stored.measurement,
        constellation: stored.constellation,
        payload: stored.payload,
        database,
    })
}

fn validate_content_record(record: &EngineContentRecord) -> Result<(), EngineCalyxContentError> {
    validate_content_id(&record.id)?;
    if record.display_name.trim().is_empty() {
        return invalid_content("display_name", "display name must be non-empty");
    }
    if record.payload.is_null() {
        return invalid_content("payload", "payload must be non-null JSON");
    }
    if record.source.path.trim().is_empty() {
        return invalid_content("source.path", "source path must be non-empty");
    }
    if record.source.source_hash.trim().is_empty() {
        return invalid_content("source.source_hash", "source hash must be non-empty");
    }
    for anchor in &record.anchors {
        anchor
            .validate_schema()
            .map_err(|error| EngineCalyxContentError::InvalidContent {
                field: "anchors".to_string(),
                reason: error.to_string(),
            })?;
    }
    Ok(())
}

fn validate_content_id(id: &str) -> Result<(), EngineCalyxContentError> {
    if id.trim().is_empty() {
        return invalid_content("id", "id must be non-empty");
    }
    if id.chars().any(char::is_control) {
        return invalid_content("id", "id must not contain control characters");
    }
    Ok(())
}

fn canonical_input(record: &EngineContentRecord) -> Result<Vec<u8>, EngineCalyxContentError> {
    serde_json::to_vec(&json!({
        "kind": record.kind,
        "id": record.id,
        "display_name": record.display_name,
        "payload": record.payload,
        "source": record.source,
    }))
    .map_err(content_store_failed)
}

fn content_row_id(kind: EngineContentKind, id: &str) -> String {
    format!("{}/{}", kind.as_str(), id)
}

fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn invalid_content<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxContentError> {
    Err(EngineCalyxContentError::InvalidContent {
        field: field.into(),
        reason: reason.into(),
    })
}

fn mismatch<T>(field: &str, expected: &str, actual: &str) -> Result<T, EngineCalyxContentError> {
    Err(EngineCalyxContentError::ReadbackMismatch {
        field: field.to_string(),
        expected: expected.to_string(),
        actual: actual.to_string(),
    })
}

fn content_store_failed(error: impl std::fmt::Display) -> EngineCalyxContentError {
    EngineCalyxContentError::Codec(error.to_string())
}
