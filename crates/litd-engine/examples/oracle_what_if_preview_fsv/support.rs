use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use calyx_assay::{AssayCacheKey, AssayStore, AssaySubject, EstimatorKind, MiEstimate, TrustTag};
use calyx_aster::cf::{ColumnFamily, base_key, recurrence_key};
use calyx_aster::dedup::{EpochSecs, OccurrenceId};
use calyx_aster::recurrence::{
    Occurrence, OccurrenceContext, StoredRecurrenceRow, encode_recurrence_row,
};
use calyx_aster::vault::encode;
use calyx_core::{
    AnchorKind, AnchorValue, Asymmetry, CxFlags, CxId, FixedClock, InputRef, LedgerRef, LensId,
    Modality, Panel, QuantPolicy, Slot, SlotId, SlotKey, SlotShape, SlotState, VaultId,
    content_address,
};
use litd_engine::what_if_preview::{
    WhatIfPreviewOutcome, WhatIfPreviewRequest, WhatIfPreviewService, WhatIfPreviewServiceConfig,
    WhatIfPreviewSourceTruth,
};
use serde::Serialize;
use serde_json::json;

pub const DOMAIN: &str = "issue1197";
pub const TEST_TS: u64 = 1_785_599_197;

pub fn request(action_id: &str) -> WhatIfPreviewRequest {
    WhatIfPreviewRequest {
        domain_id: DOMAIN.to_string(),
        action_id: action_id.to_string(),
        panel: panel(),
        desired_outcome: Some(AnchorValue::Text("NeedsRebalance".to_string())),
    }
}

pub fn service(
    root: &Path,
    name: &str,
) -> Result<WhatIfPreviewService<FixedClock>, Box<dyn std::error::Error>> {
    Ok(WhatIfPreviewService::open_with_clock(
        WhatIfPreviewServiceConfig::for_vault(root.join(name).join("vault")),
        FixedClock::new(TEST_TS),
    )?)
}

pub fn assert_preview(outcome: &WhatIfPreviewOutcome, first_child: &str, cycle: bool, low: bool) {
    let WhatIfPreviewOutcome::Previewed(result) = outcome else {
        panic!("expected previewed outcome");
    };
    assert_eq!(result.diagnostics.low_confidence_pruned, low);
    assert!(result.diagnostics.cycle_guard_satisfied);
    if low {
        assert!(result.tree.children.is_empty());
    } else {
        assert_eq!(result.tree.children[0].root.action_or_event, first_child);
        if cycle {
            assert!(result.tree.children[0].children.is_empty());
        }
    }
}

pub fn print_case(
    name: &str,
    before: &WhatIfPreviewSourceTruth,
    after: &WhatIfPreviewSourceTruth,
    action: &str,
) {
    println!(
        "FSV_CASE name={name} before_hash={:?} before_base={} before_recurrence={} before_assay={} before_ledger={} action={action} after_hash={:?} after_base={} after_recurrence={} after_assay={} after_ledger={} after_outcome={:?}",
        before.preview_row_hash,
        before.base_rows.len(),
        before.recurrence_rows.len(),
        before.assay_rows.len(),
        before.preview_ledger_rows.len(),
        after.preview_row_hash,
        after.base_rows.len(),
        after.recurrence_rows.len(),
        after.assay_rows.len(),
        after.preview_ledger_rows.len(),
        summarize_outcome(after),
    );
}

pub fn case(
    name: &'static str,
    expected: &'static str,
    before: WhatIfPreviewSourceTruth,
    after: WhatIfPreviewSourceTruth,
    action: &str,
) -> CaseEvidence {
    CaseEvidence {
        name,
        expected,
        action: action.to_string(),
        before: summarize(&before),
        after: summarize(&after),
    }
}

