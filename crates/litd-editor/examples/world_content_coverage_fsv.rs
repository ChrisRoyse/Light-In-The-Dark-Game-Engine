use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::world_content_coverage::{
    WorldContentCoverageLedger, WorldContentCoverageReport, WorldCoverageStatus,
    analyze_world_content_coverage, build_world_content_coverage_ledger,
    parse_world_content_coverage_ledger, read_world_content_coverage_ledger,
    write_world_content_coverage_ledger,
};
use serde_json::json;

#[path = "world_content_coverage_fsv/fixtures.rs"]
mod fixtures;

use fixtures::{
    balanced_index, coverage_spec, problem_index, single_item_index, single_item_spec,
    threshold_boundary_index, threshold_boundary_spec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_WORLD_CONTENT_COVERAGE_FSV",
        "issue1202_world_content_coverage",
        "manual-fsv.json",
    )?;
    let root = output_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1202 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let state_path = root.join("world-content-coverage-ledger.json");
    let aster_dir = root.join("aster");
    let before = path_state(&output_path, &state_path, &aster_dir)?;

    let problem = analyze_world_content_coverage(
        "issue1202 deliberate content gap and abundance fixture",
        &problem_index(),
        &coverage_spec("issue1202-fsv-problem"),
    )?;
    let balanced = analyze_world_content_coverage(
        "issue1202 balanced edge fixture",
        &balanced_index(),
        &coverage_spec("issue1202-fsv-balanced"),
    )?;
    let single = analyze_world_content_coverage(
        "issue1202 single-item edge fixture",
        &single_item_index(),
        &single_item_spec(),
    )?;
    let boundary = analyze_world_content_coverage(
        "issue1202 threshold boundary edge fixture",
        &threshold_boundary_index(),
        &threshold_boundary_spec(),
    )?;
    verify_expected(&problem, &balanced, &single, &boundary)?;

    let ledger = build_world_content_coverage_ledger(
        "issue1202-world-content-coverage-fsv",
        vec![
            problem.clone(),
            balanced.clone(),
            single.clone(),
            boundary.clone(),
        ],
    )?;
    let writeback = write_world_content_coverage_ledger(&state_path, &aster_dir, &ledger)?;
    let readback = read_world_content_coverage_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing world-content coverage ledger")?;
    let disk_bytes = fs::read(&state_path)?;
    if disk_bytes != readback.bytes {
        return Err(
            "state file bytes differ from Aster world-content coverage ledger bytes".into(),
        );
    }
    let parsed = parse_world_content_coverage_ledger(&readback.bytes)?;
    verify_ledger(&parsed)?;
    let aster_stats = dir_stats(&aster_dir)?;
    let document = json!({
        "schema": "litd.issue1202.world_content_coverage_fsv.v1",
        "issue": 1202,
        "source_of_truth": "Aster world-content coverage ledger row and JSON mirror readback",
        "research_applied": [
            "content coverage should use explicit measurable buckets/ratios",
            "DDA abundance should report measured and derived signal counts",
            "cross-lens blind spots should be explicit rather than hidden by nearest-neighbor results"
        ],
        "before": before,
        "happy_path": {
            "before": {"places": 2, "items": 3, "expected": "missing mid-water place plus too many swords"},
            "after": summarize(&problem)
        },
        "edges": [
            {"edge": "balanced_world_reports_none", "before": {"places": 1, "items": 1}, "after": summarize(&balanced)},
            {"edge": "single_item_corpus", "before": {"places": 1, "items": 0}, "after": summarize(&single)},
            {"edge": "threshold_boundary_inclusive", "before": {"items": 2, "expected_sword_share_milli": 500}, "after": summarize(&boundary)}
        ],
        "readback": {
            "state_path": state_path,
            "manual_output_path": output_path,
            "state_sha256": sha256_hex(&disk_bytes),
            "aster_sha256": readback.sha256,
            "writeback_sha256": writeback.sha256,
            "state_matches_aster": disk_bytes == readback.bytes,
            "ledger_id": parsed.ledger_id,
            "report_count": parsed.report_count,
            "finding_count": readback.finding_count,
            "graph_rows": readback.graph_rows,
            "seq": readback.seq,
            "aster_file_count": aster_stats.0,
            "aster_file_bytes": aster_stats.1
        }
    });
    write_document_with_final_state(&output_path, document)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote issue1202 world content coverage FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    println!(
        "ledger reports={} findings={} graph_rows={} state_matches_aster={} sha={}",
        parsed.report_count,
        readback.finding_count,
        readback.graph_rows,
        disk_bytes == readback.bytes,
        readback.sha256
    );
    println!(
        "happy status={:?} findings={} blind_spots={} materialized={}",
        problem.status,
        problem.findings.len(),
        problem.lens_blind_spots.len(),
        problem.materialized_count
    );
    Ok(())
}

fn verify_expected(
    problem: &WorldContentCoverageReport,
    balanced: &WorldContentCoverageReport,
    single: &WorldContentCoverageReport,
    boundary: &WorldContentCoverageReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if problem.status != WorldCoverageStatus::Findings
        || bucket(problem, "mid_water_place").match_count != 0
        || bucket(problem, "sword_item").match_count != 3
        || problem.lens_blind_spots.len() != 1
    {
        return Err("problem coverage fixture did not expose expected findings".into());
    }
    if balanced.status != WorldCoverageStatus::Balanced || !balanced.findings.is_empty() {
        return Err("balanced world did not report clean".into());
    }
    if single.status != WorldCoverageStatus::Balanced
        || single.abundance.n_constellations != 1
        || single.materialized_count != 1
    {
        return Err("single-item edge did not stay balanced with one Loom xterm".into());
    }
    if boundary.status != WorldCoverageStatus::Balanced
        || bucket(boundary, "sword_item").share_milli != 500
    {
        return Err("threshold boundary was not inclusive".into());
    }
    Ok(())
}

fn verify_ledger(ledger: &WorldContentCoverageLedger) -> Result<(), Box<dyn std::error::Error>> {
    if ledger.report_count != 4 {
        return Err("ledger did not persist four coverage reports".into());
    }
    verify_expected(
        report(ledger, "issue1202-fsv-problem")?,
        report(ledger, "issue1202-fsv-balanced")?,
        report(ledger, "issue1202-fsv-single")?,
        report(ledger, "issue1202-fsv-boundary")?,
    )
}

fn report<'a>(
    ledger: &'a WorldContentCoverageLedger,
    analysis_id: &str,
) -> Result<&'a WorldContentCoverageReport, Box<dyn std::error::Error>> {
    ledger
        .reports
        .iter()
        .find(|report| report.analysis_id == analysis_id)
        .ok_or_else(|| format!("missing report {analysis_id}").into())
}

fn summarize(report: &WorldContentCoverageReport) -> serde_json::Value {
    json!({
        "analysis_id": report.analysis_id,
        "status": report.status,
        "findings": report.findings,
        "lens_blind_spots": report.lens_blind_spots,
        "buckets": report.bucket_reports,
        "abundance": report.abundance,
        "measured_count": report.measured_count,
        "materialized_count": report.materialized_count,
        "agreement_edges": report.agreement_edges
    })
}

fn bucket<'a>(report: &'a WorldContentCoverageReport, id: &str) -> &'a WorldCoverageBucketReport {
    report
        .bucket_reports
        .iter()
        .find(|bucket| bucket.bucket_id == id)
        .unwrap()
}

use litd_editor::world_content_coverage::WorldCoverageBucketReport;

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
    path: &PathBuf,
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
