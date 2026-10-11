use super::*;

pub(super) fn run_headless(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: RunHeadlessHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_run_args(&args)?;
    // #1518: confine the extra project source path (read by the headless run) to the root.
    confine_tool_paths(&[("project_path", Path::new(&args.project_path))])?;
    let mut state = load_state(&args.context)?;
    let record = build_run_record(
        &args.run_id,
        &args.project_path,
        args.camera_rig,
        args.ticks,
        args.until_condition.as_ref(),
    )?;
    upsert_by(
        &mut state.runs,
        |run| run.run_id == record.run_id,
        record.clone(),
    );
    finish_mutation(
        route,
        args.context,
        state,
        "ran deterministic headless harness simulation",
        vec![record.run_id.clone()],
        json!({ "run": record }),
    )
}

pub(super) fn snapshot_restore_branch(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: SnapshotBranchHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_snapshot_args(&args)?;
    let mut state = load_state(&args.context)?;
    let run = find_run(&state, &args.run_id)?.clone();
    let record =
        build_snapshot_branch_record(&run, &args.snapshot_id, &args.branch_id, args.branch_ticks)?;
    upsert_by(
        &mut state.snapshots,
        |item| item.snapshot_id == record.snapshot_id && item.branch_id == record.branch_id,
        record.clone(),
    );
    finish_mutation(
        route,
        args.context,
        state,
        "restored a verified snapshot and ran an isolated branch",
        vec![record.snapshot_id.clone(), record.branch_id.clone()],
        json!({ "snapshot_branch": record }),
    )
}

pub(super) fn assert_state(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: AssertHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_assert_args(&args)?;
    let mut state = load_state(&args.context)?;
    let run = find_run(&state, &args.run_id)?;
    let verdict = build_assertion_verdict(&args.verdict_id, run, &args.assertions)?;
    upsert_by(
        &mut state.assertions,
        |item| item.verdict_id == verdict.verdict_id,
        verdict.clone(),
    );
    let feedback = feedback_from_assertion_verdict(&verdict)?;
    finish_mutation_with_feedback(
        route,
        args.context,
        state,
        "evaluated declarative harness assertions against stored run state",
        vec![verdict.verdict_id.clone()],
        json!({ "assertion_verdict": verdict }),
        feedback,
    )
}

pub(super) fn read_events(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    let args: ReadHarnessEventsArgs = parse_args(&route.tool_name, arguments)?;
    validate_read_events_args(&args)?;
    // #1518: confine the read path to the project root before reading from disk.
    confine_tool_paths(&[("state_path", Path::new(&args.state_path))])?;
    let readback = read_mcp_harness_state(&args.state_path)?;
    let after = args.after_event_index.unwrap_or_default() as u64;
    let limit = args.limit.unwrap_or(100);
    let events = readback
        .state
        .events
        .iter()
        .filter(|event| event.event_index > after)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    Ok(output(
        route,
        "MCP agent harness state JSON",
        json!({ "events": events }),
        Some(readback),
        None,
    ))
}

pub(super) fn run_oracles(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: RunOraclesHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_oracle_args(&args)?;
    let mut state = load_state(&args.context)?;
    let run = find_run(&state, &args.run_id)?;
    let record = build_oracle_record(&args.oracle_run_id, run, &args.oracles)?;
    upsert_by(
        &mut state.oracle_runs,
        |item| item.oracle_run_id == record.oracle_run_id,
        record.clone(),
    );
    let feedback = feedback_from_oracle_run(&record)?;
    finish_mutation_with_feedback(
        route,
        args.context,
        state,
        "ran deterministic harness oracles against stored run state",
        vec![record.oracle_run_id.clone()],
        json!({ "oracle_run": record }),
        feedback,
    )
}

pub(super) fn drive_bot(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: DriveBotHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_bot_args(&args)?;
    let mut state = load_state(&args.context)?;
    let run = find_run(&state, &args.run_id)?;
    let record = build_bot_record(&args, run)?;
    upsert_by(
        &mut state.bot_runs,
        |item| item.bot_run_id == record.bot_run_id,
        record.clone(),
    );
    let feedback = feedback_from_bot_drive_record(&record)?;
    finish_mutation_with_feedback(
        route,
        args.context,
        state,
        "drove a restored simulation with a deterministic virtual-player bot",
        vec![record.bot_run_id.clone()],
        json!({ "bot_run": record }),
        feedback,
    )
}

pub(super) fn screenshot_diff(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: ScreenshotDiffHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_screenshot_args(&args)?;
    // #1518: confine the baseline/candidate PNG read paths to the project root.
    confine_tool_paths(&[
        ("baseline_path", Path::new(&args.baseline_path)),
        ("candidate_path", Path::new(&args.candidate_path)),
    ])?;
    let mut state = load_state(&args.context)?;
    let record = build_screenshot_diff_record(&args)?;
    upsert_by(
        &mut state.screenshot_diffs,
        |item| item.diff_id == record.diff_id,
        record.clone(),
    );
    finish_mutation(
        route,
        args.context,
        state,
        "rendered/read PNG screenshots and recorded decoded-pixel visual diff metrics",
        vec![record.diff_id.clone()],
        json!({ "screenshot_diff": record }),
    )
}

