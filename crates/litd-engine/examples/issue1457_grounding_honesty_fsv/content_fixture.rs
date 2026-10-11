//! Synthetic content-constellation readbacks for the issue #1457 FSV.
//!
//! These build deterministic [`EngineContentConstellationReadback`] rows (keyword
//! index inputs + Ward subjects) without touching the learned-lens measure path,
//! so the grounding-honesty verdict runs headless. Split out of the harness main
//! to keep every example file under the <500-line ratchet (#800 sec.6).

use std::{collections::BTreeMap, path::Path};

use calyx_core::{Constellation, CxFlags, CxId, InputRef, LedgerRef, Modality, VaultId};
use litd_engine::calyx_content::{
    ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA, ENGINE_CALYX_CONTENT_READBACK_SCHEMA,
    ENGINE_CONTENT_A37_ROSTER_SCHEMA, ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE,
    ENGINE_LOWERED_CONTENT_SCHEMA, EngineContentA37FrozenRoster,
    EngineContentConstellationReadback, EngineContentKind, EngineContentMeasurementReadback,
    EngineLoweredContentArtifact,
};
use litd_engine::calyx_persistence::{
    ENGINE_CALYX_DATABASE_COLLECTION, ENGINE_CALYX_DATABASE_COLUMN,
    ENGINE_CALYX_DATABASE_READBACK_SCHEMA, ENGINE_CALYX_DATABASE_VAULT_ID,
    EngineCalyxDatabaseReadback, EngineCalyxRecordKind,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) fn content_readbacks(
    vault_dir: &Path,
) -> Result<Vec<EngineContentConstellationReadback>, Box<dyn std::error::Error>> {
    Ok(vec![
        content_readback(
            vault_dir,
            1,
            EngineContentKind::Ability,
            "ability-fire-ward",
            "Fire Ward",
            json!({
                "ability_id": "fire-ward",
                "summary": "grounded fire ward protects the gate",
                "tags": ["fire", "ward", "gate", "protect"]
            }),
        )?,
        content_readback(
            vault_dir,
            2,
            EngineContentKind::Story,
            "story-ash-gate",
            "Ash Gate",
            json!({
                "story_id": "ash-gate",
                "summary": "enemy pressure at the old ash gate",
                "tags": ["enemy", "gate", "story"]
            }),
        )?,
    ])
}

fn content_readback(
    vault_dir: &Path,
    seq: u64,
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    payload: Value,
) -> Result<EngineContentConstellationReadback, Box<dyn std::error::Error>> {
    let source_path = format!("synthetic://issue1457/{id}");
    let input_bytes = serde_json::to_vec(&json!({
        "kind": kind,
        "id": id,
        "display_name": display_name,
        "payload": payload,
        "source_path": source_path
    }))?;
    let input_hash = sha256_bytes(&input_bytes);
    let input_sha256 = hex_lower(&input_hash);
    let cx_id = CxId::from_input(&input_bytes, 1, b"issue1457-keyword-synthetic-content");
    let cx_id_text = cx_id.to_string();
    let database_sha256 =
        litd_hash::sha256_hex(format!("issue1457:{seq}:{kind:?}:{id}:{input_sha256}").as_bytes());
    let database = EngineCalyxDatabaseReadback {
        schema: ENGINE_CALYX_DATABASE_READBACK_SCHEMA.to_string(),
        vault_dir: vault_dir.to_path_buf(),
        vault_id: ENGINE_CALYX_DATABASE_VAULT_ID.to_string(),
        kind: EngineCalyxRecordKind::ContentConstellation,
        id: format!("content_constellation/{}/{}", kind.as_str(), id),
        collection: ENGINE_CALYX_DATABASE_COLLECTION.to_string(),
        row_key: format!("content_constellation/{}/{}", kind.as_str(), id),
        column: String::from_utf8_lossy(ENGINE_CALYX_DATABASE_COLUMN).to_string(),
        bytes: input_bytes.len(),
        sha256: database_sha256,
        seq,
        graph_rows: 1,
        payload: payload.clone(),
        provenance: json!({
            "source_issue": 1457,
            "source_path": source_path,
            "input_sha256": input_sha256
        }),
    };
    let lowering = EngineLoweredContentArtifact {
        schema: ENGINE_LOWERED_CONTENT_SCHEMA.to_string(),
        artifact_id: format!("lowered://issue1457/{}/{}", kind.as_str(), cx_id),
        source_cx_id: cx_id_text.clone(),
        source_kind: kind,
        source_id: id.to_string(),
        fingerprint: input_sha256.clone(),
        hot_path_policy: "frozen_fingerprinted_data_only".to_string(),
    };
    Ok(EngineContentConstellationReadback {
        schema: ENGINE_CALYX_CONTENT_READBACK_SCHEMA.to_string(),
        database,
        kind,
        id: id.to_string(),
        cx_id: cx_id_text,
        display_name: display_name.to_string(),
        source_path,
        input_sha256,
        lowering,
        measurement: synthetic_measurement(),
        constellation: Constellation {
            cx_id,
            vault_id: ENGINE_CALYX_DATABASE_VAULT_ID.parse::<VaultId>()?,
            panel_version: 1,
            created_at: 1_786_000_000,
            input_ref: InputRef {
                hash: input_hash,
                pointer: Some(format!("synthetic://issue1457/{id}")),
                redacted: false,
            },
            modality: Modality::Text,
            slots: BTreeMap::new(),
            scalars: BTreeMap::new(),
            metadata: BTreeMap::from([
                ("engine_content_kind".to_string(), kind.as_str().to_string()),
                ("engine_content_id".to_string(), id.to_string()),
                (
                    "source_path".to_string(),
                    format!("synthetic://issue1457/{id}"),
                ),
            ]),
            anchors: Vec::new(),
            provenance: LedgerRef {
                seq,
                hash: sha256_bytes(format!("issue1457-ledger:{seq}:{id}").as_bytes()),
            },
            flags: CxFlags::default(),
        },
        payload,
    })
}

fn synthetic_measurement() -> EngineContentMeasurementReadback {
    EngineContentMeasurementReadback {
        schema: ENGINE_CALYX_CONTENT_MEASUREMENT_SCHEMA.to_string(),
        panel_template: "synthetic-keyword-readback".to_string(),
        panel_source: "issue1457-fsv-synthetic-content".to_string(),
        panel_version: 1,
        input_modality: "text".to_string(),
        min_learned_lenses: 0,
        measured_learned_lenses: 0,
        min_association_families: 0,
        association_family_count: 0,
        association_families: Vec::new(),
        measured_slot_ids: Vec::new(),
        measured_lens_names: Vec::new(),
        frozen_roster: EngineContentA37FrozenRoster {
            schema: ENGINE_CONTENT_A37_ROSTER_SCHEMA.to_string(),
            panel_template: "synthetic-keyword-readback".to_string(),
            panel_version: 1,
            panel_state_sha256: String::new(),
            registry_roster_sha256: String::new(),
            lenses: Vec::new(),
        },
        dropped_synthetic_anchor_count: 0,
        sufficiency_state: "not_required_for_keyword_index_fsv".to_string(),
        sufficiency_reason: "issue1457 verifies keyword naming and Ward admission readbacks"
            .to_string(),
        measurement_state: ENGINE_CONTENT_MEASUREMENT_STATE_COMPLETE.to_string(),
        declared_learned_lenses: 0,
        pending_learned_lenses: Vec::new(),
    }
}

fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
