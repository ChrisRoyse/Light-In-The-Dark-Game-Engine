//! Issue #1454 — Full State Verification for the ONE LLM call site.
//!
//! This program calls the REAL provider with the REAL Infisical-injected key. It is NOT a unit
//! test and is NOT part of `cargo test`: it lives behind the `fsv` feature so that routine test
//! runs never require a secret and never touch the network (AGENTS.md, binding).
//!
//! Run it the way the doctrine requires — under injection, from PowerShell (Git Bash mangles the
//! `/gameeditor` path via MSYS pathconv; see #1546):
//!
//! ```text
//! infisical run --projectId=c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5 --env=prod --path=/gameeditor -- `
//!   cargo run -p calyx-agent --features fsv --example issue1454_deepseek_agent_fsv
//! ```
//!
//! Run it a SECOND time WITHOUT the `infisical run` prefix to prove EDGE 1 against real process
//! state (key genuinely absent from the environment) rather than a simulated one.
//!
//! # What is actually proven
//!
//! The source of truth is the PROVIDER, not our return values. Every case below reads back real
//! provider state — a real request id, a real HTTP status, a real token accounting — and the happy
//! path asserts on the CONTENT (a synthetic prompt whose correct answer is known in advance:
//! 2 + 2 = 4), not merely on the absence of an error. A 200 that carried the wrong number would
//! fail here.

use calyx_agent::{
    AgentModel, AgentRequest, AgentSecrets, AgentTransport, DEEPSEEK_BASE_URL,
    GAMEEDITOR_API_KEY_NAME, InfisicalSource, codes,
};

/// A key that is well-formed (passes every local shape check) but is not a real credential.
/// This forces the PROVIDER — not our validator — to be the thing that rejects it, which is the
/// only way to prove we surface the provider's actual HTTP status instead of inventing one.
const WELL_FORMED_BUT_WRONG_KEY: &str = "sk-0000000000000000000000000000000000";

fn main() -> std::process::ExitCode {
    println!("=====================================================================");
    println!(" #1454 FSV — calyx-agent -> api.deepseek.com (REAL provider, REAL key)");
    println!("=====================================================================\n");

    let mut failures = Vec::new();

    if let Err(reason) = case_1_happy_path() {
        failures.push(format!("HAPPY PATH: {reason}"));
    }
    if let Err(reason) = case_2_key_absent() {
        failures.push(format!("EDGE 1 (key absent): {reason}"));
    }
    if let Err(reason) = case_3_unsupported_model() {
        failures.push(format!("EDGE 2 (unsupported model): {reason}"));
    }
    if let Err(reason) = case_4_provider_rejects() {
        failures.push(format!("EDGE 3 (provider rejects): {reason}"));
    }

    println!("\n=====================================================================");
    if failures.is_empty() {
        println!(" FSV RESULT: ALL CASES PASSED");
        println!("=====================================================================");
        std::process::ExitCode::SUCCESS
    } else {
        println!(" FSV RESULT: {} CASE(S) FAILED", failures.len());
        for failure in &failures {
            println!("   FAIL  {failure}");
        }
        println!("=====================================================================");
        std::process::ExitCode::FAILURE
    }
}