pub fn files_under(
    root: &Path,
    cf_name: &str,
) -> Result<Vec<FileReadback>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path().join("vault").join("cf").join(cf_name);
        if !path.exists() {
            continue;
        }
        for file in fs::read_dir(path)? {
            let file = file?.path();
            if file.is_file() {
                files.push(FileReadback {
                    path: file.display().to_string(),
                    bytes: fs::metadata(&file)?.len(),
                });
            }
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

pub fn seed_oracle_fixture(
    vault: &calyx_aster::vault::AsterVault<FixedClock>,
    panel_bits: f32,
    entropy_bits: f32,
) {
    put_sufficiency(vault, &panel(), panel_bits, entropy_bits);
    seed_self_consistency(vault);
}

pub fn seed_prediction_rows(
    vault: &calyx_aster::vault::AsterVault<FixedClock>,
    action: &str,
    outcome: &str,
    count: usize,
    consequence: &str,
) {
    let rows = (0..count)
        .map(|_| row(outcome, None, Some(consequence)))
        .collect::<Vec<_>>();
    write_series(vault, action, &format!("{action}-{outcome}-{count}"), &rows);
}

pub fn seed_edge(
    vault: &calyx_aster::vault::AsterVault<FixedClock>,
    from: &str,
    to: &str,
    outcome: &str,
    grounded: bool,
) {
    write_series(
        vault,
        from,
        &format!("{from}-{to}"),
        &[EdgeRow {
            outcome: "edge".to_string(),
            truth: None,
            consequence: Some(to.to_string()),
            consequence_outcome: outcome.to_string(),
            grounded,
        }],
    );
}

pub fn fsv_root() -> PathBuf {
    std::env::var("LITD_ISSUE1197_FSV_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new("target/fsv/issue1197_engine_what_if").into())
}

fn summarize_outcome(state: &WhatIfPreviewSourceTruth) -> Option<String> {
    state
        .preview_record
        .as_ref()
        .map(|record| match &record.outcome {
            WhatIfPreviewOutcome::Previewed(result) => format!(
                "previewed:nodes={}:low_pruned={}",
                result.diagnostics.nodes, result.diagnostics.low_confidence_pruned
            ),
            WhatIfPreviewOutcome::Rejected(rejection) => {
                format!("rejected:{}", rejection.oracle_error_code)
            }
        })
}

fn summarize(state: &WhatIfPreviewSourceTruth) -> StateSummary {
    let (nodes, low_confidence_pruned, first_child) = match state.preview_record.as_ref() {
        Some(record) => match &record.outcome {
            WhatIfPreviewOutcome::Previewed(result) => (
                Some(result.diagnostics.nodes),
                Some(result.diagnostics.low_confidence_pruned),
                result
                    .tree
                    .children
                    .first()
                    .map(|child| child.root.action_or_event.clone()),
            ),
            WhatIfPreviewOutcome::Rejected(_) => (None, None, None),
        },
        None => (None, None, None),
    };
    StateSummary {
        preview_hash: state.preview_row_hash.clone(),
        base_rows: state.base_rows.len(),
        recurrence_rows: state.recurrence_rows.len(),
        assay_rows: state.assay_rows.len(),
        preview_ledger_rows: state.preview_ledger_rows.len(),
        oracle_ledger_rows: state.oracle_ledger_rows.len(),
        outcome: summarize_outcome(state),
        nodes,
        low_confidence_pruned,
        first_child,
    }
}

#[derive(Serialize)]
pub struct CaseEvidence {
    pub name: &'static str,
    pub expected: &'static str,
    pub action: String,
    pub before: StateSummary,
    pub after: StateSummary,
}

#[derive(Serialize)]
pub struct StateSummary {
    preview_hash: Option<String>,
    base_rows: usize,
    recurrence_rows: usize,
    assay_rows: usize,
    preview_ledger_rows: usize,
    oracle_ledger_rows: usize,
    pub outcome: Option<String>,
    nodes: Option<u64>,
    low_confidence_pruned: Option<bool>,
    first_child: Option<String>,
}

#[derive(Serialize)]
pub struct FileReadback {
    path: String,
    bytes: u64,
}

fn put_sufficiency(
    vault: &calyx_aster::vault::AsterVault<FixedClock>,
    panel: &Panel,
    panel_bits: f32,
    entropy_bits: f32,
) {
    let key = AssayCacheKey::scoped(panel.version, DOMAIN, vault_id(), AnchorKind::Reward);
    let mut store = AssayStore::default();
    store.put(
        key.clone(),
        AssaySubject::Panel,
        estimate(panel_bits, EstimatorKind::PanelSufficiency),
        "issue1197 panel",
        1,
    );
    store.put(
        key.clone(),
        AssaySubject::OutcomeEntropy,
        estimate(entropy_bits, EstimatorKind::OutcomeEntropy),
        "issue1197 entropy",
        1,
    );
    for slot in &panel.slots {
        store.put(
            key.clone(),
            AssaySubject::Lens { slot: slot.slot_id },
            estimate(panel_bits / panel.slots.len() as f32, EstimatorKind::Ksg),
            "issue1197 lens",
            1,
        );
    }
    store.persist_to_vault(vault).unwrap();
}

fn seed_self_consistency(vault: &calyx_aster::vault::AsterVault<FixedClock>) {
    seed_truth_rows(vault, "calibration-a", "Pass", 6);
    seed_truth_rows(vault, "calibration-b", "Pass", 3);
    seed_truth_rows(vault, "calibration-c", "Pass", 2);
    write_series(
        vault,
        "calibration-mixed",
        "calibration-mixed",
        &[
            row("Pass", Some("Pass"), None),
            row("Fail", Some("Fail"), None),
        ],
    );
    for idx in 0..40 {
        let label = if idx % 2 == 0 { "Pass" } else { "Fail" };
        seed_truth_rows(vault, &format!("calibration-single-{idx}"), label, 1);
    }
}

fn seed_truth_rows(
    vault: &calyx_aster::vault::AsterVault<FixedClock>,
    action: &str,
    label: &str,
    count: usize,
) {
    let rows = (0..count)
        .map(|_| row(label, Some(label), None))
        .collect::<Vec<_>>();
    write_series(vault, action, action, &rows);
}

fn write_series(
    vault: &calyx_aster::vault::AsterVault<FixedClock>,
    action: &str,
    series_key: &str,
    rows: &[EdgeRow],
) {
    let cx_id = cx_id(action, series_key);
    vault
        .write_cf(
            ColumnFamily::Base,
            base_key(cx_id),
            encode::encode_constellation_base(&constellation(cx_id, action)).unwrap(),
        )
        .unwrap();
    for (index, row) in rows.iter().enumerate() {
        let occurrence = Occurrence {
            id: OccurrenceId(index as u64),
            t_k: EpochSecs(1_000 + index as i64),
            context: OccurrenceContext::new(context(action, row)).unwrap(),
        };
        vault
            .write_cf(
                ColumnFamily::Recurrence,
                recurrence_key(cx_id, index as u64),
                encode_recurrence_row(&StoredRecurrenceRow::Occurrence(occurrence)).unwrap(),
            )
            .unwrap();
    }
}

#[derive(Clone)]
struct EdgeRow {
    outcome: String,
    truth: Option<String>,
    consequence: Option<String>,
    consequence_outcome: String,
    grounded: bool,
}

fn row(outcome: &str, truth: Option<&str>, consequence: Option<&str>) -> EdgeRow {
    EdgeRow {
        outcome: outcome.to_string(),
        truth: truth.map(ToOwned::to_owned),
        consequence: consequence.map(ToOwned::to_owned),
        consequence_outcome: "NeedsRebalance".to_string(),
        grounded: true,
    }
}

fn context(action: &str, row: &EdgeRow) -> Vec<u8> {
    let mut value =
        json!({"action": action, "oracle_verdict": { "value": { "text": &row.outcome } }});
    if let Some(truth) = &row.truth {
        value["ground_truth_anchor"] = json!({ "value": { "text": truth } });
    }
    if let Some(action_or_event) = &row.consequence {
        value["consequences"] = json!([{
            "action_or_event": action_or_event,
            "domain": DOMAIN,
            "outcome": { "value": { "text": &row.consequence_outcome } },
            "grounded": row.grounded,
            "provisional": !row.grounded
        }]);
    }
    serde_json::to_vec(&value).unwrap()
}

fn estimate(bits: f32, estimator: EstimatorKind) -> MiEstimate {
    MiEstimate::point(bits, 120, estimator, TrustTag::Trusted)
}

fn panel() -> Panel {
    Panel {
        version: 1_197,
        slots: [1_u16, 2].into_iter().map(slot).collect(),
        created_at: TEST_TS,
        kernel_ref: None,
        guard_ref: None,
    }
}

fn slot(id: u16) -> Slot {
    let slot_id = SlotId::new(id);
    Slot {
        slot_id,
        slot_key: SlotKey::new(slot_id, format!("issue1197-slot-{id}")),
        lens_id: LensId::from_bytes([id as u8; 16]),
        shape: SlotShape::Dense(2),
        modality: Modality::Code,
        asymmetry: Asymmetry::None,
        quant: QuantPolicy::None,
        resource: Default::default(),
        axis: Some("issue1197-what-if".to_string()),
        retrieval_only: false,
        excluded_from_dedup: false,
        bits_about: BTreeMap::new(),
        state: SlotState::Active,
        added_at_panel_version: 1_197,
    }
}

fn constellation(cx_id: CxId, action: &str) -> calyx_core::Constellation {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        calyx_oracle::ORACLE_DOMAIN_METADATA_KEY.to_string(),
        DOMAIN.to_string(),
    );
    metadata.insert(
        calyx_oracle::ORACLE_ACTION_METADATA_KEY.to_string(),
        action.to_string(),
    );
    calyx_core::Constellation {
        cx_id,
        vault_id: vault_id(),
        panel_version: 1_197,
        created_at: TEST_TS,
        input_ref: InputRef {
            hash: [cx_id.as_bytes()[0]; 32],
            pointer: None,
            redacted: false,
        },
        modality: Modality::Structured,
        slots: BTreeMap::new(),
        scalars: BTreeMap::new(),
        metadata,
        anchors: Vec::new(),
        provenance: LedgerRef {
            seq: 0,
            hash: [0; 32],
        },
        flags: CxFlags::default(),
    }
}

fn cx_id(action: &str, series_key: &str) -> CxId {
    CxId::from_bytes(content_address([
        DOMAIN.as_bytes(),
        action.as_bytes(),
        series_key.as_bytes(),
    ]))
}

fn vault_id() -> VaultId {
    "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap()
}
