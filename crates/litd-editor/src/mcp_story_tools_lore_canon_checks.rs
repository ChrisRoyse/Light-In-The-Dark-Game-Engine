use std::collections::BTreeSet;

use super::{
    LoreCanonClaim, LoreCanonClaimFinding, LoreCanonHit, LoreCanonQuery, LoreCanonQueryKind,
    LoreGroundedAnswer, refs,
};
use crate::mcp_story_tools::{LoreEntry, mcp_story_tools_store::McpStoryState};

pub(super) fn check_claim(claim: &LoreCanonClaim, state: &McpStoryState) -> LoreCanonClaimFinding {
    let Some(subject) = state
        .lore_entries
        .iter()
        .find(|entry| entry.entry_id == claim.subject_entry_id)
    else {
        return finding(
            claim,
            "out_of_distribution",
            vec![],
            "missing subject entry",
        );
    };
    if let (Some(key), Some(value)) = (&claim.metadata_key, &claim.metadata_value) {
        return check_metadata_claim(claim, subject, key, value);
    }
    check_link_claim(claim, state, subject)
}

pub(super) fn grounded_answer(
    query: &LoreCanonQuery,
    hits: &[LoreCanonHit],
    contradictions: usize,
    ood: usize,
) -> Option<LoreGroundedAnswer> {
    if !matches!(query.kind, LoreCanonQueryKind::GroundedAnswer)
        || hits.is_empty()
        || contradictions > 0
        || ood > 0
    {
        return None;
    }
    let selected = hits.iter().take(3).collect::<Vec<_>>();
    let answer_text = selected
        .iter()
        .map(|hit| format!("{}: {}", hit.title, hit.excerpt))
        .collect::<Vec<_>>()
        .join(" ");
    Some(LoreGroundedAnswer {
        answer_text,
        source_ids: selected.iter().map(|hit| hit.source_id.clone()).collect(),
        evidence_refs: selected
            .iter()
            .flat_map(|hit| hit.source_refs.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    })
}

pub(super) fn query_status(
    kind: LoreCanonQueryKind,
    hits: usize,
    contradictions: usize,
    ood: usize,
) -> String {
    if contradictions > 0 {
        "contradiction".to_string()
    } else if ood > 0 || hits == 0 {
        "out_of_distribution".to_string()
    } else if matches!(kind, LoreCanonQueryKind::GroundedAnswer) {
        "grounded_answer".to_string()
    } else {
        "grounded".to_string()
    }
}

fn check_metadata_claim(
    claim: &LoreCanonClaim,
    subject: &LoreEntry,
    key: &str,
    value: &str,
) -> LoreCanonClaimFinding {
    match subject.metadata.get(key) {
        Some(actual) if actual == value => finding(
            claim,
            "supported",
            refs("lore://entry", &subject.entry_id, &[]),
            "metadata matches canon",
        ),
        Some(actual) => finding(
            claim,
            "contradiction",
            refs("lore://entry", &subject.entry_id, &[]),
            format!("metadata {key} is {actual}, not {value}"),
        ),
        None => finding(
            claim,
            "unverified",
            vec![],
            format!("metadata {key} is absent"),
        ),
    }
}

fn check_link_claim(
    claim: &LoreCanonClaim,
    state: &McpStoryState,
    subject: &LoreEntry,
) -> LoreCanonClaimFinding {
    let Some(target_id) = &claim.target_entry_id else {
        return finding(claim, "unverified", vec![], "claim has no target entry");
    };
    if !state
        .lore_entries
        .iter()
        .any(|entry| entry.entry_id == *target_id)
    {
        return finding(claim, "out_of_distribution", vec![], "missing target entry");
    }
    let relation = claim.relation.as_deref().unwrap_or_default();
    if subject
        .links
        .iter()
        .any(|link| link.target_entry_id == *target_id && link.relation == relation)
    {
        return finding(
            claim,
            "supported",
            link_ref(subject, target_id, relation),
            "link matches canon",
        );
    }
    if let Some(conflict) = subject.links.iter().find(|link| {
        link.target_entry_id == *target_id && relations_conflict(&link.relation, relation)
    }) {
        return finding(
            claim,
            "contradiction",
            link_ref(subject, target_id, &conflict.relation),
            format!(
                "relation {relation} conflicts with canon {}",
                conflict.relation
            ),
        );
    }
    finding(claim, "unverified", vec![], "no supporting canon link")
}

fn finding(
    claim: &LoreCanonClaim,
    status: &str,
    evidence_refs: Vec<String>,
    reason: impl Into<String>,
) -> LoreCanonClaimFinding {
    LoreCanonClaimFinding {
        claim_id: claim.claim_id.clone(),
        status: status.to_string(),
        subject_entry_id: claim.subject_entry_id.clone(),
        evidence_refs,
        reason: reason.into(),
    }
}

fn relations_conflict(existing: &str, proposed: &str) -> bool {
    matches!(
        (existing, proposed),
        ("serves", "opposes")
            | ("opposes", "serves")
            | ("protects", "destroys")
            | ("destroys", "protects")
            | ("allied_with", "enemy_of")
            | ("enemy_of", "allied_with")
            | ("attuned_to", "repelled_by")
            | ("repelled_by", "attuned_to")
    ) || existing
        .strip_prefix("not_")
        .is_some_and(|rest| rest == proposed)
        || proposed
            .strip_prefix("not_")
            .is_some_and(|rest| rest == existing)
}

fn link_ref(entry: &LoreEntry, target_id: &str, relation: &str) -> Vec<String> {
    vec![format!(
        "lore://entry/{}/link/{target_id}/{relation}",
        entry.entry_id
    )]
}
