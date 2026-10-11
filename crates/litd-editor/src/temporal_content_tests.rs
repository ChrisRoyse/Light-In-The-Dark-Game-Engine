use std::fs;

use calyx_core::{
    BoostConfig, DecayFunction, FusionWeights, PeriodicOptions, SequenceOptions, TemporalPolicy,
};

use super::*;

const TUESDAY_2024_01_02_14H_UTC: i64 = 1_704_204_000;
const HOUR: i64 = 3_600;

#[test]
fn scores_recency_periodic_and_position_lenses() {
    let items = fixture_items();
    let report = score_temporal_content(
        "Aster temporal-content ledger row and JSON mirror",
        &items,
        &query("issue1204-happy", policy_for_hour(Some(14), Some(1))),
    )
    .unwrap();

    let fresh = score(&report, "fresh_periodic_mid");
    let one_hour = score(&report, "one_hour_old");
    let stale = score(&report, "two_hour_old_tail");

    assert_eq!(report.item_count, 5);
    assert_eq!(report.scores[0].item_id, "fresh_periodic_mid");
    assert_eq!(fresh.final_score_milli, 866);
    assert_eq!(one_hour.final_score_milli, 846);
    assert_eq!(stale.final_score_milli, 821);
    assert_close(fresh.lens.e2_recency, 1.0, 1e-6);
    assert_close(one_hour.lens.e2_recency, 0.5, 1e-6);
    assert_close(stale.lens.e2_recency, 0.25, 1e-6);
    assert_close(fresh.lens.e3_periodic, 1.0, 1e-6);
    assert_close(one_hour.lens.e3_periodic_hour, 11.0 / 12.0, 1e-6);
    assert_close(fresh.lens.e4_sequence, 0.5, 1e-6);
    assert_close(fresh.lens.temporal_multiplier, 0.0825, 1e-6);
    println!(
        "issue1204 happy before={{query_time:{TUESDAY_2024_01_02_14H_UTC}, items:{}}} after={{top:{}, top_final:{}, e2:[{:.3},{:.3},{:.3}], e3_hour_one_hour:{:.3}}}",
        items.len(),
        report.scores[0].item_id,
        fresh.final_score_milli,
        fresh.lens.e2_recency,
        one_hour.lens.e2_recency,
        stale.lens.e2_recency,
        one_hour.lens.e3_periodic_hour
    );
}

#[test]
fn edge_cases_cover_aperiodic_boundary_recency_and_zero_primary() {
    let happy = score_temporal_content(
        "Aster temporal-content ledger row and JSON mirror",
        &fixture_items(),
        &query("issue1204-edges", policy_for_hour(Some(14), Some(1))),
    )
    .unwrap();
    let zero = score(&happy, "zero_primary_recent");
    let missing_time = score(&happy, "missing_event_time_sequence_only");
    assert_eq!(zero.final_score_milli, 0);
    assert_eq!(
        zero.temporal_inactive_reason.as_deref(),
        Some("zero_primary_score")
    );
    assert_eq!(missing_time.final_score_milli, 725);
    assert_eq!(missing_time.lens.e2_recency, 0.0);
    assert_eq!(missing_time.lens.e3_periodic, 0.0);
    assert_eq!(missing_time.lens.e4_sequence, 1.0);

    let boundary_report = score_temporal_content(
        "boundary-period ledger readback",
        &[item(
            "period_boundary",
            800,
            Some(TUESDAY_2024_01_02_14H_UTC - 15 * HOUR),
            0,
            1,
        )],
        &query("issue1204-boundary", policy_for_hour(Some(0), None)),
    )
    .unwrap();
    let boundary = &boundary_report.scores[0];
    assert_close(boundary.lens.e3_periodic_hour, 11.0 / 12.0, 1e-6);
    assert_close(boundary.lens.e3_periodic_day, 1.0, 1e-6);

    let aperiodic_report = score_temporal_content(
        "aperiodic-policy ledger readback",
        &[item(
            "aperiodic_no_targets",
            800,
            Some(TUESDAY_2024_01_02_14H_UTC - 6 * HOUR),
            0,
            1,
        )],
        &query("issue1204-aperiodic", policy_for_hour(None, None)),
    )
    .unwrap();
    let aperiodic = &aperiodic_report.scores[0];
    assert_close(aperiodic.lens.e3_periodic, 1.0, 1e-6);
    assert!(aperiodic.final_score_milli > 800);
    println!(
        "issue1204 edges zero_final={} missing_time={{e2:{},e3:{},e4:{},final:{}}} boundary_hour={:.3} aperiodic_e3={:.3}",
        zero.final_score_milli,
        missing_time.lens.e2_recency,
        missing_time.lens.e3_periodic,
        missing_time.lens.e4_sequence,
        missing_time.final_score_milli,
        boundary.lens.e3_periodic_hour,
        aperiodic.lens.e3_periodic
    );
}