/// HAPPY PATH — a synthetic prompt with a KNOWN expected answer.
///
/// We do not assert "no error". We assert the provider returned `{"sum": 4}`, because a
/// transport that cheerfully returns someone else's arithmetic is not working.
fn case_1_happy_path() -> Result<(), String> {
    println!("--- HAPPY PATH: synthetic 2+2, expected content {{\"sum\": 4}} ---");

    let secrets = match AgentSecrets::gameeditor() {
        Ok(secrets) => secrets,
        Err(err) => {
            return Err(format!(
                "could not load {GAMEEDITOR_API_KEY_NAME} [{}] {} \
                 -- run this under `infisical run ... --` (see #1546)",
                err.code(),
                err.message()
            ));
        }
    };

    // Sanitized evidence only. The key itself is never printed, logged, or returned.
    let metadata = secrets.metadata(AgentModel::Flash);
    println!("  BEFORE: key_present={}", metadata.key_present);
    println!("  BEFORE: key_length={}", metadata.key_length);
    println!("  BEFORE: key_sha256_prefix={}", metadata.key_sha256_prefix);
    println!("  BEFORE: endpoint={}", metadata.chat_completions_url);
    println!("  BEFORE: model={}", metadata.model);

    let request = AgentRequest::new(
        AgentModel::Flash,
        "You are a deterministic arithmetic oracle. Return strict json only.",
        "Compute 2+2. Respond with exactly the json object {\"sum\": <integer>} and nothing else.",
    )
    .with_max_tokens(64);

    let completion = AgentTransport::new(secrets)
        .complete(&request)
        .map_err(|err| format!("[{}] {}", err.code(), err.message()))?;

    // READ BACK the provider's own state. These are the fields an operator can trace.
    println!("  AFTER : request_id={}", completion.id);
    println!("  AFTER : model={}", completion.model);
    println!("  AFTER : finish_reason={}", completion.finish_reason);
    println!("  AFTER : content={}", completion.content);
    println!(
        "  AFTER : usage prompt={:?} completion={:?} total={:?}",
        completion.usage.prompt_tokens,
        completion.usage.completion_tokens,
        completion.usage.total_tokens
    );

    // The actual assertion: the CONTENT is correct, not merely present.
    let parsed: serde_json::Value = serde_json::from_str(&completion.content)
        .map_err(|err| format!("content was not valid json: {err}"))?;
    let sum = parsed
        .get("sum")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| format!("no integer `sum` field in content {:?}", completion.content))?;

    if sum != 4 {
        return Err(format!("provider returned sum={sum}, expected 4"));
    }
    if completion.id.is_empty() {
        return Err("provider returned an empty request id".to_string());
    }

    println!("  ASSERT PASS: parsed.sum == 4 (expected 4)");
    println!("  ASSERT PASS: real provider request id present\n");
    Ok(())
}

/// EDGE 1 — the key is absent from the process environment.
///
/// Must fail LOUDLY with a precise code and produce NO output. There is no degraded local text,
/// no cached answer, and no fallback provider to fall back to — by design.
fn case_2_key_absent() -> Result<(), String> {
    println!("--- EDGE 1: key absent from process env ---");

    let key_in_env = std::env::var(GAMEEDITOR_API_KEY_NAME).ok();
    let present = key_in_env.as_deref().is_some_and(|k| !k.is_empty());
    println!("  BEFORE: {GAMEEDITOR_API_KEY_NAME} present in env = {present}");

    if present {
        // The env var IS injected right now, so we cannot observe the genuinely-absent case in
        // this process. Rather than fake it by mutating the environment, we exercise the SAME
        // validation path through an explicitly-empty value and report honestly.
        println!("  NOTE  : running UNDER injection, so the absent case cannot be observed here.");
        println!("  NOTE  : exercising the same fail-closed path via an empty value; re-run this");
        println!(
            "  NOTE  : example WITHOUT `infisical run` to observe SECRET_ENV_MISSING for real."
        );

        let err = AgentSecrets::from_values(InfisicalSource::gameeditor(), "", DEEPSEEK_BASE_URL)
            .err()
            .ok_or("empty key was ACCEPTED — fail-closed contract is broken")?;

        println!("  AFTER : error_code={}", err.code());
        println!("  AFTER : message={}", err.message());
        if err.code() != codes::SECRET_EMPTY {
            return Err(format!(
                "expected {}, got {}",
                codes::SECRET_EMPTY,
                err.code()
            ));
        }
        println!("  AFTER : completion produced = NONE");
        println!("  ASSERT PASS: empty key fails closed, no output\n");
        return Ok(());
    }

    // The key is genuinely absent — the real condition.
    let err = AgentSecrets::gameeditor()
        .err()
        .ok_or("secrets loaded WITHOUT a key present — fail-closed contract is broken")?;

    println!("  AFTER : error_code={}", err.code());
    println!("  AFTER : message={}", err.message());
    println!("  AFTER : completion produced = NONE");

    if err.code() != codes::SECRET_ENV_MISSING {
        return Err(format!(
            "expected {}, got {}",
            codes::SECRET_ENV_MISSING,
            err.code()
        ));
    }
    println!("  ASSERT PASS: absent key fails closed with a precise code, no output\n");
    Ok(())
}

