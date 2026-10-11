//! Unit coverage for the sidecar transport that needs no spawned binary. The live
//! round-trip + cross-implementation parity proof is the `calyx_sidecar_fsv` example.

use super::*;
use std::path::PathBuf;

#[test]
fn resolve_binary_reports_missing_with_diagnostic_code() {
    let config = CalyxSidecarConfig::with_binary(PathBuf::from(
        "Z:/no/such/calyx-mcp-addon-binary-9f13.exe",
    ));
    let error = config
        .resolve_binary()
        .expect_err("a nonexistent binary path must fail closed");
    assert_eq!(error.code, SIDECAR_BINARY_MISSING);
    assert!(
        !error.remediation.is_empty(),
        "every sidecar error must carry operator remediation"
    );
}

#[test]
fn resolve_binary_accepts_an_existing_file() {
    // The running test executable is a guaranteed-existing file; resolution only checks
    // that the path is a real file, so this exercises the success branch with no temp dir.
    let exe = std::env::current_exe().expect("test executable path");
    let config = CalyxSidecarConfig::with_binary(exe.clone());
    let resolved = config.resolve_binary().expect("existing file resolves");
    assert_eq!(resolved, exe);
}

#[test]
fn availability_is_total_and_greys_out_when_absent() {
    // Absent addon MUST NOT error: it must return a greyed-out readback with a reason.
    let config = CalyxSidecarConfig::with_binary(PathBuf::from(
        "Z:/no/such/calyx-mcp-addon-binary-4c02.exe",
    ));
    let readback = probe_calyx_sidecar_availability(&config);
    assert!(
        !readback.available,
        "a missing binary must report unavailable"
    );
    assert_eq!(readback.reason_code, SIDECAR_BINARY_MISSING);
    assert_eq!(readback.tool_count, 0);
    assert!(readback.tool_names.is_empty());
    assert!(
        readback.ward_off_by_vacuity,
        "addon absent => no LLM generation => Ward gate off by vacuity (#1790)"
    );
    assert!(readback.reason.contains("absent"));
}
