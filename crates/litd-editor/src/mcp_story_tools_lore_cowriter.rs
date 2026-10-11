use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    CoWriteLoreArgs, LoreCowriterDraft, LoreCowriterDraftKind, LoreCowriterWardPolicy, LoreEntry,
    LoreEntryKind, LoreEntryLink, LoreTimelineMarker, McpStoryToolCallOutput, invalid_mcp_story,
    mcp_story_tools_store::{
        McpStoryToolRecord, read_mcp_story_state, validate_mcp_story_state, write_mcp_story_state,
        write_mcp_story_tool_ledger,
    },
};

#[path = "mcp_story_tools_lore_cowriter_store.rs"]
mod store;
#[path = "mcp_story_tools_lore_cowriter_support.rs"]
mod support;

pub use store::{
    LORE_COWRITER_LEDGER_SCHEMA, LoreCowriterLedger, LoreCowriterLedgerReadback,
    read_lore_cowriter_ledger, validate_lore_cowriter_ledger,
};
use support::{
    blocked_terms, draft_kind_name, entry_kind, refs, source_refs, tags, terms, timeline_markers,
    validate_args,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreCowriterGrounding {
    pub matched_anchor_ids: Vec<String>,
    pub matched_terms: Vec<String>,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreCowriterWardDecision {
    pub passed: bool,
    pub code: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreCowriterDraftResult {
    pub draft_id: String,
    pub status: String,
    pub entry_id: String,
    pub title: String,
    pub kind: LoreEntryKind,
    pub grounding: LoreCowriterGrounding,
    pub ward_decision: LoreCowriterWardDecision,
    pub lowered_fingerprint: String,
}

#[derive(Debug, Clone)]
struct CanonAnchor {
    anchor_id: String,
    entry_id: Option<String>,
    title: String,
    source_refs: Vec<String>,
    terms: BTreeSet<String>,
}

pub(super) fn co_write_lore_tool(
    route: crate::mcp_tools::McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    super::validate_mutating_route(&route)?;
    let args: CoWriteLoreArgs = super::parse_args(super::CO_WRITE_LORE_TOOL, arguments)?;
    // #1518: confine all three write sinks to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
        ("cowriter_ledger_path", args.cowriter_ledger_path.as_path()),
    ])?;
    super::validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_args(&args)?;

    let state_before = read_mcp_story_state(&args.state_path)?;
    if state_before.state.state_id != args.state_id {
        return Err(invalid_mcp_story(
            "lore_cowriter.state_id",
            "state_id mismatch",
        ));
    }
    if state_before.state.lore_entries.is_empty() {
        return Err(invalid_mcp_story(
            "lore_cowriter.canon",
            "at least one existing lore entry is required for grounding",
        ));
    }

    let anchors = canon_anchors(&state_before.state);
    let mut next_state = state_before.state.clone();
    let mut used_entry_ids = next_state
        .lore_entries
        .iter()
        .map(|entry| entry.entry_id.clone())
        .collect::<BTreeSet<_>>();
    let mut generated = Vec::with_capacity(args.drafts.len());
    let mut results = Vec::with_capacity(args.drafts.len());

    for draft in &args.drafts {
        let (entry, result) = generate_entry(draft, &args, &anchors, &used_entry_ids)?;
        used_entry_ids.insert(entry.entry_id.clone());
        generated.push(entry);
        results.push(result);
    }

    next_state.lore_entries.extend(generated.clone());
    next_state.update_count += 1;
    validate_mcp_story_state(&next_state)?;
    let state_after = write_mcp_story_state(&args.state_path, &next_state)?;
    let source_of_truth = format!(
        "Calyx Aster story state {} sha256 {} -> {}",
        state_after.path, state_before.sha256, state_after.sha256
    );
    let cowriter_ledger = store::build_ledger(store::CowriterLedgerArgs {
        ledger_id: args.cowriter_ledger_id.clone(),
        request_id: args.request_id.clone(),
        requested_by: args.requested_by.clone(),
        source_of_truth,
        state_sha256_before: state_before.sha256.clone(),
        state_sha256_after: state_after.sha256.clone(),
        ward_policy: args.ward_policy.clone(),
        results,
    })?;
    let cowriter_readback =
        store::write_lore_cowriter_ledger(&args.cowriter_ledger_path, cowriter_ledger)?;
    let affected_ids = generated
        .iter()
        .map(|entry| entry.entry_id.clone())
        .collect::<Vec<_>>();
    let record = McpStoryToolRecord {
        request_id: args.request_id.clone(),
        requested_by: args.requested_by.clone(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        state_path: args.state_path.display().to_string(),
        state_sha256: state_after.sha256.clone(),
        state_bytes: state_after.bytes,
        action_summary: "co-wrote Ward-gated original lore entries".to_string(),
        affected_ids,
        character_count: state_after.character_count,
        dialogue_count: state_after.dialogue_count,
        quest_count: state_after.quest_count,
        cinematic_count: state_after.cinematic_count,
        lore_context_count: state_after.lore_context_count,
        lore_entry_count: state_after.lore_entry_count,
        lore_entry_link_count: state_after.lore_entry_link_count,
        lore_timeline_marker_count: state_after.lore_timeline_marker_count,
        lore_surface_count: state_after.lore_surface_count,
        lore_surface_lore_ref_count: state_after.lore_surface_lore_ref_count,
        lore_surface_codex_unlock_count: state_after.lore_surface_codex_unlock_count,
    };
    let ledger_readback = write_mcp_story_tool_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(super::output(
        route,
        "Calyx Aster story state + story MCP ledger + lore co-writer provenance ledger",
        json!({
            "state_readback": state_after.clone(),
            "lore_cowriter_readback": cowriter_readback
        }),
        Some(state_after),
        Some(ledger_readback),
    ))
}

fn generate_entry(
    draft: &LoreCowriterDraft,
    args: &CoWriteLoreArgs,
    anchors: &[CanonAnchor],
    used_entry_ids: &BTreeSet<String>,
) -> Result<(LoreEntry, LoreCowriterDraftResult), EngineError> {
    let blocked = blocked_terms(&draft.prompt, &args.ward_policy.blocked_terms);
    if !blocked.is_empty() {
        return Err(invalid_mcp_story(
            "lore_cowriter.ward",
            format!("LITD-LORE-COWRITER-BLOCKED: {}", blocked.join("|")),
        ));
    }
    let grounding = evaluate_grounding(&draft.prompt, anchors);
    let ward_decision = ward_decision(&grounding, &args.ward_policy);
    if !ward_decision.passed {
        return Err(invalid_mcp_story(
            "lore_cowriter.grounding",
            format!(
                "{}: {}",
                ward_decision.code,
                ward_decision.reasons.join("; ")
            ),
        ));
    }
    let entry_id = draft
        .requested_entry_id
        .clone()
        .unwrap_or_else(|| format!("cowrite_{}", draft.draft_id));
    if used_entry_ids.contains(&entry_id) {
        return Err(invalid_mcp_story(
            "lore_cowriter.entry_id",
            format!("generated lore entry {entry_id} already exists"),
        ));
    }
    let kind = entry_kind(draft.kind);
    let title = generated_title(draft, &grounding, anchors);
    let source_refs = source_refs(&grounding);
    let links = grounding
        .matched_anchor_ids
        .iter()
        .filter_map(|anchor_id| anchors.iter().find(|anchor| &anchor.anchor_id == anchor_id))
        .filter_map(|anchor| anchor.entry_id.as_ref().map(|entry_id| (entry_id, anchor)))
        .take(4)
        .map(|(entry_id, anchor)| LoreEntryLink {
            target_entry_id: entry_id.clone(),
            relation: "grounded_in".to_string(),
            description: format!("Grounded in existing canon anchor {}", anchor.title),
        })
        .collect::<Vec<_>>();
    let timeline_markers = timeline_markers(&entry_id, draft.kind, draft.seed);
    let mut metadata = BTreeMap::new();
    metadata.insert("generated_by".to_string(), "lore_cowriter".to_string());
    metadata.insert(
        "draft_kind".to_string(),
        draft_kind_name(draft.kind).to_string(),
    );
    metadata.insert(
        "ward_policy_id".to_string(),
        args.ward_policy.policy_id.clone(),
    );
    metadata.insert(
        "grounding_anchor_count".to_string(),
        grounding.matched_anchor_ids.len().to_string(),
    );
    metadata.insert(
        "matched_terms".to_string(),
        grounding.matched_terms.join("_"),
    );
    let summary = format!(
        "Original {} lore grounded in {}.",
        draft_kind_name(draft.kind),
        grounding.matched_terms.join(", ")
    );
    let entry = LoreEntry {
        entry_id: entry_id.clone(),
        kind: kind.clone(),
        title: title.clone(),
        summary,
        body_rich_text: body_text(&title, draft, &grounding, anchors),
        metadata,
        source_refs,
        tags: tags(draft, &grounding),
        links,
        timeline_markers,
    };
    let lowered_fingerprint = lowered_fingerprint(&entry, &grounding, &ward_decision)?;
    let result = LoreCowriterDraftResult {
        draft_id: draft.draft_id.clone(),
        status: "admitted".to_string(),
        entry_id,
        title,
        kind,
        grounding,
        ward_decision,
        lowered_fingerprint,
    };
    Ok((entry, result))
}

fn canon_anchors(state: &super::mcp_story_tools_store::McpStoryState) -> Vec<CanonAnchor> {
    let mut anchors = Vec::new();
    for entry in &state.lore_entries {
        let mut text = format!(
            "{} {} {} {}",
            entry.entry_id, entry.title, entry.summary, entry.body_rich_text
        );
        for (key, value) in &entry.metadata {
            text.push(' ');
            text.push_str(key);
            text.push(' ');
            text.push_str(value);
        }
        for tag in &entry.tags {
            text.push(' ');
            text.push_str(tag);
        }
        anchors.push(CanonAnchor {
            anchor_id: format!("lore_entry_{}", entry.entry_id),
            entry_id: Some(entry.entry_id.clone()),
            title: entry.title.clone(),
            source_refs: refs(
                &format!("lore://entry/{}", entry.entry_id),
                &entry.source_refs,
            ),
            terms: terms(&text),
        });
    }
    for surface in &state.lore_surfaces {
        anchors.push(CanonAnchor {
            anchor_id: format!("lore_surface_{}", surface.surface_id),
            entry_id: surface.lore_entry_ids.first().cloned(),
            title: surface.title.clone(),
            source_refs: refs(
                &format!("lore://surface/{}", surface.surface_id),
                &surface.source_refs,
            ),
            terms: terms(&format!(
                "{} {} {} {}",
                surface.surface_id, surface.title, surface.excerpt, surface.body_rich_text
            )),
        });
    }
    anchors
}

fn evaluate_grounding(prompt: &str, anchors: &[CanonAnchor]) -> LoreCowriterGrounding {
    let prompt_terms = terms(prompt);
    let mut matched_anchor_ids = BTreeSet::new();
    let mut matched_terms = BTreeSet::new();
    let mut source_refs = BTreeSet::new();
    for anchor in anchors {
        let overlap = prompt_terms
            .intersection(&anchor.terms)
            .cloned()
            .collect::<Vec<_>>();
        if !overlap.is_empty() {
            matched_anchor_ids.insert(anchor.anchor_id.clone());
            matched_terms.extend(overlap);
            source_refs.extend(anchor.source_refs.iter().cloned());
        }
    }
    LoreCowriterGrounding {
        matched_anchor_ids: matched_anchor_ids.into_iter().collect(),
        matched_terms: matched_terms.into_iter().collect(),
        source_refs: source_refs.into_iter().collect(),
    }
}

fn ward_decision(
    grounding: &LoreCowriterGrounding,
    ward: &LoreCowriterWardPolicy,
) -> LoreCowriterWardDecision {
    let mut reasons = Vec::new();
    if grounding.matched_anchor_ids.len() < ward.min_source_anchors {
        reasons.push(format!(
            "matched {} anchors, need {}",
            grounding.matched_anchor_ids.len(),
            ward.min_source_anchors
        ));
    }
    if grounding.matched_terms.len() < ward.min_grounded_terms {
        reasons.push(format!(
            "matched {} terms, need {}",
            grounding.matched_terms.len(),
            ward.min_grounded_terms
        ));
    }
    if reasons.is_empty() {
        LoreCowriterWardDecision {
            passed: true,
            code: "LITD-WARD-PASS".to_string(),
            reasons,
        }
    } else {
        LoreCowriterWardDecision {
            passed: false,
            code: "LITD-WARD-INSUFFICIENT-GROUNDING".to_string(),
            reasons,
        }
    }
}

fn body_text(
    title: &str,
    draft: &LoreCowriterDraft,
    grounding: &LoreCowriterGrounding,
    anchors: &[CanonAnchor],
) -> String {
    let anchor_titles = grounding
        .matched_anchor_ids
        .iter()
        .filter_map(|id| anchors.iter().find(|anchor| &anchor.anchor_id == id))
        .map(|anchor| anchor.title.as_str())
        .take(3)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "## {title}\n{title} is original {kind} lore drafted from the creator theme: {prompt}.\n\nCanon anchors: {anchors}.\n\nIt keeps faith with {terms} while adding new names, motives, and story hooks for the Ashen Reach.",
        kind = draft_kind_name(draft.kind),
        prompt = draft.prompt.trim(),
        anchors = anchor_titles,
        terms = grounding.matched_terms.join(", ")
    )
}

fn generated_title(
    draft: &LoreCowriterDraft,
    grounding: &LoreCowriterGrounding,
    anchors: &[CanonAnchor],
) -> String {
    let name = generated_name(draft.seed, draft.kind);
    let anchor = grounding
        .matched_anchor_ids
        .first()
        .and_then(|id| anchors.iter().find(|anchor| &anchor.anchor_id == id))
        .map(|anchor| anchor.title.as_str())
        .unwrap_or("Ashen Reach");
    match draft.kind {
        LoreCowriterDraftKind::Deity => format!("{name}, Lantern of {anchor}"),
        LoreCowriterDraftKind::Cabal => format!("The {name} Covenant"),
        LoreCowriterDraftKind::History => format!("The {name} Chronicle"),
        LoreCowriterDraftKind::Legend => format!("Legend of {name} and {anchor}"),
    }
}

fn generated_name(seed: u64, kind: LoreCowriterDraftKind) -> String {
    let deity = ["Oratha", "Velmorn", "Iraveth", "Caldris"];
    let cabal = ["Cinderfold", "Glassward", "Mournveil", "Starwrit"];
    let history = ["Third Bell", "Ashen Compact", "Last Ember", "Veil Oath"];
    let legend = ["Grey Lantern", "Silent Crown", "Ember Road", "Hollow Star"];
    let table = match kind {
        LoreCowriterDraftKind::Deity => &deity,
        LoreCowriterDraftKind::Cabal => &cabal,
        LoreCowriterDraftKind::History => &history,
        LoreCowriterDraftKind::Legend => &legend,
    };
    table[(seed as usize) % table.len()].to_string()
}

fn lowered_fingerprint(
    entry: &LoreEntry,
    grounding: &LoreCowriterGrounding,
    ward: &LoreCowriterWardDecision,
) -> Result<String, EngineError> {
    let bytes = serde_json::to_vec(&(entry, grounding, ward)).map_err(|error| {
        EngineError::SerializationFailed {
            format: "lore-cowriter-lowered-json",
            reason: error.to_string(),
        }
    })?;
    Ok(sha256_hex(&bytes))
}

#[cfg(test)]
#[path = "mcp_story_tools_lore_cowriter_tests.rs"]
mod tests;
