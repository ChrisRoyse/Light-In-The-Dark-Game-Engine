use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use calyx_assay::{
    AssayCacheKey, AssayStore, AssaySubject, EstimatorKind, MiEstimate, PowerCalibration, TrustTag,
};
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
use serde_json::json;

use crate::what_if_preview::{
    WhatIfPreviewOutcome, WhatIfPreviewRequest, WhatIfPreviewService,
    WhatIfPreviewServiceConfig as Config,
};
use crate::{EngineError, fsv_artifacts::begin_issue_run};

const DOMAIN: &str = "issue1197";
const TEST_TS: u64 = 1_785_599_197;

#[test]
fn preview_persists_known_consequence_tree_and_reopens_source_truth() {
    let root = test_root("happy");
    let mut service = open_service(&root);
    seed_oracle_fixture(service.vault(), 1.0, 0.8, 0.9);
    seed_prediction_rows(
        service.vault(),
        "buff_knight_damage",
        "Overpowered",
        12,
        "enemy_ttk_drop",
    );
    seed_edge(
        service.vault(),
        "enemy_ttk_drop",
        "economy_gold_inflation",
        "GoldInflation",
        true,
    );
    seed_edge(
        service.vault(),
        "economy_gold_inflation",
        "raid_reward_rebalance",
        "NeedsRebalance",
        true,
    );

    let report = service
        .preview_change(request("buff_knight_damage"))
        .unwrap();
    let WhatIfPreviewOutcome::Previewed(result) = &report.record.outcome else {
        panic!("expected previewed outcome");
    };
    assert_eq!(
        result.prediction.outcome,
        AnchorValue::Text("Overpowered".to_string())
    );
    assert_eq!(
        result.tree.children[0].root.action_or_event,
        "enemy_ttk_drop"
    );
    assert_eq!(
        result.tree.children[0].children[0].root.action_or_event,
        "economy_gold_inflation"
    );
    assert_eq!(result.diagnostics.repeated_action_paths, 0);
    assert!(result.diagnostics.cycle_guard_satisfied);
    assert!(!result.diagnostics.low_confidence_pruned);
    assert!(report.source_truth.preview_row_hash.is_some());
    assert_eq!(report.source_truth.preview_ledger_rows.len(), 1);
    assert!(!report.source_truth.oracle_ledger_rows.is_empty());

    drop(service);
    let reopened = open_service(&root);
    let readback = reopened
        .read_source_truth(DOMAIN, "buff_knight_damage")
        .unwrap();
    assert_eq!(readback.preview_record, Some(report.record));
    assert!(readback.base_rows.len() >= 4);
    assert!(readback.recurrence_rows.len() >= 15);
    assert!(!readback.assay_rows.is_empty());
}