pub(super) fn calyx_signals(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: CalyxSignalsHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_calyx_args(&args)?;
    let mut state = load_state(&args.context)?;
    let run = args
        .run_id
        .as_deref()
        .map(|run_id| find_run(&state, run_id))
        .transpose()?;
    let record = build_calyx_signals_record(&args.signal_batch_id, run, &args.authored_content)?;
    validate_calyx_signals_record(&record)?;
    upsert_by(
        &mut state.calyx_signals,
        |item| item.signal_batch_id == record.signal_batch_id,
        record.clone(),
    );
    let feedback = feedback_from_calyx_signals(&record)?;
    finish_mutation_with_feedback(
        route,
        args.context,
        state,
        "derived Calyx-facing harness signals from persisted run state",
        vec![record.signal_batch_id.clone()],
        json!({ "calyx_signals": record }),
        feedback,
    )
}

pub(super) fn run_pev(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    validate_mutating_route(&route)?;
    let args: RunPevHarnessArgs = parse_args(&route.tool_name, arguments)?;
    validate_pev_args(&args)?;
    // #1518: confine the extra project source path used by the single-shot PEV run.
    confine_tool_paths(&[("project_path", Path::new(&args.project_path))])?;
    let mut state = load_state(&args.context)?;
    if args.turns.is_some() {
        let outcome = build_pev_loop_transcript(&args, state)?;
        let feedback = feedback_from_pev_transcript(&outcome.transcript)?;
        return finish_mutation_with_transcript_and_feedback(FinishMutationArgs {
            route,
            context: args.context,
            state: outcome.state,
            summary: "orchestrated a multi-turn plan-execute-verify loop transcript",
            affected_ids: vec![outcome.transcript.transcript_id.clone()],
            result: json!({ "pev_transcript": outcome.transcript }),
            transcript_turns: outcome.transcript_turns,
            feedback,
        });
    }
    let run = build_run_record(
        &args.run_id,
        &args.project_path,
        args.camera_rig,
        args.ticks,
        None,
    )?;
    let assertions =
        build_assertion_verdict(&format!("{}-assert", args.pev_id), &run, &args.assertions)?;
    let oracles = build_oracle_record(&format!("{}-oracle", args.pev_id), &run, &args.oracles)?;
    let signals = build_calyx_signals_record(&format!("{}-signals", args.pev_id), Some(&run), &[])?;
    if assertions.failed_count != 0 || oracles.failed_count != 0 || !signals.admitted {
        return invalid_harness(
            "pev.verdict",
            format!(
                "PEV failed before commit: assertions_failed={} oracles_failed={} admitted={}",
                assertions.failed_count, oracles.failed_count, signals.admitted
            ),
        );
    }
    let pev = HarnessPevRunRecord {
        pev_id: args.pev_id.clone(),
        run_id: run.run_id.clone(),
        run_record: run.clone(),
        assertion_verdict: assertions.clone(),
        oracle_verdict: oracles.clone(),
        calyx_signals: signals.clone(),
        verdict: "passed".to_string(),
    };
    let run_key = run.run_id.clone();
    let assertion_key = assertions.verdict_id.clone();
    let oracle_key = oracles.oracle_run_id.clone();
    let signal_key = signals.signal_batch_id.clone();
    upsert_by(&mut state.runs, |item| item.run_id == run_key, run);
    upsert_by(
        &mut state.assertions,
        |item| item.verdict_id == assertion_key,
        assertions,
    );
    upsert_by(
        &mut state.oracle_runs,
        |item| item.oracle_run_id == oracle_key,
        oracles,
    );
    upsert_by(
        &mut state.calyx_signals,
        |item| item.signal_batch_id == signal_key,
        signals,
    );
    upsert_by(
        &mut state.pev_runs,
        |item| item.pev_id == pev.pev_id,
        pev.clone(),
    );
    finish_mutation(
        route,
        args.context,
        state,
        "executed a transactional plan-execute-verify harness loop",
        vec![pev.pev_id.clone(), pev.run_id.clone()],
        json!({ "pev": pev }),
    )
}

pub(super) fn read_harness(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    let args: ReadHarnessArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine the read path to the project root before reading from disk.
    confine_tool_paths(&[("state_path", Path::new(&args.state_path))])?;
    let readback = read_mcp_harness_state(&args.state_path)?;
    Ok(output(
        route,
        "MCP agent harness state JSON",
        json!({ "state_readback": readback }),
        Some(readback),
        None,
    ))
}

pub(super) fn load_state(context: &HarnessMutationContext) -> Result<McpHarnessState, EngineError> {
    // #1518: confine all three context write sinks — state_path, ledger_path, and the aster
    // ledger dir — to the project root before any fs effect. Every mutation handler funnels
    // through load_state, so this is the single choke point for the shared context paths;
    // finish_mutation writes all three, and confining here runs before that write.
    confine_tool_paths(&[
        ("state_path", Path::new(&context.state_path)),
        ("ledger_path", Path::new(&context.ledger_path)),
        ("aster_dir", Path::new(&context.aster_dir)),
    ])?;
    load_or_create_mcp_harness_state(&context.state_path, &context.state_id)
}

pub(super) fn find_run<'a>(
    state: &'a McpHarnessState,
    run_id: &str,
) -> Result<&'a HarnessRunRecord, EngineError> {
    state
        .runs
        .iter()
        .find(|run| run.run_id == run_id)
        .ok_or_else(|| {
            invalid_harness::<()>("run_id", format!("missing run {run_id}")).unwrap_err()
        })
}
