use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use calyx_core::{
    BoostConfig, DecayFunction, FusionWeights, PeriodicOptions, SequenceOptions, TemporalPolicy,
};
use litd_editor::temporal_content::{
    TemporalContentItem, TemporalContentLedger, TemporalContentQuery, TemporalContentReport,
    TemporalContentScore, build_temporal_content_ledger, parse_temporal_content_ledger,
    read_temporal_content_ledger, score_temporal_content, write_temporal_content_ledger,
};
use serde_json::json;

const TUESDAY_2024_01_02_14H_UTC: i64 = 1_704_204_000;
const HOUR: i64 = 3_600;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_TEMPORAL_CONTENT_FSV",
        "issue1204_temporal_content",
        "manual-fsv.json",
    )?;
    let root = output_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1204 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let state_path = root.join("temporal-content-ledger.json");
    let aster_dir = root.join("aster");
    let before = path_state(&output_path, &state_path, &aster_dir)?;

    let happy = score_temporal_content(
        "Aster temporal-content ledger row and JSON mirror",
        &fixture_items(),
        &query("issue1204-happy", policy_for_hour(Some(14), Some(1))?),
    )?;
    let boundary = score_temporal_content(
        "boundary-period ledger readback",
        &[item(
            "period_boundary",
            800,
            Some(TUESDAY_2024_01_02_14H_UTC - 15 * HOUR),
            0,
            1,
        )],
        &query("issue1204-boundary", policy_for_hour(Some(0), None)?),
    )?;
    let aperiodic = score_temporal_content(
        "aperiodic-policy ledger readback",
        &[item(
            "aperiodic_no_targets",
            800,
            Some(TUESDAY_2024_01_02_14H_UTC - 6 * HOUR),
            0,
            1,
        )],
        &query("issue1204-aperiodic", policy_for_hour(None, None)?),
    )?;
    let invalid_sequence_error = invalid_sequence_error()?;
    let ledger = build_temporal_content_ledger(
        "issue1204-temporal-content-fsv",
        vec![happy.clone(), boundary.clone(), aperiodic.clone()],
    )?;
    verify_expected(&ledger, &invalid_sequence_error)?;

    let writeback = write_temporal_content_ledger(&state_path, &aster_dir, &ledger)?;
    let readback = read_temporal_content_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing issue1204 temporal-content ledger")?;
    let disk_bytes = fs::read(&state_path)?;
    if disk_bytes != readback.bytes {
        return Err("state file bytes differ from Aster temporal-content ledger bytes".into());
    }
    let parsed = parse_temporal_content_ledger(&readback.bytes)?;
    verify_expected(&parsed, &invalid_sequence_error)?;
    let aster_stats = dir_stats(&aster_dir)?;

    let happy_readback = report(&parsed, "issue1204-happy")?;
    let boundary_readback = report(&parsed, "issue1204-boundary")?;
    let aperiodic_readback = report(&parsed, "issue1204-aperiodic")?;
    let fresh = score(happy_readback, "fresh_periodic_mid")?;
    let one_hour = score(happy_readback, "one_hour_old")?;
    let stale = score(happy_readback, "two_hour_old_tail")?;
    let zero = score(happy_readback, "zero_primary_recent")?;
    let missing = score(happy_readback, "missing_event_time_sequence_only")?;
    let document = json!({
        "schema": "litd.issue1204.temporal_content_fsv.v1",
        "issue": 1204,
        "source_of_truth": "Aster temporal-content ledger row and JSON mirror readback",
        "research_applied": [
            "Calyx AP-60 temporal signals remain post-retrieval and never dominant",
            "Recency uses a half-life decay curve with explicit time-unit policy",
            "Periodic scoring uses circular hour/day distance at period boundaries"
        ],
        "before": before,
        "happy_path": {
            "before": {
                "query_time_secs": TUESDAY_2024_01_02_14H_UTC,
                "target_hour": 14,
                "target_day_of_week": 1,
                "half_life_secs": HOUR,
                "items": fixture_items().iter().map(summarize_item).collect::<Vec<_>>()
            },
            "after": {
                "state_path": state_path,
                "state_sha256": sha256_hex(&disk_bytes),
                "aster_sha256": readback.sha256,
                "writeback_sha256": writeback.sha256,
                "state_matches_aster": disk_bytes == readback.bytes,
                "ledger_id": parsed.ledger_id,
                "report_count": parsed.report_count,
                "item_count": happy_readback.item_count,
                "graph_rows": readback.graph_rows,
                "seq": readback.seq,
                "aster_file_count": aster_stats.0,
                "aster_file_bytes": aster_stats.1,
                "scores": happy_readback.scores.iter().map(summarize_score).collect::<Vec<_>>(),
                "passed": true
            }
        },
        "edges": [
            {
                "edge": "recency_decay_verified",
                "before": {"event_ages_secs": [0, HOUR, 2 * HOUR]},
                "after": {
                    "e2_recency": [
                        fresh.lens.e2_recency,
                        one_hour.lens.e2_recency,
                        stale.lens.e2_recency
                    ],
                    "final_scores": [
                        fresh.final_score_milli,
                        one_hour.final_score_milli,
                        stale.final_score_milli
                    ]
                }
            },
            {
                "edge": "period_boundary_wraparound",
                "before": {"target_hour": 0, "event_hour": 23},
                "after": summarize_score(&boundary_readback.scores[0])
            },
            {
                "edge": "aperiodic_policy_no_targets",
                "before": {"target_hour": null, "target_day_of_week": null},
                "after": summarize_score(&aperiodic_readback.scores[0])
            },
            {
                "edge": "missing_timestamp_does_not_fake_recency_or_periodic",
                "before": {"event_time_secs": null, "sequence_position": 0, "sequence_total": 10},
                "after": summarize_score(missing)
            },
            {
                "edge": "zero_primary_never_dominant",
                "before": {"base_score_milli": 0, "event_time_secs": TUESDAY_2024_01_02_14H_UTC},
                "after": summarize_score(zero)
            },
            {
                "edge": "invalid_sequence_total_rejected",
                "before": {"sequence_total": 0},
                "after": {"error": invalid_sequence_error}
            }
        ]
    });
    write_document_with_final_state(&output_path, document)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote issue1204 temporal content FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    println!(
        "ledger reports={} happy_items={} graph_rows={} state_matches_aster={} sha={}",
        parsed.report_count,
        happy_readback.item_count,
        readback.graph_rows,
        disk_bytes == readback.bytes,
        readback.sha256
    );
    println!(
        "scores fresh={} one_hour={} stale={} missing={} zero={} boundary_hour={:.3} aperiodic_e3={:.3}",
        fresh.final_score_milli,
        one_hour.final_score_milli,
        stale.final_score_milli,
        missing.final_score_milli,
        zero.final_score_milli,
        boundary_readback.scores[0].lens.e3_periodic_hour,
        aperiodic_readback.scores[0].lens.e3_periodic
    );
    Ok(())
}

