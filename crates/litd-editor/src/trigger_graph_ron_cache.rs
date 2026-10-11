//! Generation+selection-keyed cache of an ECA trigger graph's stable-RON byte count (#1461
//! item 3).
//!
//! The Trigger Editor panel is immediate-mode and rendered a `stable graph bytes=N` label by
//! RON-serializing the WHOLE selected graph EVERY frame (`trigger_graph_stable_ron`). That byte
//! count only changes when the graph does, so it is memoized here: keyed by
//! `(generation, selected_index)` and recomputed only when the owner bumps the generation on a
//! trigger mutation or the user selects a different graph. A never-mutated project therefore
//! serializes each graph at most once.

use crate::mcp_trigger_tools::EcaGraph;
use crate::trigger_editor::trigger_graph_stable_ron;

/// Cache of the last computed `(generation, selected_index) -> stable-RON byte length`. `Clone`
/// so it can live in the cloneable editor runtime. The `Err` variant carries the stringified
/// serialization error for inline display.
#[derive(Debug, Default, Clone)]
pub struct TriggerGraphRonBytesCache {
    cached: Option<((u64, usize), Result<usize, String>)>,
}

impl TriggerGraphRonBytesCache {
    /// Stable-RON byte length of `graph` for `(generation, selected_index)`, serializing only
    /// when that key differs from the cached one. When the key is unchanged the stored value is
    /// returned without re-serializing — no allocation, no RON pass.
    pub fn bytes(
        &mut self,
        generation: u64,
        selected_index: usize,
        graph: &EcaGraph,
    ) -> Result<usize, String> {
        let key = (generation, selected_index);
        if let Some((cached_key, result)) = &self.cached
            && *cached_key == key
        {
            return result.clone();
        }
        let result = trigger_graph_stable_ron(graph)
            .map(|ron| ron.len())
            .map_err(|error| error.to_string());
        self.cached = Some((key, result.clone()));
        result
    }
}

#[cfg(test)]
#[path = "trigger_graph_ron_cache_tests.rs"]
mod tests;
