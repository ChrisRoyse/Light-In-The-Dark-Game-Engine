use litd_engine::{
    calyx_backend::{
        CalyxAccessMode, CalyxBackendContractError, CalyxBackendRole, CalyxRuntimeBoundary,
        engine_calyx_architecture, verify_engine_calyx_architecture,
    },
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1114_calyx_backend_architecture")?;
    let contract = engine_calyx_architecture();
    let readback = verify_engine_calyx_architecture(&contract)?;
    let report = json!({
        "schema": "litd.engine.issue1114.calyx_backend_architecture.v1",
        "schema_version": 1,
        "issue": 1114,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON and hashes"
        },
        "contract": contract,
        "readback": readback,
        "acceptance": {
            "backend_role_count": readback.backend_role_count,
            "hot_path_exclusion_count": readback.hot_path_exclusion_count,
            "lowered_only_hot_paths": readback.lowered_only_hot_paths,
            "transport_readback": readback.transport_readback,
            "cgo_disabled": !contract.transport.cgo_allowed,
            "calyx_source_of_truth_mentions_aster": readback.calyx_source_of_truth.contains("Calyx Aster"),
        },
        "edges": {
            "direct_calyx_in_sim_tick": direct_calyx_in_sim_tick_edge()?,
            "missing_lowering_valve": missing_lowering_valve_edge()?,
            "cgo_allowed": cgo_allowed_edge()?,
            "missing_backend_role": missing_backend_role_edge()?,
        },
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1114_CALYX_BACKEND_ARCHITECTURE_FSV report={} bytes={} sha256={} roles={} hot_path_exclusions={} transport={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["acceptance"]["backend_role_count"],
        readback.json["acceptance"]["hot_path_exclusion_count"],
        readback.json["acceptance"]["transport_readback"],
    );
    Ok(())
}

fn direct_calyx_in_sim_tick_edge() -> Result<Value, CalyxBackendContractError> {
    let mut contract = engine_calyx_architecture();
    let sim_rule = contract
        .boundary_rules
        .iter_mut()
        .find(|rule| rule.boundary == CalyxRuntimeBoundary::DeterministicSimTick)
        .ok_or_else(|| CalyxBackendContractError::Missing {
            field: "boundary_rules",
            reason: "missing deterministic_sim_tick".to_string(),
        })?;
    sim_rule.access_mode = CalyxAccessMode::DirectCalyxBackend;
    expect_invalid(
        verify_engine_calyx_architecture(&contract),
        "boundary_rules.access_mode",
        "deterministic_sim_tick",
    )
}

fn missing_lowering_valve_edge() -> Result<Value, CalyxBackendContractError> {
    let mut contract = engine_calyx_architecture();
    contract
        .boundary_rules
        .retain(|rule| rule.boundary != CalyxRuntimeBoundary::LoweringValve);
    expect_missing(
        verify_engine_calyx_architecture(&contract),
        "boundary_rules",
        "lowering_valve",
    )
}

fn cgo_allowed_edge() -> Result<Value, CalyxBackendContractError> {
    let mut contract = engine_calyx_architecture();
    contract.transport.cgo_allowed = true;
    expect_invalid(
        verify_engine_calyx_architecture(&contract),
        "transport.cgo_allowed",
        "cgo must be disabled",
    )
}

fn missing_backend_role_edge() -> Result<Value, CalyxBackendContractError> {
    let mut contract = engine_calyx_architecture();
    contract
        .roles
        .retain(|role| *role != CalyxBackendRole::LoweringValve);
    expect_missing(
        verify_engine_calyx_architecture(&contract),
        "roles",
        "lowering_valve",
    )
}

fn expect_invalid(
    result: Result<impl Sized, CalyxBackendContractError>,
    expected_field: &'static str,
    reason_contains: &str,
) -> Result<Value, CalyxBackendContractError> {
    match result {
        Err(CalyxBackendContractError::Invalid { field, reason }) => {
            if field != expected_field || !reason.contains(reason_contains) {
                return Err(CalyxBackendContractError::Invalid {
                    field: "fsv.edge",
                    reason: format!(
                        "expected invalid {expected_field} containing {reason_contains}, got {field}: {reason}"
                    ),
                });
            }
            Ok(edge("invalid", field, &reason))
        }
        Err(error) => Err(error),
        Ok(_) => Err(CalyxBackendContractError::Invalid {
            field: "fsv.edge",
            reason: format!("expected invalid {expected_field}"),
        }),
    }
}

fn expect_missing(
    result: Result<impl Sized, CalyxBackendContractError>,
    expected_field: &'static str,
    reason_contains: &str,
) -> Result<Value, CalyxBackendContractError> {
    match result {
        Err(CalyxBackendContractError::Missing { field, reason }) => {
            if field != expected_field || !reason.contains(reason_contains) {
                return Err(CalyxBackendContractError::Invalid {
                    field: "fsv.edge",
                    reason: format!(
                        "expected missing {expected_field} containing {reason_contains}, got {field}: {reason}"
                    ),
                });
            }
            Ok(edge("missing", field, &reason))
        }
        Err(error) => Err(error),
        Ok(_) => Err(CalyxBackendContractError::Invalid {
            field: "fsv.edge",
            reason: format!("expected missing {expected_field}"),
        }),
    }
}

fn edge(kind: &str, field: &str, reason: &str) -> Value {
    json!({
        "after": {
            "error_kind": kind,
            "field": field,
            "reason": reason,
        }
    })
}
