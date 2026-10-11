//! FSV for #825 (tech-tree requirement records) + #829 (object-data lowering valve).
//!
//! Vertical slice, all through the REAL MCP surface: author `Tech` records FROM a base with
//! tier / requires / dependency-equivalent overrides; persist through the object MCP tool into
//! the Aster vault; INDEPENDENT fresh-handle vault readback matches the authored fields; lower
//! the WHOLE tech tree through the #829 valve MCP tool to a graph-validated, fingerprinted
//! litd/data TOML on disk; re-read the TOML (separate MCP tool) and verify every record + the
//! fingerprint.
//!
//! Edge cases (before/after state printed): (1) an out-of-band tier override is typed-rejected
//! at persist time and NOTHING persists; (2) a zero-override copy preserves base tech semantics;
//! (3) re-lowering unchanged data yields an identical fingerprint. Issue-specific graph edges:
//! a DANGLING requirement fails loud, and a requirement CYCLE is rejected with a typed
//! `ObjectRequirementCycle` naming the cycle.
//!
//! Negative control: corrupt the lowered artifact on disk and prove the readback rejects it.

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    mcp_object_tools::{
        COPY_BASE_OBJECT_TOOL, LOWER_TECH_REQUIREMENTS_TOOL, READ_LOWERED_TECH_REQUIREMENTS_TOOL,
        call_mcp_object_tool, read_mcp_object_document,
    },
};
use litd_engine::{EngineError, fsv_artifacts::begin_issue_run};
use serde_json::{Value, json};

struct Ctx {
    document_path: std::path::PathBuf,
    ledger_path: std::path::PathBuf,
}

fn base_template(display: &str) -> Value {
    json!({
        "base_object_id": "base.tech.node",
        "kind": "Tech",
        "display_name": display,
        // Only tier ships in the base (empty list values are not valid single-line properties);
        // requires / equivalents are added per node via overrides.
        "properties": { "tech.tier": "1" },
    })
}

fn copy_tech(
    ctx: &Ctx,
    document_path: &std::path::Path,
    new_object_id: &str,
    rawcode: &str,
    overrides: Value,
    seed: Option<EditorDocument>,
) -> Result<Value, EngineError> {
    let initial = match seed {
        Some(doc) => serde_json::to_value(doc).unwrap(),
        None => Value::Null,
    };
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ctx.ledger_path,
            "ledger_id": "issue825-tech-ledger",
            "request_id": format!("issue825-copy-{new_object_id}"),
            "requested_by": "litd-editor-issue825-fsv",
            "initial_document": initial,
            "new_object_id": new_object_id,
            "rawcode": rawcode,
            "base": base_template(new_object_id),
            "overrides": overrides,
        }),
    )
    .map(|out| out.result)
}

fn lower_tech(document_path: &std::path::Path, artifact_path: &std::path::Path) -> Value {
    call_mcp_object_tool(
        LOWER_TECH_REQUIREMENTS_TOOL,
        json!({ "document_path": document_path, "artifact_path": artifact_path }),
    )
    .expect("lower_tech_requirements MCP tool must succeed")
    .result
}

