fn known_area_ids() -> [&'static str; 5] {
    [
        VORALIAN_AREA_ID,
        GALADON_AREA_ID,
        LOCH_GRYNMEAR,
        BRAMBLEFIELD,
        BLACKWATER_APPROACH,
    ]
}

#[allow(clippy::too_many_arguments)]
fn hidden(
    connection_id: &str,
    from_room_id: &str,
    to_room_id: Option<&str>,
    to_area_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: to_area_id.to_string(),
        to_room_id: to_room_id.map(str::to_string),
        direction,
        travel_kind,
        discovery,
        locked_by_ref: None,
    }
}

fn area_spec_hash(spec: &AreaSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize area spec edge: {error}"),
        })
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1637.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1637.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
