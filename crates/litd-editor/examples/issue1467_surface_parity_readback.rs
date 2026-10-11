use std::{error::Error, fs};

use litd_editor::{
    mcp_tools::expected_calyx_mcp_tool_names,
    structured_capability_coverage::{
        build_structured_capability_coverage_audit, parse_structured_capability_coverage_audit,
        read_structured_capability_coverage_audit, write_structured_capability_coverage_audit,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::json;

fn main() -> Result<(), Box<dyn Error>> {
    let run = begin_issue_run("issue1467_mcp_gui_parity")?;
    let root = run.run_dir().to_path_buf();

    let audit = build_structured_capability_coverage_audit(
        "issue1467-surface-parity",
        "manual FSV: live MCP registry plus #1467 GUI/MCP surface declarations",
        expected_calyx_mcp_tool_names()?,
    )?;
    let store_dir = root.join("coverage-aster");
    let writeback = write_structured_capability_coverage_audit(&root, &store_dir, &audit)?;
    let loaded = read_structured_capability_coverage_audit(&store_dir, &audit.audit_id)?
        .ok_or("coverage audit missing after write")?;
    let parsed = parse_structured_capability_coverage_audit(&loaded.bytes)?;
    if parsed != audit || loaded.sha256 != writeback.sha256 {
        return Err("coverage audit readback did not match writeback".into());
    }

    let audit_path = root.join("structured-capability-coverage-audit.json");
    fs::write(&audit_path, serde_json::to_vec_pretty(&parsed)?)?;
    let summary = json!({
        "issue": 1467,
        "source": "structured_capability_coverage Calyx write/readback",
        "run_dir": root,
        "audit_path": audit_path,
        "store_dir": store_dir,
        "audit_sha256": sha256_hex(&fs::read(&audit_path)?),
        "aster_sha256": loaded.sha256,
        "surface_declaration_count": parsed.surface_parity.declaration_count,
        "both_surface_count": parsed.surface_parity.both_surface_count,
        "documented_exemption_count": parsed.surface_parity.documented_exemption_count,
        "missing_mcp_surface_declarations": parsed.surface_parity.missing_mcp_surface_declarations,
        "undeclared_single_surface_capabilities": parsed.surface_parity.undeclared_single_surface_capabilities,
        "passed": parsed.surface_parity.missing_mcp_surface_declarations.is_empty()
            && parsed.surface_parity.undeclared_single_surface_capabilities.is_empty()
    });
    let summary_path = root.join("surface-parity-readback.json");
    let summary_bytes = serde_json::to_vec_pretty(&summary)?;
    fs::write(&summary_path, &summary_bytes)?;
    let persisted_summary_bytes = fs::read(&summary_path)?;
    if persisted_summary_bytes != summary_bytes {
        return Err("surface parity summary changed after physical write".into());
    }
    let persisted_summary: serde_json::Value = serde_json::from_slice(&persisted_summary_bytes)?;
    if persisted_summary["passed"] != true {
        return Err(format!(
            "surface parity failed closed: missing_mcp={} undeclared_single_surface={}",
            persisted_summary["missing_mcp_surface_declarations"],
            persisted_summary["undeclared_single_surface_capabilities"]
        )
        .into());
    }
    let summary_sha = sha256_hex(&persisted_summary_bytes);

    println!("SURFACE_PARITY_READBACK={}", summary_path.display());
    println!("SURFACE_PARITY_SHA256={summary_sha}");
    println!("SURFACE_PARITY_RUN={}", root.display());
    Ok(())
}
