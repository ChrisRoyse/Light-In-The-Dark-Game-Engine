use std::collections::BTreeSet;

use calyx_aster::cf::ColumnFamily;
use calyx_aster::vault::AsterVault;
use calyx_core::{Clock, VaultStore};
use calyx_oracle::{
    ConsequenceTree, HOP_ATTENUATION, MIN_CONFIDENCE_THRESHOLD, is_provisional_ledger_ref,
};

use crate::EngineError;

use super::support::{calyx_failed, hash_hex, hex_bytes};
use super::types::{PreviewRawCfRow, WhatIfTreeDiagnostics};

pub(super) fn tree_diagnostics(tree: &ConsequenceTree) -> WhatIfTreeDiagnostics {
    let mut paths = Vec::new();
    let mut stats = TreeStats::default();
    walk_tree(tree, &mut paths, &mut stats);
    let next_confidence = tree.root.confidence * HOP_ATTENUATION;
    WhatIfTreeDiagnostics {
        nodes: stats.nodes,
        leaves: stats.leaves,
        max_hop: stats.max_hop,
        repeated_action_paths: stats.repeated_action_paths,
        cycle_guard_satisfied: stats.repeated_action_paths == 0,
        low_confidence_pruned: tree.children.is_empty()
            && next_confidence < MIN_CONFIDENCE_THRESHOLD,
        provisional_nodes: stats.provisional_nodes,
        grounded_nodes: stats.grounded_nodes,
        prediction_ledger: tree.root.provenance.clone(),
    }
}

pub(super) fn raw_cf_rows<C: Clock>(
    vault: &AsterVault<C>,
    cf: ColumnFamily,
) -> Result<Vec<PreviewRawCfRow>, EngineError> {
    let rows = vault
        .scan_cf_at(vault.snapshot(), cf)
        .map_err(|error| calyx_failed("oracle.what_if.cf.scan", error))?;
    Ok(rows
        .into_iter()
        .map(|(key, value)| PreviewRawCfRow {
            key_hex: hex_bytes(&key),
            value_hash: hash_hex(&value),
            bytes: value.len() as u64,
        })
        .collect())
}

fn walk_tree(tree: &ConsequenceTree, path: &mut Vec<String>, stats: &mut TreeStats) {
    stats.nodes += 1;
    stats.max_hop = stats.max_hop.max(tree.root.hop);
    if is_provisional_ledger_ref(&tree.root.provenance) {
        stats.provisional_nodes += 1;
    } else {
        stats.grounded_nodes += 1;
    }
    path.push(format!(
        "{}:{}",
        tree.root.domain.as_str(),
        tree.root.action_or_event
    ));
    let unique = path.iter().collect::<BTreeSet<_>>().len();
    if unique < path.len() {
        stats.repeated_action_paths += 1;
    }
    if tree.children.is_empty() {
        stats.leaves += 1;
    } else {
        for child in &tree.children {
            walk_tree(child, path, stats);
        }
    }
    path.pop();
}

#[derive(Default)]
struct TreeStats {
    nodes: u64,
    leaves: u64,
    max_hop: u8,
    repeated_action_paths: u64,
    provisional_nodes: u64,
    grounded_nodes: u64,
}