fn verify_expected(
    ledger: &TemporalContentLedger,
    invalid_sequence_error: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let happy = report(ledger, "issue1204-happy")?;
    let boundary = report(ledger, "issue1204-boundary")?;
    let aperiodic = report(ledger, "issue1204-aperiodic")?;
    if score(happy, "fresh_periodic_mid")?.final_score_milli != 866 {
        return Err("fresh periodic item did not receive expected temporal boost".into());
    }
    if score(happy, "one_hour_old")?.final_score_milli != 846 {
        return Err("one-hour recency decay did not match expected half-life".into());
    }
    if score(happy, "two_hour_old_tail")?.final_score_milli != 821 {
        return Err("two-hour recency decay did not match expected quarter-life".into());
    }
    if score(happy, "zero_primary_recent")?.final_score_milli != 0 {
        return Err("temporal boost created score from zero primary relevance".into());
    }
    let missing = score(happy, "missing_event_time_sequence_only")?;
    if missing.lens.e2_recency != 0.0
        || missing.lens.e3_periodic != 0.0
        || missing.final_score_milli != 725
    {
        return Err("missing timestamp faked recency/periodic evidence".into());
    }
    if (boundary.scores[0].lens.e3_periodic_hour - 11.0 / 12.0).abs() > 1e-6 {
        return Err("period boundary did not use circular hour distance".into());
    }
    if (aperiodic.scores[0].lens.e3_periodic - 1.0).abs() > 1e-6 {
        return Err("aperiodic policy without targets was not neutral".into());
    }
    if !invalid_sequence_error.contains("editor.temporal_content.item") {
        return Err("invalid sequence input did not fail closed with item field".into());
    }
    Ok(())
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
        requested_by: "issue1204-fsv".to_string(),
    }
}

