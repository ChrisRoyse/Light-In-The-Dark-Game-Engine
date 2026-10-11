//! Shared helpers for Calyx signal verdict construction.

use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::super::{
    HarnessCalyxAuthoredContent, HarnessCalyxPanelReadback, HarnessCalyxSignalStatus,
    HarnessCalyxSignalSubsystem, HarnessCalyxSignalVerdict,
    mcp_harness_tools_validation::invalid_harness,
};

const MIN_CONTENT_LENSES: usize = 10;

pub(super) struct MatchEvidence {
    pub terms: Vec<String>,
    pub sources: Vec<String>,
}

pub(super) fn grounded_bits_milli(grounding: &crate::authoring::GroundingReport) -> u32 {
    let terms = grounding.matched_terms.len() as u32 * 150;
    let anchors = grounding.matched_anchor_ids.len() as u32 * 250;
    let sources = grounding.source_refs.len() as u32 * 100;
    terms + anchors + sources
}

pub(super) fn matched_terms(query: &str, content: &HarnessCalyxAuthoredContent) -> MatchEvidence {
    let query_terms = token_set(query);
    let mut matched = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for term in content
        .corpus
        .lore_terms
        .iter()
        .chain(content.corpus.factions.iter())
        .chain(std::iter::once(&content.corpus.region))
    {
        let term_tokens = token_set(term);
        if !term_tokens.is_empty() && term_tokens.iter().all(|term| query_terms.contains(term)) {
            matched.insert(term.to_ascii_lowercase());
        }
    }
    for anchor in &content.corpus.anchors {
        for term in &anchor.terms {
            let term_tokens = token_set(term);
            if !term_tokens.is_empty() && term_tokens.iter().all(|term| query_terms.contains(term))
            {
                matched.insert(term.to_ascii_lowercase());
                sources.insert(anchor.source_ref.clone());
            }
        }
    }
    MatchEvidence {
        terms: matched.into_iter().collect(),
        sources: sources.into_iter().collect(),
    }
}

pub(super) fn content_terms(content: &HarnessCalyxAuthoredContent) -> BTreeSet<String> {
    token_set(&format!(
        "{} {} {} {}",
        content.request.prompt,
        content.candidate.display_name,
        content.candidate.summary,
        content.candidate.tags.join(" ")
    ))
}

pub(super) fn token_set(text: &str) -> BTreeSet<String> {
    let normalized = text
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>();
    normalized
        .split_whitespace()
        .filter(|token| token.len() > 2)
        .map(ToString::to_string)
        .collect()
}

pub(super) fn validate_panel(panel: &HarnessCalyxPanelReadback) -> Result<(), EngineError> {
    if panel.template_name.trim().is_empty()
        || panel.template_status != "gate_passed"
        || panel.content_lens_count < MIN_CONTENT_LENSES
        || panel.association_family_count < 2
        || panel.lens_names.len() != panel.content_lens_count
        || panel.source_ref.trim().is_empty()
        || !is_sha256(&panel.source_sha256)
    {
        return invalid_harness(
            "calyx_signals.panel",
            "panel readback must be a gate-passed >=10-lens A37 template with source hash",
        );
    }
    let unique = panel.lens_names.iter().collect::<BTreeSet<_>>();
    if unique.len() != panel.lens_names.len() {
        return invalid_harness("calyx_signals.panel", "panel lens names must be unique");
    }
    Ok(())
}

pub(super) fn validate_same_panel(
    expected: Option<&HarnessCalyxPanelReadback>,
    actual: &HarnessCalyxPanelReadback,
) -> Result<(), EngineError> {
    if let Some(expected) = expected
        && expected != actual
    {
        return invalid_harness(
            "calyx_signals.panel",
            "all authored content in one signal batch must use the same panel readback",
        );
    }
    Ok(())
}

pub(super) fn signal(
    content_id: &str,
    subsystem: HarnessCalyxSignalSubsystem,
    status: HarnessCalyxSignalStatus,
    code: impl Into<String>,
    message: impl Into<String>,
    evidence: Vec<String>,
    remediation: impl Into<String>,
) -> HarnessCalyxSignalVerdict {
    HarnessCalyxSignalVerdict {
        content_id: content_id.to_string(),
        subsystem,
        status,
        code: code.into(),
        message: message.into(),
        evidence,
        remediation: remediation.into(),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