#[test]
fn insufficient_bits_fail_closed_and_persist_rejection() {
    let root = test_root("insufficient");
    let mut service = open_service(&root);
    seed_oracle_fixture(service.vault(), 0.25, 0.8, 0.9);
    seed_prediction_rows(
        service.vault(),
        "buff_archer_range",
        "Overpowered",
        12,
        "range_lock",
    );

    let error = service
        .preview_change(request("buff_archer_range"))
        .expect_err("insufficient preview must fail closed");
    assert!(matches!(
        error,
        EngineError::OraclePreviewInsufficient { .. }
    ));
    assert_eq!(error.code(), "LITD-ORACLE-PREVIEW-INSUFFICIENT");
    let readback = service
        .read_source_truth(DOMAIN, "buff_archer_range")
        .unwrap();
    let record = readback.preview_record.unwrap();
    match record.outcome {
        WhatIfPreviewOutcome::Rejected(rejection) => {
            assert_eq!(
                rejection.oracle_error_code,
                calyx_oracle::CALYX_ORACLE_INSUFFICIENT
            );
            assert!(rejection.bound.is_some());
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
    assert_eq!(readback.preview_ledger_rows.len(), 1);
}

#[test]
fn cycle_edge_is_guarded_without_revisiting_root() {
    let root = test_root("cycle");
    let mut service = open_service(&root);
    seed_oracle_fixture(service.vault(), 1.0, 0.8, 0.9);
    seed_prediction_rows(
        service.vault(),
        "buff_mage_burst",
        "Overpowered",
        12,
        "cooldown_pressure",
    );
    seed_edge(
        service.vault(),
        "cooldown_pressure",
        "buff_mage_burst",
        "BackEdge",
        true,
    );

    let report = service.preview_change(request("buff_mage_burst")).unwrap();
    let WhatIfPreviewOutcome::Previewed(result) = report.record.outcome else {
        panic!("expected previewed outcome");
    };
    assert_eq!(result.tree.children.len(), 1);
    assert!(result.tree.children[0].children.is_empty());
    assert_eq!(result.diagnostics.repeated_action_paths, 0);
    assert!(result.diagnostics.cycle_guard_satisfied);
}

#[test]
fn low_confidence_prediction_prunes_downstream_tree() {
    let root = test_root("low-confidence");
    let mut service = open_service(&root);
    seed_oracle_fixture(service.vault(), 1.0, 0.8, 0.9);
    seed_prediction_rows(
        service.vault(),
        "buff_rogue_crit",
        "Overpowered",
        1,
        "crit_spike",
    );
    seed_prediction_rows(service.vault(), "buff_rogue_crit", "Fine", 1, "no_change");
    seed_edge(
        service.vault(),
        "crit_spike",
        "loot_spike",
        "LootSpike",
        true,
    );

    let report = service.preview_change(request("buff_rogue_crit")).unwrap();
    let WhatIfPreviewOutcome::Previewed(result) = report.record.outcome else {
        panic!("expected previewed outcome");
    };
    assert!(result.prediction.confidence < 0.1);
    assert!(result.tree.children.is_empty());
    assert!(result.diagnostics.low_confidence_pruned);
}

fn request(action_id: &str) -> WhatIfPreviewRequest {
    WhatIfPreviewRequest {
        domain_id: DOMAIN.to_string(),
        action_id: action_id.to_string(),
        panel: panel(),
        desired_outcome: Some(AnchorValue::Text("NeedsRebalance".to_string())),
    }
}

fn open_service(root: &Path) -> WhatIfPreviewService<FixedClock> {
    WhatIfPreviewService::open_with_clock(
        Config::for_vault(root.join("vault")),
        FixedClock::new(TEST_TS),
    )
    .unwrap()
}

fn seed_oracle_fixture(
    vault: &calyx_aster::vault::AsterVault<FixedClock>,
    panel_bits: f32,
    entropy_bits: f32,
    ceiling: f32,
) {
    put_sufficiency(vault, &panel(), panel_bits, entropy_bits);
    seed_self_consistency(vault, ceiling);
}

fn put_sufficiency(
    vault: &calyx_aster::vault::AsterVault<FixedClock>,
    panel: &Panel,
    panel_bits: f32,
    entropy_bits: f32,
) {
    let key = AssayCacheKey::scoped(panel.version, DOMAIN, vault_id(), AnchorKind::Reward);
    let mut store = AssayStore::default();
    let passing_calibration =
        PowerCalibration::new(1.0, 1.0, 0.5, 120, panel.slots.len(), 0).unwrap();
    store.put(
        key.clone(),
        AssaySubject::Panel,
        estimate(panel_bits, EstimatorKind::PanelSufficiency)
            .with_power_calibration(passing_calibration),
        "issue1197 panel bits",
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
            "issue1197 lens bits",
            1,
        );
    }
    store.persist_to_vault(vault).unwrap();
}

fn seed_self_consistency(vault: &calyx_aster::vault::AsterVault<FixedClock>, ceiling: f32) {
    let _ = ceiling;
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

fn seed_prediction_rows(
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

fn seed_edge(
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
    let mut value = json!({
        "action": action,
        "oracle_verdict": { "value": { "text": &row.outcome } }
    });
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

fn test_root(name: &str) -> PathBuf {
    begin_issue_run(&format!("issue1197_what_if_preview_{name}"))
        .unwrap()
        .run_dir()
        .to_path_buf()
}
