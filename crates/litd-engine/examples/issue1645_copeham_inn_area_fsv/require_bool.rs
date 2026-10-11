fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1645.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
