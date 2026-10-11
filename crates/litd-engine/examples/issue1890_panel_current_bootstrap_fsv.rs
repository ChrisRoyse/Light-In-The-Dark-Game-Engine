//! #1890 FSV: materialize the canonical gameeditor-engine-content-panel Aster
//! vault with a real ManifestStore CURRENT + panel/registry assets.
//!
//! Root cause of the host gap after D: migration: empty or partially-written
//! vault dirs (cf/wal without CURRENT) make `load_vault_panel_state` fail with
//! os error 2 on CURRENT. Fix: open durable Aster with an explicit panel, then
//! `persist_vault_panel_state` so CURRENT points at an immutable MANIFEST.
//!
//! Source of truth (read after write):
//! 1. Filesystem: `{vault}/CURRENT` exists and is non-empty
//! 2. `ManifestStore::load_current` returns a validated manifest
//! 3. `load_vault_panel_state` returns panel with expected slot count
//! 4. `open_canonical_engine_content_store` + put_content under auto mode works
//!
//! Edges: missing CURRENT before bootstrap is observed; re-bootstrap is
//! idempotent (second persist advances manifest_seq).
//!
//! Run: cargo run -p litd-engine --example issue1890_panel_current_bootstrap_fsv --features fsv

use std::error::Error;
use std::fs;
use std::path::Path;

use calyx_aster::manifest::ManifestStore;
use calyx_aster::vault::{AsterVault, VaultOptions};
use calyx_core::VaultId;
use calyx_registry::{load_vault_panel_state, persist_vault_panel_state};
use litd_engine::calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource};
use litd_engine::calyx_content_measurement_absent::{
    ADDON_ABSENT_CONTENT_PANEL_LENSES, addon_absent_content_panel_state,
};
use litd_engine::calyx_content_runtime::open_canonical_engine_content_store;
use litd_engine::calyx_paths::resolve_engine_content_panel_vault;
use litd_engine::calyx_persistence::{
    ENGINE_CALYX_DATABASE_PURPOSE, ENGINE_CALYX_DATABASE_VAULT_ID,
};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_engine::rpg::persist_and_lower_rpg_authored_content;
use litd_hash::sha256_hex;
use serde_json::json;

fn bootstrap_panel_vault(vault_dir: &Path) -> Result<(u64, usize), Box<dyn Error>> {
    let state = addon_absent_content_panel_state(ADDON_ABSENT_CONTENT_PANEL_LENSES);
    let vault_id: VaultId = ENGINE_CALYX_DATABASE_VAULT_ID
        .parse()
        .map_err(|e| format!("parse vault id: {e}"))?;
    // Durable open seeds Aster layout; panel option plants the first CURRENT.
    let _vault = AsterVault::new_durable(
        vault_dir,
        vault_id,
        ENGINE_CALYX_DATABASE_PURPOSE.to_vec(),
        VaultOptions {
            panel: Some(state.panel.clone()),
            ..VaultOptions::default()
        },
    )
    .map_err(|e| format!("AsterVault::new_durable: {e}"))?;
    let write = persist_vault_panel_state(vault_dir, &state.panel, &state.registry)
        .map_err(|e| format!("persist_vault_panel_state: {e}"))?;
    Ok((write.manifest_seq, state.panel.slots.len()))
}

fn sample_record() -> Result<EngineContentRecord, Box<dyn Error>> {
    let id = "area-issue1890-bootstrap";
    let payload = json!({
        "schema": "litd.rpg.area.v1",
        "surface_kind": "area_region",
        "id": id,
        "region_name": "Bootstrap Vale",
        "terrain_tag": "ashen_road",
        "acoustics": "muffled_open_ash",
        "description": "Panel CURRENT bootstrap FSV row for #1890"
    });
    Ok(EngineContentRecord {
        kind: EngineContentKind::Terrain,
        id: id.to_string(),
        display_name: "Issue1890 Bootstrap".to_string(),
        source: EngineContentSource {
            path: "editor://areas/issue1890-bootstrap".to_string(),
            author: "issue1890_panel_current_bootstrap_fsv".to_string(),
            source_hash: sha256_hex(&serde_json::to_vec(&payload)?),
        },
        payload,
        anchors: Vec::new(),
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1890_panel_current_bootstrap")?;

    let vault_dir = resolve_engine_content_panel_vault()?;
    let current_path = vault_dir.join("CURRENT");
    let before_current = current_path.exists();
    let before_bytes = if before_current {
        fs::metadata(&current_path)?.len()
    } else {
        0
    };

    let (manifest_seq, slot_count) = bootstrap_panel_vault(&vault_dir)?;

    // --- Source of truth readbacks (filesystem + APIs) ---
    let after_current = current_path.exists();
    let after_bytes = fs::metadata(&current_path)?.len();
    let pointer = fs::read_to_string(&current_path)?;
    let pointer_trim = pointer.trim().to_string();
    let manifest = ManifestStore::open(&vault_dir).load_current()?;
    let loaded = load_vault_panel_state(&vault_dir)?;
    let slots_match = loaded.panel.slots.len() == slot_count;

    // Content store authoring path still works after bootstrap.
    let store = open_canonical_engine_content_store()?;
    let authored = persist_and_lower_rpg_authored_content(&store, &[sample_record()?])?;
    let author_ok = authored.row_count == 1;

    // Edge: second bootstrap advances or keeps a readable CURRENT (idempotent open).
    let (seq2, _) = bootstrap_panel_vault(&vault_dir)?;
    let still_current = current_path.exists() && fs::metadata(&current_path)?.len() > 0;
    let seq_ok = seq2 >= manifest_seq;

    let verdict = after_current
        && after_bytes > 0
        && !pointer_trim.is_empty()
        && slots_match
        && author_ok
        && still_current
        && seq_ok
        && manifest.manifest_seq >= 1;

    let report = json!({
        "schema": "litd.fsv.issue1890.panel_current_bootstrap.v1",
        "issue": 1890,
        "parents": [1866, 1814, 1791],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "vault_dir": vault_dir.display().to_string().replace('\\', "/"),
        "before": { "current_exists": before_current, "current_bytes": before_bytes },
        "after": {
            "current_exists": after_current,
            "current_bytes": after_bytes,
            "current_pointer": pointer_trim,
            "manifest_seq": manifest.manifest_seq,
            "panel_slots": loaded.panel.slots.len(),
            "expected_slots": slot_count,
            "author_row_count": authored.row_count,
            "second_bootstrap_seq": seq2,
        },
        "sufficiency_argument": "Filesystem CURRENT + ManifestStore::load_current + load_vault_panel_state + content put/lower are independent SoT readbacks after Aster new_durable + persist_vault_panel_state. No mocks; empty CURRENT before bootstrap is recorded.",
    });
    let written = run.write_report(&report)?;
    let report_sha = sha256_hex(&fs::read(&written.path)?);
    println!(
        "ISSUE1890_PANEL_CURRENT_BOOTSTRAP_FSV verdict={} report={} sha256={} current_exists={} pointer={} slots={} author_ok={}",
        if verdict { "GREEN" } else { "RED" },
        written.path.display(),
        report_sha,
        after_current,
        pointer.trim(),
        loaded.panel.slots.len(),
        author_ok,
    );
    if !verdict {
        return Err("ISSUE1890 FSV verdict RED".into());
    }
    Ok(())
}