#[test]
fn invalid_sequence_total_fails_closed() {
    let mut items = fixture_items();
    items[0].sequence_total = 0;
    let error = score_temporal_content(
        "Aster temporal-content ledger row and JSON mirror",
        &items,
        &query("issue1204-invalid", policy_for_hour(Some(14), Some(1))),
    )
    .unwrap_err();
    assert!(error.to_string().contains("editor.temporal_content.item"));
    println!("issue1204 invalid_sequence_total error={error}");
}

#[test]
fn ledger_persists_to_aster_and_reads_back() {
    let happy = score_temporal_content(
        "Aster temporal-content ledger row and JSON mirror",
        &fixture_items(),
        &query("issue1204-ledger-happy", policy_for_hour(Some(14), Some(1))),
    )
    .unwrap();
    let boundary = score_temporal_content(
        "boundary-period ledger readback",
        &[item(
            "period_boundary",
            800,
            Some(TUESDAY_2024_01_02_14H_UTC - 15 * HOUR),
            0,
            1,
        )],
        &query("issue1204-ledger-boundary", policy_for_hour(Some(0), None)),
    )
    .unwrap();
    let ledger =
        build_temporal_content_ledger("issue1204-temporal-content-test", vec![happy, boundary])
            .unwrap();
    let root = test_root("temporal-content");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("temporal-content-ledger-legacy.json");
    let aster_dir = root.join("aster");

    let writeback = write_temporal_content_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let readback = read_temporal_content_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_temporal_content_ledger(&readback.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(readback.bytes, writeback.bytes);
    assert_eq!(parsed.report_count, 2);
    assert_eq!(parsed.reports[0].scores[0].item_id, "fresh_periodic_mid");
    assert_eq!(parsed.reports[0].scores[0].final_score_milli, 866);
    println!(
        "issue1204 aster reports={} graph_rows={} sha={} state_bytes={}",
        parsed.report_count,
        readback.graph_rows,
        readback.sha256,
        readback.bytes.len()
    );
}

fn fixture_items() -> Vec<TemporalContentItem> {
    vec![
        item(
            "fresh_periodic_mid",
            800,
            Some(TUESDAY_2024_01_02_14H_UTC),
            5,
            10,
        ),
        item(
            "one_hour_old",
            800,
            Some(TUESDAY_2024_01_02_14H_UTC - HOUR),
            5,
            10,
        ),
        item(
            "two_hour_old_tail",
            800,
            Some(TUESDAY_2024_01_02_14H_UTC - 2 * HOUR),
            10,
            10,
        ),
        item(
            "zero_primary_recent",
            0,
            Some(TUESDAY_2024_01_02_14H_UTC),
            0,
            10,
        ),
        item("missing_event_time_sequence_only", 700, None, 0, 10),
    ]
}

fn item(
    item_id: &str,
    base_score_milli: u16,
    event_time_secs: Option<i64>,
    sequence_position: u64,
    sequence_total: u64,
) -> TemporalContentItem {
    TemporalContentItem {
        item_id: item_id.to_string(),
        title: item_id.replace('_', " "),
        base_score_milli,
        event_time_secs,
        sequence_position,
        sequence_total,
        provenance_refs: vec![format!("Aster://issue1204/{item_id}")],
    }
}

fn query(query_id: &str, policy: TemporalPolicy) -> TemporalContentQuery {
    TemporalContentQuery {
        query_id: query_id.to_string(),
        query_time_secs: TUESDAY_2024_01_02_14H_UTC,
        tz_offset_secs: 0,
        policy,
        requested_by: "issue1204-test".to_string(),
    }
}

fn policy_for_hour(target_hour: Option<u8>, target_day_of_week: Option<u8>) -> TemporalPolicy {
    TemporalPolicy::new(
        true,
        DecayFunction::Exponential {
            half_life_secs: HOUR as u64,
        },
        PeriodicOptions {
            target_hour,
            target_day_of_week,
            use_now: false,
        },
        SequenceOptions::default(),
        FusionWeights::default(),
        BoostConfig::default(),
        true,
    )
    .unwrap()
}

fn score<'a>(report: &'a TemporalContentReport, item_id: &str) -> &'a TemporalContentScore {
    report
        .scores
        .iter()
        .find(|score| score.item_id == item_id)
        .unwrap()
}

fn assert_close(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual} expected={expected} tolerance={tolerance}"
    );
}

// #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
fn test_root(name: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(&format!("editor-{name}"))
}
