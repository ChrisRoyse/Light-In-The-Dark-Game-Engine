use calyx_aster::cf::ColumnFamily;
use calyx_aster::vault::AsterVault;
use calyx_core::{Clock, VaultStore};
use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::support::{STREAM_PANEL_VERSION, calyx_insufficient, hash_hex, hex_bytes, run_calyx};
use super::types::{LiveWorldMetricKind, RawCfRow, RollingValidation, TrendReport};

#[derive(Deserialize, Serialize)]
pub(super) struct ObservationContext {
    pub value: f64,
    pub kind: LiveWorldMetricKind,
}

pub(super) fn stream_input(stream_id: &str, kind: LiveWorldMetricKind) -> Vec<u8> {
    format!("litd-live-world-oracle-v1:{}:{stream_id}", kind.as_str()).into_bytes()
}

pub(super) fn stream_panel_version() -> u32 {
    STREAM_PANEL_VERSION
}

pub(super) fn trend_report(
    series: &calyx_aster::recurrence::RecurrenceSeries,
) -> Result<TrendReport, EngineError> {
    let values = observation_values(series)?;
    let (intercept, slope) = fit_line(&values)?;
    let (last_t_secs, last_value) = *values.last().expect("support checked");
    Ok(TrendReport {
        slope_per_sec: slope,
        intercept,
        last_value,
        last_t_secs,
    })
}

pub(super) fn rolling_validation(
    series: &calyx_aster::recurrence::RecurrenceSeries,
) -> Result<RollingValidation, EngineError> {
    let values = observation_values(series)?;
    let mut errors = Vec::new();
    for end in 4..values.len() {
        let partial = fit_line(&values[..end])?;
        let (t, actual) = values[end];
        let predicted = partial.0 + partial.1 * t as f64;
        errors.push(actual - predicted);
    }
    if errors.is_empty() {
        return Err(EngineError::OracleOperationFailed {
            op: "oracle.validation.rolling_origin".to_string(),
            source: Box::new(calyx_insufficient(
                "rolling validation needs at least five observations",
            )),
        });
    }
    let rmse = (errors.iter().map(|err| err * err).sum::<f64>() / errors.len() as f64).sqrt();
    let max_abs_error = errors.iter().map(|err| err.abs()).fold(0.0, f64::max);
    Ok(RollingValidation {
        origin_count: errors.len(),
        rmse,
        max_abs_error,
    })
}

pub(super) fn raw_cf_range_rows<C: Clock>(
    vault: &AsterVault<C>,
    cf: ColumnFamily,
    range: &calyx_aster::cf::KeyRange,
) -> Result<Vec<RawCfRow>, EngineError> {
    let rows = run_calyx("oracle.cf.scan_range", || {
        vault.scan_cf_range_at(vault.snapshot(), cf, range)
    })?;
    Ok(rows
        .into_iter()
        .map(|(key, value)| RawCfRow {
            key_hex: hex_bytes(&key),
            value_hash: hash_hex(&value),
            bytes: value.len() as u64,
        })
        .collect())
}

fn observation_values(
    series: &calyx_aster::recurrence::RecurrenceSeries,
) -> Result<Vec<(i64, f64)>, EngineError> {
    let mut values = Vec::new();
    for occurrence in &series.occurrences {
        let context: ObservationContext = serde_json::from_slice(&occurrence.context.bytes)
            .map_err(|error| EngineError::OracleOperationFailed {
                op: "oracle.recurrence.context.decode".to_string(),
                source: Box::new(calyx_insufficient(format!(
                    "decode observation context: {error}"
                ))),
            })?;
        values.push((occurrence.t_k.0, context.value));
    }
    values.sort_by_key(|(time, _)| *time);
    Ok(values)
}

fn fit_line(values: &[(i64, f64)]) -> Result<(f64, f64), EngineError> {
    let n = values.len() as f64;
    let mean_t = values.iter().map(|(t, _)| *t as f64).sum::<f64>() / n;
    let mean_y = values.iter().map(|(_, y)| *y).sum::<f64>() / n;
    let denom = values
        .iter()
        .map(|(t, _)| (*t as f64 - mean_t).powi(2))
        .sum::<f64>();
    if denom <= 0.0 {
        return Err(EngineError::OracleOperationFailed {
            op: "oracle.trend.fit".to_string(),
            source: Box::new(calyx_insufficient("trend requires distinct timestamps")),
        });
    }
    let slope = values
        .iter()
        .map(|(t, y)| (*t as f64 - mean_t) * (*y - mean_y))
        .sum::<f64>()
        / denom;
    Ok((mean_y - slope * mean_t, slope))
}
