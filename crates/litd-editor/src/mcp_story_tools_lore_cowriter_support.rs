use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use super::{
    CoWriteLoreArgs, LoreCowriterDraft, LoreCowriterDraftKind, LoreCowriterGrounding,
    LoreCowriterWardPolicy, LoreEntryKind, LoreTimelineMarker, invalid_mcp_story,
};

const MAX_DRAFTS: usize = 16;
const MAX_BLOCKED_TERMS: usize = 64;
const MAX_PROMPT_LEN: usize = 4096;

pub(super) fn validate_args(args: &CoWriteLoreArgs) -> Result<(), EngineError> {
    validate_path("cowriter_ledger_path", &args.cowriter_ledger_path)?;
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(&args.cowriter_ledger_id) {
        return Err(invalid_mcp_story(
            "lore_cowriter.cowriter_ledger_id",
            "co-writer ledger id must be row-safe",
        ));
    }
    validate_ward(&args.ward_policy)?;
    if args.drafts.is_empty() || args.drafts.len() > MAX_DRAFTS {
        return Err(invalid_mcp_story(
            "lore_cowriter.drafts",
            format!("draft count must be in 1..={MAX_DRAFTS}"),
        ));
    }
    let mut draft_ids = BTreeSet::new();
    let mut entry_ids = BTreeSet::new();
    for draft in &args.drafts {
        validate_symbol("lore_cowriter.draft_id", &draft.draft_id)?;
        validate_single_line("lore_cowriter.prompt", &draft.prompt)?;
        if !draft_ids.insert(draft.draft_id.as_str()) {
            return Err(invalid_mcp_story(
                "lore_cowriter.draft_id",
                format!("duplicate draft {}", draft.draft_id),
            ));
        }
        if let Some(entry_id) = &draft.requested_entry_id {
            validate_symbol("lore_cowriter.requested_entry_id", entry_id)?;
            if !entry_ids.insert(entry_id.as_str()) {
                return Err(invalid_mcp_story(
                    "lore_cowriter.requested_entry_id",
                    format!("duplicate requested entry {entry_id}"),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_mcp_story(field, "path must be non-empty"));
    }
    if path.is_file() {
        return Err(invalid_mcp_story(
            field,
            format!(
                "{} is a file; expected a Calyx Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

pub(super) fn terms(text: &str) -> BTreeSet<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| term.len() >= 4)
        .collect()
}

pub(super) fn blocked_terms(prompt: &str, blocked: &[String]) -> Vec<String> {
    let lower = prompt.to_ascii_lowercase();
    blocked
        .iter()
        .filter(|term| lower.contains(&term.to_ascii_lowercase()))
        .cloned()
        .collect()
}

pub(super) fn refs(primary: &str, refs: &[String]) -> Vec<String> {
    let mut out = vec![primary.to_string()];
    out.extend(refs.iter().cloned());
    out
}

pub(super) fn source_refs(grounding: &LoreCowriterGrounding) -> Vec<String> {
    let mut refs = vec!["issue_1053".to_string()];
    refs.extend(grounding.matched_anchor_ids.iter().cloned());
    refs.into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn tags(draft: &LoreCowriterDraft, grounding: &LoreCowriterGrounding) -> Vec<String> {
    let mut tags = vec![
        "ai_cowriter".to_string(),
        "original_lore".to_string(),
        draft_kind_name(draft.kind).to_string(),
    ];
    tags.extend(grounding.matched_terms.iter().take(5).cloned());
    tags.into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn timeline_markers(
    entry_id: &str,
    kind: LoreCowriterDraftKind,
    seed: u64,
) -> Vec<LoreTimelineMarker> {
    if !matches!(kind, LoreCowriterDraftKind::History) {
        return Vec::new();
    }
    vec![LoreTimelineMarker {
        marker_id: format!("{entry_id}_marker"),
        label: "Co-written canon seed".to_string(),
        order: (seed % 10_000) as i32,
        description: "Grounded history draft accepted by the lore co-writer Ward gate".to_string(),
    }]
}

pub(super) fn entry_kind(kind: LoreCowriterDraftKind) -> LoreEntryKind {
    match kind {
        LoreCowriterDraftKind::Deity => LoreEntryKind::Deity,
        LoreCowriterDraftKind::Cabal => LoreEntryKind::Cabal,
        LoreCowriterDraftKind::History => LoreEntryKind::History,
        LoreCowriterDraftKind::Legend => LoreEntryKind::Event,
    }
}

pub(super) fn draft_kind_name(kind: LoreCowriterDraftKind) -> &'static str {
    match kind {
        LoreCowriterDraftKind::Deity => "deity",
        LoreCowriterDraftKind::Cabal => "cabal",
        LoreCowriterDraftKind::History => "history",
        LoreCowriterDraftKind::Legend => "legend",
    }
}

fn validate_ward(ward: &LoreCowriterWardPolicy) -> Result<(), EngineError> {
    validate_symbol("lore_cowriter.ward.policy_id", &ward.policy_id)?;
    if ward.min_source_anchors == 0
        || ward.min_grounded_terms == 0
        || ward.blocked_terms.len() > MAX_BLOCKED_TERMS
    {
        return Err(invalid_mcp_story(
            "lore_cowriter.ward",
            "Ward thresholds must be non-zero and blocked terms bounded",
        ));
    }
    for term in &ward.blocked_terms {
        validate_single_line("lore_cowriter.ward.blocked_terms", term)?;
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > MAX_PROMPT_LEN || value.contains(['\n', '\r', '\t'])
    {
        return Err(invalid_mcp_story(
            field,
            "value must be non-empty bounded single-line text",
        ));
    }
    Ok(())
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_mcp_story(field, "symbol cannot be empty"));
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(invalid_mcp_story(
            field,
            "symbol must be an ASCII identifier",
        ));
    }
    Ok(())
}
