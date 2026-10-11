use calyx_core::{DecayFunction, SequenceDirection, TemporalPolicy};
use litd_engine::EngineError;

use super::{
    TEMPORAL_CONTENT_SCHEMA, TemporalContentItem, TemporalContentQuery, TemporalContentReport,
    TemporalContentScore, TemporalLensEvidence,
};

const SECS_PER_HOUR: i64 = 3_600;
const SECS_PER_DAY: i64 = 86_400;
const UNIX_EPOCH_DOW_MONDAY_ZERO: i64 = 3;

pub(super) fn validate_query(query: &TemporalContentQuery) -> Result<(), EngineError> {
    if query.query_id.trim().is_empty()
        || query.query_id.contains(['\n', '\r', '\t'])
        || query.requested_by.trim().is_empty()
        || query.requested_by.contains(['\n', '\r', '\t'])
        || i64::from(query.tz_offset_secs).abs() > 86_400
    {
        return invalid_temporal_content(
            "query",
            "query id, requester, and timezone offset must be valid",
        );
    }
    validate_policy(&query.policy)
}

pub(super) fn validate_item(item: &TemporalContentItem) -> Result<(), EngineError> {
    if item.item_id.trim().is_empty()
        || item.item_id.contains(['\n', '\r', '\t'])
        || item.title.trim().is_empty()
        || item.base_score_milli > 1000
        || item.sequence_total == 0
        || item.sequence_position > item.sequence_total
        || item.provenance_refs.is_empty()
        || item
            .provenance_refs
            .iter()
            .any(|value| value.trim().is_empty() || value.contains(['\n', '\r', '\t']))
    {
        return invalid_temporal_content(
            "item",
            "item id, title, base score, sequence, and provenance must be valid",
        );
    }
    Ok(())
}

pub(super) fn validate_report(report: &TemporalContentReport) -> Result<(), EngineError> {
    if report.schema != TEMPORAL_CONTENT_SCHEMA
        || report.query_id.trim().is_empty()
        || report.source_of_truth.trim().is_empty()
        || report.requested_by.trim().is_empty()
        || report.item_count == 0
        || report.item_count != report.scores.len()
    {
        return invalid_temporal_content("report", "report fields and counts must be valid");
    }
    validate_policy(&report.policy)?;
    for score in &report.scores {
        if score.item_id.trim().is_empty()
            || score.title.trim().is_empty()
            || score.base_score_milli > 1000
            || score.final_score_milli > 1100
            || score.sequence_total == 0
            || score.sequence_position > score.sequence_total
            || score.provenance_refs.is_empty()
            || !unit_score(score.lens.e2_recency)
            || !unit_score(score.lens.e3_periodic_hour)
            || !unit_score(score.lens.e3_periodic_day)
            || !unit_score(score.lens.e3_periodic)
            || !unit_score(score.lens.e4_sequence)
            || !unit_score(score.lens.fused_temporal)
            || !score
                .lens
                .e4_positional
                .iter()
                .all(|value| value.is_finite() && (-1.0..=1.0).contains(value))
            || !score.lens.temporal_multiplier.is_finite()
            || score.lens.temporal_multiplier < 0.0
        {
            return invalid_temporal_content(
                "report.score",
                "score evidence must be finite and valid",
            );
        }
        if !score.temporal_active && score.temporal_inactive_reason.is_none() {
            return invalid_temporal_content(
                "report.inactive_reason",
                "inactive temporal rows need a reason",
            );
        }
        if score.temporal_active && score.temporal_inactive_reason.is_some() {
            return invalid_temporal_content(
                "report.inactive_reason",
                "active temporal rows must not carry an inactive reason",
            );
        }
        if score.base_score_milli == 0 && score.final_score_milli != 0 {
            return invalid_temporal_content(
                "report.never_dominant",
                "temporal boost must not create score from zero primary score",
            );
        }
    }
    Ok(())
}

pub(super) fn score_item(
    item: &TemporalContentItem,
    query: &TemporalContentQuery,
) -> Result<TemporalContentScore, EngineError> {
    let (e2, e3_hour, e3_day) = match item.event_time_secs {
        Some(event_time) => {
            let (hour, day) = e3_periodic_scores(event_time, query);
            (
                score_e2_recency(event_time, query.query_time_secs, &query.policy.decay),
                hour,
                day,
            )
        }
        None => (0.0, 0.0, 0.0),
    };
    let e3 = (e3_hour + e3_day) / 2.0;
    let e4_sequence = score_e4_sequence(
        item.sequence_position,
        item.sequence_total,
        query.policy.sequence.direction,
    );
    let e4_positional = e4_positional(item.sequence_position, item.sequence_total);
    let fused = fuse_temporal(e2, e3, e4_sequence, &query.policy);
    let multiplier = fused * query.policy.boost.post_retrieval_alpha;
    let temporal_active = query.policy.enabled && item.base_score_milli > 0 && fused > 0.0;
    let temporal_inactive_reason = if temporal_active {
        None
    } else if !query.policy.enabled {
        Some("policy_disabled".to_string())
    } else if item.base_score_milli == 0 {
        Some("zero_primary_score".to_string())
    } else {
        Some("no_temporal_signal".to_string())
    };
    let final_score_milli = if item.base_score_milli == 0 || !query.policy.enabled {
        item.base_score_milli as u32
    } else {
        ((item.base_score_milli as f32) * (1.0 + multiplier)).round() as u32
    };
    Ok(TemporalContentScore {
        item_id: item.item_id.clone(),
        title: item.title.clone(),
        base_score_milli: item.base_score_milli,
        final_score_milli,
        event_time_secs: item.event_time_secs,
        sequence_position: item.sequence_position,
        sequence_total: item.sequence_total,
        temporal_active,
        temporal_inactive_reason,
        lens: TemporalLensEvidence {
            e2_recency: e2,
            e3_periodic_hour: e3_hour,
            e3_periodic_day: e3_day,
            e3_periodic: e3,
            e4_sequence,
            e4_positional,
            fused_temporal: fused,
            post_retrieval_alpha: query.policy.boost.post_retrieval_alpha,
            temporal_multiplier: multiplier,
        },
        provenance_refs: item.provenance_refs.clone(),
    })
}