fn policy_for_hour(
    target_hour: Option<u8>,
    target_day_of_week: Option<u8>,
) -> Result<TemporalPolicy, Box<dyn std::error::Error>> {
    Ok(TemporalPolicy::new(
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
    )?)
}

fn invalid_sequence_error() -> Result<String, Box<dyn std::error::Error>> {
    let mut items = fixture_items();
    items[0].sequence_total = 0;
    Ok(score_temporal_content(
        "invalid sequence ledger readback",
        &items,
        &query(
            "issue1204-invalid-sequence",
            policy_for_hour(Some(14), Some(1))?,
        ),
    )
    .unwrap_err()
    .to_string())
}

fn report<'a>(
    ledger: &'a TemporalContentLedger,
    query_id: &str,
) -> Result<&'a TemporalContentReport, Box<dyn std::error::Error>> {
    ledger
        .reports
        .iter()
        .find(|report| report.query_id == query_id)
        .ok_or_else(|| format!("missing report {query_id}").into())
}

fn score<'a>(
    report: &'a TemporalContentReport,
    item_id: &str,
) -> Result<&'a TemporalContentScore, Box<dyn std::error::Error>> {
    report
        .scores
        .iter()
        .find(|score| score.item_id == item_id)
        .ok_or_else(|| format!("missing score {item_id}").into())
}

fn summarize_item(item: &TemporalContentItem) -> serde_json::Value {
    json!({
        "item_id": item.item_id,
        "base_score_milli": item.base_score_milli,
        "event_time_secs": item.event_time_secs,
        "sequence_position": item.sequence_position,
        "sequence_total": item.sequence_total
    })
}

fn summarize_score(score: &TemporalContentScore) -> serde_json::Value {
    json!({
        "item_id": score.item_id,
        "base_score_milli": score.base_score_milli,
        "final_score_milli": score.final_score_milli,
        "event_time_secs": score.event_time_secs,
        "sequence_position": score.sequence_position,
        "sequence_total": score.sequence_total,
        "temporal_active": score.temporal_active,
        "temporal_inactive_reason": score.temporal_inactive_reason,
        "e2_recency": score.lens.e2_recency,
        "e3_periodic_hour": score.lens.e3_periodic_hour,
        "e3_periodic_day": score.lens.e3_periodic_day,
        "e3_periodic": score.lens.e3_periodic,
        "e4_sequence": score.lens.e4_sequence,
        "e4_positional": score.lens.e4_positional,
        "fused_temporal": score.lens.fused_temporal,
        "temporal_multiplier": score.lens.temporal_multiplier,
        "provenance_refs": score.provenance_refs
    })
}

fn path_state(
    output_path: &Path,
    state_path: &Path,
    aster_dir: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let aster_stats = dir_stats(aster_dir)?;
    Ok(json!({
        "output_exists": output_path.exists(),
        "state_exists": state_path.exists(),
        "aster_exists": aster_dir.exists(),
        "aster_file_count": aster_stats.0,
        "aster_file_bytes": aster_stats.1
    }))
}

fn dir_stats(path: &Path) -> Result<(usize, u64), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok((0, 0));
    }
    let mut stack = vec![path.to_path_buf()];
    let mut count = 0;
    let mut bytes = 0;
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                count += 1;
                bytes += metadata.len();
            }
        }
    }
    Ok((count, bytes))
}

fn write_document_with_final_state(
    path: &Path,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    document["after_artifact_write"] = json!({"output_path": path});
    let bytes = serde_json::to_vec_pretty(&document)?;
    fs::write(path, &bytes)?;
    let readback = fs::read(path)?;
    if readback != bytes {
        return Err("manual FSV artifact bytes differed after readback".into());
    }
    Ok(())
}