fn read_lowered(artifact_path: &std::path::Path, fingerprint: &str) -> Value {
    call_mcp_object_tool(
        READ_LOWERED_TECH_REQUIREMENTS_TOOL,
        json!({ "artifact_path": artifact_path, "expected_fingerprint": fingerprint }),
    )
    .expect("read_lowered_tech_requirements MCP tool must succeed")
    .result
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue825_tech_requirements")?;
    let vault = run.artifact_dir("vault")?;
    let ctx = Ctx {
        document_path: vault.join("tech-document-aster"),
        ledger_path: vault.join("tech-ledger-aster"),
    };
    let tree_toml = run.artifact_path("lowered/tech.tree.toml")?;
    let tree_toml_b = run.artifact_path("lowered/tech.tree.redo.toml")?;
    let clone_toml = run.artifact_path("lowered/tech.clone.toml")?;

    // 1. Author a 4-node tech tree FROM the base. town_hall + keep both provide the
    //    "req.townhall_tier" equivalence alias; barracks requires the alias (any town-hall tier)
    //    and footman requires the barracks.
    let seed = EditorDocument::new("calyx://world/issue825-tech-editor", "issue825-rev-1")?;
    copy_tech(
        &ctx,
        &ctx.document_path,
        "tech.town_hall",
        "tTh1",
        json!({ "tech.tier": "1", "tech.equivalents": "req.townhall_tier" }),
        Some(seed),
    )?;
    copy_tech(
        &ctx,
        &ctx.document_path,
        "tech.keep",
        "tKp1",
        json!({ "tech.tier": "2", "tech.requires": "tech.town_hall", "tech.equivalents": "req.townhall_tier" }),
        None,
    )?;
    copy_tech(
        &ctx,
        &ctx.document_path,
        "tech.barracks",
        "tBr1",
        json!({ "tech.tier": "1", "tech.requires": "req.townhall_tier" }),
        None,
    )?;
    copy_tech(
        &ctx,
        &ctx.document_path,
        "tech.footman",
        "tFt1",
        json!({ "tech.tier": "2", "tech.requires": "tech.barracks" }),
        None,
    )?;

    // 2. INDEPENDENT vault readback (fresh handle) confirms the persisted, merged records.
    let persisted = read_mcp_object_document(&ctx.document_path)?;
    let barracks = persisted
        .document
        .objects
        .get("tech.barracks")
        .ok_or("barracks missing from independent vault readback")?;
    let vault_requires = barracks.properties.get("tech.requires").cloned();
    let vault_tier = barracks.properties.get("tech.tier").cloned();

    // 3. Lower the whole tree through the #829 valve, then re-read the TOML from disk.
    let lowered = lower_tech(&ctx.document_path, &tree_toml);
    let fingerprint = lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .ok_or("lower_tech produced no fingerprint")?
        .to_string();
    let disk = read_lowered(&tree_toml, &fingerprint);
    let tree = &disk["lowered_artifact"]["tech_tree"];

    // Edge 3: deterministic re-lowering yields an identical fingerprint.
    let lowered_again = lower_tech(&ctx.document_path, &tree_toml_b);
    let fingerprint_again = lowered_again["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // Edge 1: an out-of-range tier override is rejected AND nothing is persisted.
    let before_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("tech.bad");
    let bad = copy_tech(
        &ctx,
        &ctx.document_path,
        "tech.bad",
        "tBd1",
        json!({ "tech.tier": "99" }),
        None,
    );
    let after_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("tech.bad");

    // Edge 2: zero-override copy preserves base tech semantics in a fresh document.
    let clone_ctx = Ctx {
        document_path: vault.join("tech-clone-aster"),
        ledger_path: vault.join("tech-clone-ledger-aster"),
    };
    let clone_seed = EditorDocument::new("calyx://world/issue825-tech-clone", "issue825-clone-1")?;
    copy_tech(
        &clone_ctx,
        &clone_ctx.document_path,
        "tech.clone",
        "tCl1",
        json!({}),
        Some(clone_seed),
    )?;
    let clone_lowered = lower_tech(&clone_ctx.document_path, &clone_toml);
    let clone_fp = clone_lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let clone_disk = read_lowered(&clone_toml, &clone_fp);
    let clone_record = &clone_disk["lowered_artifact"]["tech_tree"]["records"][0];

    // Graph edge A: a DANGLING requirement fails loud at lowering time.
    let dangling_ctx = Ctx {
        document_path: vault.join("tech-dangling-aster"),
        ledger_path: vault.join("tech-dangling-ledger-aster"),
    };
    let dangling_seed = EditorDocument::new(
        "calyx://world/issue825-tech-dangling",
        "issue825-dangling-1",
    )?;
    copy_tech(
        &dangling_ctx,
        &dangling_ctx.document_path,
        "tech.orphan",
        "tOr1",
        json!({ "tech.requires": "tech.does_not_exist" }),
        Some(dangling_seed),
    )?;
    let dangling = call_mcp_object_tool(
        LOWER_TECH_REQUIREMENTS_TOOL,
        json!({
            "document_path": dangling_ctx.document_path,
            "artifact_path": run.artifact_path("lowered/tech.dangling.toml")?,
        }),
    );

    // Graph edge B: a requirement CYCLE is rejected with a typed ObjectRequirementCycle.
    let cycle_ctx = Ctx {
        document_path: vault.join("tech-cycle-aster"),
        ledger_path: vault.join("tech-cycle-ledger-aster"),
    };
    let cycle_seed = EditorDocument::new("calyx://world/issue825-tech-cycle", "issue825-cycle-1")?;
    copy_tech(
        &cycle_ctx,
        &cycle_ctx.document_path,
        "tech.a",
        "tCa1",
        json!({ "tech.requires": "tech.c" }),
        Some(cycle_seed),
    )?;
    copy_tech(
        &cycle_ctx,
        &cycle_ctx.document_path,
        "tech.b",
        "tCb1",
        json!({ "tech.requires": "tech.a" }),
        None,
    )?;
    copy_tech(
        &cycle_ctx,
        &cycle_ctx.document_path,
        "tech.c",
        "tCc1",
        json!({ "tech.requires": "tech.b" }),
        None,
    )?;
    let cycle_err = call_mcp_object_tool(
        LOWER_TECH_REQUIREMENTS_TOOL,
        json!({
            "document_path": cycle_ctx.document_path,
            "artifact_path": run.artifact_path("lowered/tech.cycle.toml")?,
        }),
    )
    .err();
    let cycle_is_typed = matches!(cycle_err, Some(EngineError::ObjectRequirementCycle { .. }));
    let cycle_named = match &cycle_err {
        Some(EngineError::ObjectRequirementCycle { cycle }) => {
            cycle.contains("tech.a") && cycle.contains("tech.b") && cycle.contains("tech.c")
        }
        _ => false,
    };

    // Negative control: corrupt the lowered tree on disk; the readback MUST reject it.
    let mut corrupt = std::fs::read_to_string(&tree_toml)?;
    corrupt = corrupt.replace("tier = 2", "tier = 3");
    let corrupt_path = run.artifact_path("lowered/tech.corrupt.toml")?;
    if let Some(parent) = corrupt_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&corrupt_path, &corrupt)?;
    let corrupt_rejected = call_mcp_object_tool(
        READ_LOWERED_TECH_REQUIREMENTS_TOOL,
        json!({ "artifact_path": corrupt_path, "expected_fingerprint": fingerprint }),
    )
    .is_err();
    let corrupt_written = corrupt != std::fs::read_to_string(&tree_toml)?;

    let report = json!({
        "schema": "litd.issue825.tech_requirements.v1",
        "issues": [825, 829],
        "source_of_truth": "object MCP tools -> Aster vault -> #829 lowering valve -> graph-validated litd/data tech-tree TOML re-read from disk",
        "happy_path": {
            "fingerprint": fingerprint,
            "vault_readback": { "barracks_requires": vault_requires, "barracks_tier": vault_tier },
            "disk_tree": {
                "node_count": tree["node_count"],
                "edge_count": tree["edge_count"],
                "record_count": tree["records"].as_array().map(|r| r.len()).unwrap_or(0),
                "barracks_requires": tree["records"].as_array().and_then(|records| {
                    records.iter().find(|r| r["tech_id"] == json!("tech.barracks")).map(|r| r["requires"].clone())
                }),
            },
        },
        "edges": {
            "invalid_tier_rejected_nothing_persisted": {
                "before_present": before_bad,
                "copy_errored": bad.is_err(),
                "after_present": after_bad,
                "error": bad.err().map(|e| e.to_string()),
            },
            "zero_override_preserves_base": {
                "clone_tech_id": clone_record["tech_id"],
                "clone_tier": clone_record["tier"],
                "clone_requires": clone_record["requires"],
            },
            "deterministic_relowering": {
                "fingerprint_a": fingerprint,
                "fingerprint_b": fingerprint_again,
            },
        },
        "graph_validation": {
            "dangling_rejected": dangling.is_err(),
            "dangling_error": dangling.err().map(|e| e.to_string()),
            "cycle_rejected_typed": cycle_is_typed,
            "cycle_named": cycle_named,
            "cycle_error": cycle_err.map(|e| e.to_string()),
        },
        "negative_control": {
            "corrupt_written": corrupt_written,
            "corrupt_readback_rejected": corrupt_rejected,
        },
    });

    verify(&report)?;
    let readback = run.write_report(&report)?;
    let bytes = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&bytes)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&bytes) != readback.sha256 {
        return Err("persisted #825 report hash mismatch".into());
    }
    println!(
        "wrote #825/#829 tech requirements FSV to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}

fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let need = |cond: bool, msg: &str| -> Result<(), Box<dyn std::error::Error>> {
        if cond { Ok(()) } else { Err(msg.into()) }
    };
    let happy = &report["happy_path"];
    need(
        happy["vault_readback"]["barracks_requires"] == json!("req.townhall_tier"),
        "vault readback barracks_requires mismatch",
    )?;
    let tree = &happy["disk_tree"];
    need(tree["node_count"] == json!(4), "disk node_count mismatch")?;
    need(tree["edge_count"] == json!(4), "disk edge_count mismatch")?;
    need(
        tree["record_count"] == json!(4),
        "disk record_count mismatch",
    )?;
    need(
        tree["barracks_requires"] == json!(["req.townhall_tier"]),
        "disk barracks_requires mismatch",
    )?;

    let edges = &report["edges"];
    let bad = &edges["invalid_tier_rejected_nothing_persisted"];
    need(
        bad["before_present"] == json!(false)
            && bad["copy_errored"] == json!(true)
            && bad["after_present"] == json!(false),
        "invalid tier override was NOT rejected without persistence",
    )?;
    let clone = &edges["zero_override_preserves_base"];
    need(
        clone["clone_tech_id"] == json!("tech.clone"),
        "clone identity wrong",
    )?;
    need(
        clone["clone_tier"] == json!(1),
        "clone did not preserve base tier",
    )?;
    need(
        clone["clone_requires"] == json!([]),
        "clone did not preserve empty base requires",
    )?;
    let det = &edges["deterministic_relowering"];
    need(
        det["fingerprint_a"] == det["fingerprint_b"]
            && !det["fingerprint_a"].as_str().unwrap_or_default().is_empty(),
        "re-lowering unchanged tech data was not deterministic",
    )?;

    let graph = &report["graph_validation"];
    need(
        graph["dangling_rejected"] == json!(true),
        "dangling requirement was NOT rejected",
    )?;
    need(
        graph["cycle_rejected_typed"] == json!(true) && graph["cycle_named"] == json!(true),
        "requirement cycle was NOT rejected with a typed error naming the cycle",
    )?;

    let neg = &report["negative_control"];
    need(
        neg["corrupt_written"] == json!(true) && neg["corrupt_readback_rejected"] == json!(true),
        "corrupt lowered tech artifact was NOT rejected on readback",
    )?;
    Ok(())
}
