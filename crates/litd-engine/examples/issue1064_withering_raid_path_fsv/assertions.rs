fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        if edge["state_unchanged"] != true || edge["error_code"] != "LITD-RPG-STATS-INVALID-CONFIG"
        {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1064.edge.{name}"),
                reason: format!("unexpected edge {edge}"),
            });
        }
    }
    Ok(())
}