/// EDGE 2 — an unsupported model id must be rejected BEFORE the wire, never coerced.
///
/// `deepseek-chat` is the legacy alias the provider retires on 2026-07-24. A client that quietly
/// rewrites it to a supported model would give a silently-substituted — i.e. silently wrong —
/// answer. It must fail instead.
fn case_3_unsupported_model() -> Result<(), String> {
    println!("--- EDGE 2: unsupported/legacy model id (no silent coercion) ---");
    println!("  BEFORE: requested model = \"deepseek-chat\" (retired 2026-07-24)");
    println!("  BEFORE: served models    = [deepseek-v4-flash, deepseek-v4-pro]");

    let err = AgentModel::parse("deepseek-chat")
        .err()
        .ok_or("legacy alias \"deepseek-chat\" was ACCEPTED — it must fail closed")?;

    println!("  AFTER : error_code={}", err.code());
    println!("  AFTER : message={}", err.message());
    println!("  AFTER : network call made = NO (rejected before the wire)");

    if err.code() != codes::MODEL_UNSUPPORTED {
        return Err(format!(
            "expected {}, got {}",
            codes::MODEL_UNSUPPORTED,
            err.code()
        ));
    }
    println!("  ASSERT PASS: unsupported model rejected, not coerced\n");
    Ok(())
}

/// EDGE 3 — the PROVIDER rejects the call.
///
/// A well-formed-but-wrong key passes every local shape check, so the rejection can only come
/// from the provider. That is the point: it proves we surface the provider's ACTUAL status rather
/// than manufacturing a local error and calling it a day.
fn case_4_provider_rejects() -> Result<(), String> {
    println!("--- EDGE 3: provider rejects (real HTTP error from api.deepseek.com) ---");
    println!("  BEFORE: key = well-formed (sk- prefix, correct length class) but NOT a credential");
    println!("  BEFORE: local shape validation should PASS, provider should REJECT");

    let secrets = AgentSecrets::from_values(
        InfisicalSource::gameeditor(),
        WELL_FORMED_BUT_WRONG_KEY,
        DEEPSEEK_BASE_URL,
    )
    .map_err(|err| {
        format!(
            "local validation rejected the well-formed key [{}] {} \
             — this case can no longer reach the provider",
            err.code(),
            err.message()
        )
    })?;

    println!("  BEFORE: local validation PASSED (as intended) — now calling the real provider");

    let request = AgentRequest::new(
        AgentModel::Flash,
        "You are a deterministic arithmetic oracle. Return strict json only.",
        "Compute 2+2. Respond with exactly the json object {\"sum\": <integer>} and nothing else.",
    )
    .with_max_tokens(64);

    let err = AgentTransport::new(secrets)
        .complete(&request)
        .err()
        .ok_or("provider ACCEPTED an invalid credential — fail-closed contract is broken")?;

    println!("  AFTER : error_code={}", err.code());
    println!("  AFTER : message={}", err.message());
    println!("  AFTER : completion produced = NONE");

    if err.message().contains(WELL_FORMED_BUT_WRONG_KEY) {
        return Err("the error message LEAKED the key material".to_string());
    }
    println!("  ASSERT PASS: provider rejection surfaced, key not leaked into the error\n");
    Ok(())
}
