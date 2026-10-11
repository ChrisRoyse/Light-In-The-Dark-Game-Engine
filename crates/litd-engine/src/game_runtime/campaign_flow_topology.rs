//! #1852 campaign unlock_after topology: cycle detection + fail-loud flow errors.

use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;
use crate::game_package::RuntimeCampaignManifest;

/// Detect a cycle in the `unlock_after` dependency graph via Kahn's algorithm. A campaign
/// whose chapters cyclically depend on each other has chapters that can never unlock (their
/// prerequisites are never all satisfiable) -- a hard authoring error, rejected fail-loud.
pub(crate) fn detect_unlock_cycle(manifest: &RuntimeCampaignManifest) -> Result<(), EngineError> {
    let chapter_ids: BTreeSet<&str> = manifest
        .chapters
        .iter()
        .map(|chapter| chapter.chapter_id.as_str())
        .collect();
    let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
    let mut successors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for chapter in &manifest.chapters {
        let predecessors: Vec<&str> = chapter
            .unlock_after
            .iter()
            .map(String::as_str)
            .filter(|predecessor| chapter_ids.contains(predecessor))
            .collect();
        in_degree.insert(chapter.chapter_id.as_str(), predecessors.len());
        for predecessor in predecessors {
            successors
                .entry(predecessor)
                .or_default()
                .push(chapter.chapter_id.as_str());
        }
    }
    let mut ready: Vec<&str> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut processed = 0_usize;
    while let Some(node) = ready.pop() {
        processed += 1;
        if let Some(children) = successors.get(node) {
            for child in children {
                if let Some(degree) = in_degree.get_mut(child) {
                    *degree -= 1;
                    if *degree == 0 {
                        ready.push(child);
                    }
                }
            }
        }
    }
    if processed != manifest.chapters.len() {
        return Err(flow_error(
            "topology.unlock_cycle",
            "campaign unlock_after graph has a cycle: one or more chapters can never unlock",
        ));
    }
    Ok(())
}

pub(crate) fn flow_error(field: &str, reason: impl Into<String>) -> EngineError {
    let field = format!("litd_game.campaign_flow.{field}");
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-GAME-CAMPAIGN-FLOW",
        "litd-game campaign flow failed"
    );
    EngineError::GamePackageInvalidConfig { field, reason }
}