fn score_e2_recency(event_time_secs: i64, query_time_secs: i64, decay: &DecayFunction) -> f32 {
    let age_secs = query_time_secs.saturating_sub(event_time_secs).max(0) as f32;
    match *decay {
        DecayFunction::Linear { max_age_secs } => {
            if max_age_secs == 0 {
                return if age_secs == 0.0 { 1.0 } else { 0.0 };
            }
            (1.0 - age_secs / max_age_secs as f32).clamp(0.0, 1.0)
        }
        DecayFunction::Exponential { half_life_secs } => {
            if half_life_secs == 0 {
                return if age_secs == 0.0 { 1.0 } else { 0.0 };
            }
            (-age_secs * std::f32::consts::LN_2 / half_life_secs as f32)
                .exp()
                .clamp(0.0, 1.0)
        }
        DecayFunction::Step => {
            if age_secs < SECS_PER_HOUR as f32 {
                0.8
            } else if age_secs < SECS_PER_DAY as f32 {
                0.5
            } else {
                0.1
            }
        }
    }
}

fn e3_periodic_scores(event_time_secs: i64, query: &TemporalContentQuery) -> (f32, f32) {
    let query_bucket = time_bucket(query.query_time_secs, query.tz_offset_secs);
    let event_bucket = time_bucket(event_time_secs, query.tz_offset_secs);
    let target_hour = query.policy.periodic.target_hour.or(query
        .policy
        .periodic
        .use_now
        .then_some(query_bucket.0));
    let target_day = query.policy.periodic.target_day_of_week.or(query
        .policy
        .periodic
        .use_now
        .then_some(query_bucket.1));
    (
        target_hour.map_or(1.0, |target| {
            circular_score(event_bucket.0, target, 24, 12.0)
        }),
        target_day.map_or(1.0, |target| circular_score(event_bucket.1, target, 7, 3.5)),
    )
}

fn score_e4_sequence(position: u64, total: u64, direction: SequenceDirection) -> f32 {
    if total <= 1 {
        return 1.0;
    }
    if position >= total {
        return match direction {
            SequenceDirection::Forward => 0.0,
            SequenceDirection::Backward => 1.0,
        };
    }
    let ratio = (position as f32 / total as f32).clamp(0.0, 1.0);
    match direction {
        SequenceDirection::Forward => 1.0 - ratio,
        SequenceDirection::Backward => ratio,
    }
}

fn e4_positional(position: u64, total: u64) -> [f32; 4] {
    let denominator = total.max(1) as f32;
    let pos_ratio = (position as f32 / denominator).clamp(0.0, 1.0);
    let bwd_ratio = 1.0 - pos_ratio;
    [
        (pos_ratio * std::f32::consts::PI).sin(),
        (pos_ratio * std::f32::consts::PI).cos(),
        (bwd_ratio * std::f32::consts::PI).sin(),
        (bwd_ratio * std::f32::consts::PI).cos(),
    ]
}

fn fuse_temporal(e2: f32, e3: f32, e4: f32, policy: &TemporalPolicy) -> f32 {
    if !policy.enabled {
        return 0.0;
    }
    (policy.fusion_weights.recency * e2
        + policy.fusion_weights.periodic * e3
        + policy.fusion_weights.sequence * e4)
        .clamp(0.0, 1.0)
}

fn circular_score(value: u8, target: u8, span: i16, max_distance: f32) -> f32 {
    let diff = (i16::from(value) - i16::from(target)).abs();
    let dist = diff.min(span - diff) as f32;
    (1.0 - dist / max_distance).clamp(0.0, 1.0)
}

fn time_bucket(time_secs: i64, tz_offset_secs: i32) -> (u8, u8) {
    let local_secs = time_secs.saturating_add(i64::from(tz_offset_secs));
    let hour = (local_secs.rem_euclid(SECS_PER_DAY) / SECS_PER_HOUR) as u8;
    let local_day = local_secs.div_euclid(SECS_PER_DAY);
    let dow = (local_day + UNIX_EPOCH_DOW_MONDAY_ZERO).rem_euclid(7) as u8;
    (hour, dow)
}

fn validate_policy(policy: &TemporalPolicy) -> Result<(), EngineError> {
    policy
        .validate()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.temporal_content.policy".to_string(),
            reason: format!("{}: {}", error.code, error.message),
        })
}

fn unit_score(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

pub(super) fn invalid_temporal_content<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.temporal_content.{}", field.into()),
        reason: reason.into(),
    })
}
