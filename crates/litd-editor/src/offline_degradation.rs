//! Calyx feature availability gates for local-first editor degradation.

use std::{
    env, fs,
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    time::Duration,
};

use calyx_mcp::JsonRpcResponse;
use litd_engine::{
    EngineError,
    calyx_paths::{CALYX_PATH_MISSING, validate_gameeditor_calyx_home},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::mcp_tools::{call_engine_calyx_mcp_tool, default_engine_calyx_mcp_surface_manifest};

#[path = "offline_degradation_args.rs"]
mod offline_degradation_args;
use offline_degradation_args::*;

pub const OFFLINE_DEGRADATION_SCHEMA: &str = "litd.editor.offline_degradation.v1";
pub const CALYX_ONLINE: &str = "LITD-CALYX-ONLINE";
pub const CALYX_MISSING_HOME: &str = "LITD-CALYX-OFFLINE-MISSING-HOME";
pub const CALYX_HOME_NOT_DIRECTORY: &str = "LITD-CALYX-OFFLINE-HOME-NOT-DIRECTORY";
pub const CALYX_HOME_UNREADABLE: &str = "LITD-CALYX-OFFLINE-HOME-UNREADABLE";
pub const CALYX_ENDPOINT_HARD_DOWN: &str = "LITD-CALYX-OFFLINE-ENDPOINT-HARD-DOWN";
pub const CALYX_ENDPOINT_TIMEOUT: &str = "LITD-CALYX-OFFLINE-ENDPOINT-TIMEOUT";
pub const CALYX_ENDPOINT_REJECTED: &str = "LITD-CALYX-OFFLINE-ENDPOINT-REJECTED";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalyxAvailabilityProbe {
    pub probe_id: String,
    pub calyx_home: Option<PathBuf>,
    pub endpoint: Option<CalyxEndpointProbe>,
    pub timeout_ms: u64,
    pub required_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalyxEndpointProbe {
    pub address: String,
    pub require_initialize: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalyxEndpointReadback {
    pub address: String,
    pub connected: bool,
    pub initialized: bool,
    pub reason_code: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalyxAvailabilityReadback {
    pub schema: String,
    pub probe_id: String,
    pub source_of_truth: String,
    pub available: bool,
    pub reason_code: String,
    pub reason: String,
    pub calyx_home: Option<String>,
    pub home_exists: bool,
    pub home_is_dir: bool,
    pub home_readable: bool,
    pub endpoint: Option<CalyxEndpointReadback>,
    pub tool_count: usize,
    pub required_tools_present: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalyxFeaturePanel {
    pub feature_id: String,
    pub label: String,
    pub enabled: bool,
    pub status: String,
    pub disabled_reason: String,
    pub reason_code: String,
    pub availability: CalyxAvailabilityReadback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxFeatureCallReadback {
    pub feature_id: String,
    pub tool_name: String,
    pub availability: CalyxAvailabilityReadback,
    pub response: JsonRpcResponse,
}

impl CalyxAvailabilityProbe {
    pub fn from_env(probe_id: impl Into<String>) -> Self {
        Self {
            probe_id: probe_id.into(),
            calyx_home: None,
            endpoint: None,
            timeout_ms: 500,
            required_tools: Vec::new(),
        }
    }

    pub fn local_home(probe_id: impl Into<String>, calyx_home: impl Into<PathBuf>) -> Self {
        Self {
            probe_id: probe_id.into(),
            calyx_home: Some(calyx_home.into()),
            endpoint: None,
            timeout_ms: 500,
            required_tools: Vec::new(),
        }
    }
}

impl CalyxFeaturePanel {
    pub fn require_enabled(&self) -> Result<(), EngineError> {
        if self.enabled {
            Ok(())
        } else {
            Err(feature_disabled_error(&self.feature_id, &self.availability))
        }
    }
}

pub fn probe_calyx_availability(
    probe: &CalyxAvailabilityProbe,
) -> Result<CalyxAvailabilityReadback, EngineError> {
    validate_probe(probe)?;
    let manifest = default_engine_calyx_mcp_surface_manifest()?;
    let required = if probe.required_tools.is_empty() {
        manifest
            .tools
            .iter()
            .take(3)
            .map(|tool| tool.tool_name.clone())
            .collect::<Vec<_>>()
    } else {
        probe.required_tools.clone()
    };
    let present = required
        .iter()
        .filter(|name| manifest.tools.iter().any(|tool| &tool.tool_name == *name))
        .cloned()
        .collect::<Vec<_>>();
    if present.len() != required.len() {
        return invalid(
            "tools",
            format!("required Calyx MCP tools missing: required={required:?} present={present:?}"),
        );
    }

    let home = probe
        .calyx_home
        .clone()
        .or_else(|| env::var_os("CALYX_HOME").map(PathBuf::from));
    let Some(home) = home else {
        return Ok(readback(ReadbackArgs {
            probe,
            home: None,
            home_exists: false,
            home_is_dir: false,
            home_readable: false,
            endpoint: None,
            tool_count: manifest.tools.len(),
            required_tools_present: present,
            reason_code: CALYX_MISSING_HOME,
            reason: "CALYX_HOME is not set; Calyx-backed AI/search/dedup features are disabled",
        }));
    };
    let home_exists = home.exists();
    let home_is_dir = home.is_dir();
    let home_readable = home_is_dir && fs::read_dir(&home).is_ok();
    let home = match validate_gameeditor_calyx_home(&home) {
        Ok(resolved) => resolved,
        Err(error) if error.code == CALYX_PATH_MISSING => home,
        Err(error) => {
            return Ok(readback(ReadbackArgs {
                probe,
                home: Some(home),
                home_exists,
                home_is_dir,
                home_readable,
                endpoint: None,
                tool_count: manifest.tools.len(),
                required_tools_present: present,
                reason_code: error.code,
                reason: format!("{error}; Calyx features are disabled"),
            }));
        }
    };
    if !home_exists || !home_is_dir {
        return Ok(readback(ReadbackArgs {
            probe,
            home: Some(home),
            home_exists,
            home_is_dir,
            home_readable,
            endpoint: None,
            tool_count: manifest.tools.len(),
            required_tools_present: present,
            reason_code: CALYX_HOME_NOT_DIRECTORY,
            reason: "configured CALYX_HOME does not exist as a directory; Calyx features are disabled",
        }));
    }
    if !home_readable {
        return Ok(readback(ReadbackArgs {
            probe,
            home: Some(home),
            home_exists,
            home_is_dir,
            home_readable: false,
            endpoint: None,
            tool_count: manifest.tools.len(),
            required_tools_present: present,
            reason_code: CALYX_HOME_UNREADABLE,
            reason: "configured CALYX_HOME cannot be read; Calyx features are disabled",
        }));
    }

    let endpoint = match &probe.endpoint {
        Some(endpoint) => {
            let observed = probe_endpoint(endpoint, probe.timeout_ms)?;
            if !observed.connected || (endpoint.require_initialize && !observed.initialized) {
                return Ok(readback(ReadbackArgs {
                    probe,
                    home: Some(home),
                    home_exists: true,
                    home_is_dir: true,
                    home_readable: true,
                    endpoint: Some(observed.clone()),
                    tool_count: manifest.tools.len(),
                    required_tools_present: present,
                    reason_code: &observed.reason_code,
                    reason: &observed.reason,
                }));
            }
            Some(observed)
        }
        None => None,
    };

    Ok(readback(ReadbackArgs {
        probe,
        home: Some(home),
        home_exists: true,
        home_is_dir: true,
        home_readable: true,
        endpoint,
        tool_count: manifest.tools.len(),
        required_tools_present: present,
        reason_code: CALYX_ONLINE,
        reason: "Calyx local feature gate is online",
    }))
}

pub fn build_calyx_feature_panel(
    feature_id: impl Into<String>,
    label: impl Into<String>,
    probe: &CalyxAvailabilityProbe,
) -> Result<CalyxFeaturePanel, EngineError> {
    let feature_id = feature_id.into();
    let label = label.into();
    validate_single_line("feature_id", &feature_id)?;
    validate_single_line("label", &label)?;
    let availability = probe_calyx_availability(probe)?;
    let enabled = availability.available;
    let status = if enabled {
        format!("online:{}", availability.reason_code)
    } else {
        format!("offline:{}", availability.reason_code)
    };
    let disabled_reason = if enabled {
        String::new()
    } else {
        format!("AI features offline: {}", availability.reason)
    };
    Ok(CalyxFeaturePanel {
        feature_id,
        label,
        enabled,
        status,
        disabled_reason,
        reason_code: availability.reason_code.clone(),
        availability,
    })
}

pub fn call_calyx_mcp_tool_when_available(
    feature_id: impl Into<String>,
    probe: &CalyxAvailabilityProbe,
    tool_name: &str,
    arguments: Value,
) -> Result<CalyxFeatureCallReadback, EngineError> {
    let feature_id = feature_id.into();
    validate_single_line("tool_name", tool_name)?;
    let availability = probe_calyx_availability(probe)?;
    if !availability.available {
        return Err(feature_disabled_error(&feature_id, &availability));
    }
    let response = call_engine_calyx_mcp_tool(tool_name, arguments)?;
    Ok(CalyxFeatureCallReadback {
        feature_id,
        tool_name: tool_name.to_string(),
        availability,
        response,
    })
}

pub fn feature_disabled_error(
    feature_id: &str,
    availability: &CalyxAvailabilityReadback,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.offline_degradation.{feature_id}"),
        reason: format!(
            "AI features offline: {} ({})",
            availability.reason, availability.reason_code
        ),
    }
}

fn probe_endpoint(
    endpoint: &CalyxEndpointProbe,
    timeout_ms: u64,
) -> Result<CalyxEndpointReadback, EngineError> {
    validate_single_line("endpoint.address", &endpoint.address)?;
    let addr: SocketAddr = endpoint
        .address
        .parse()
        .map_err(|error| ui_error("endpoint.address", format!("{error}")))?;
    let timeout = Duration::from_millis(timeout_ms);
    let mut stream = match TcpStream::connect_timeout(&addr, timeout) {
        Ok(stream) => stream,
        Err(error) => {
            let code = if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) {
                CALYX_ENDPOINT_TIMEOUT
            } else {
                CALYX_ENDPOINT_HARD_DOWN
            };
            return Ok(endpoint_readback(endpoint, false, false, code, error));
        }
    };
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| ui_error("endpoint.read_timeout", format!("{error}")))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| ui_error("endpoint.write_timeout", format!("{error}")))?;
    if !endpoint.require_initialize {
        return Ok(endpoint_readback(
            endpoint,
            true,
            true,
            CALYX_ONLINE,
            "TCP connect succeeded",
        ));
    }
    match invoke_initialize(&mut stream) {
        Ok(()) => Ok(endpoint_readback(
            endpoint,
            true,
            true,
            CALYX_ONLINE,
            "length-prefixed MCP initialize succeeded",
        )),
        Err(error) => {
            let text = error.to_string();
            let code = classify_initialize_error(&text);
            Ok(endpoint_readback(endpoint, true, false, code, text))
        }
    }
}

fn classify_initialize_error(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timed out")
        || lower.contains("wouldblock")
        || lower.contains("would block")
        || lower.contains("did not properly respond")
        || lower.contains("could not be completed immediately")
        || lower.contains("10035")
        || lower.contains("10060")
    {
        CALYX_ENDPOINT_TIMEOUT
    } else if lower.contains("failed to fill whole buffer")
        || lower.contains("forcibly closed")
        || lower.contains("connection reset")
        || lower.contains("connection was aborted")
        || lower.contains("broken pipe")
        || lower.contains("early eof")
    {
        CALYX_ENDPOINT_HARD_DOWN
    } else {
        CALYX_ENDPOINT_REJECTED
    }
}

fn invoke_initialize(stream: &mut TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let body = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":701,"method":"initialize"}))?;
    stream.write_all(&(body.len() as u32).to_be_bytes())?;
    stream.write_all(&body)?;
    stream.flush()?;
    let mut len = [0_u8; 4];
    stream.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len);
    if len == 0 || len > 4 * 1024 * 1024 {
        return Err(format!("invalid MCP response frame length {len}").into());
    }
    let mut payload = vec![0; len as usize];
    stream.read_exact(&mut payload)?;
    let value: Value = serde_json::from_slice(&payload)?;
    if value["result"]["serverInfo"]["name"].as_str().is_none() {
        return Err(format!("initialize response missing serverInfo: {value}").into());
    }
    Ok(())
}

fn readback(args: ReadbackArgs<'_, impl Into<String>>) -> CalyxAvailabilityReadback {
    let ReadbackArgs {
        probe,
        home,
        home_exists,
        home_is_dir,
        home_readable,
        endpoint,
        tool_count,
        required_tools_present,
        reason_code,
        reason,
    } = args;
    CalyxAvailabilityReadback {
        schema: OFFLINE_DEGRADATION_SCHEMA.to_string(),
        probe_id: probe.probe_id.clone(),
        source_of_truth:
            "CALYX_HOME directory metadata, Calyx MCP source tool registry, and optional loopback MCP endpoint probe"
                .to_string(),
        available: reason_code == CALYX_ONLINE,
        reason_code: reason_code.to_string(),
        reason: reason.into(),
        calyx_home: home.map(|path| path.display().to_string()),
        home_exists,
        home_is_dir,
        home_readable,
        endpoint,
        tool_count,
        required_tools_present,
    }
}

fn endpoint_readback(
    endpoint: &CalyxEndpointProbe,
    connected: bool,
    initialized: bool,
    reason_code: &str,
    reason: impl ToString,
) -> CalyxEndpointReadback {
    CalyxEndpointReadback {
        address: endpoint.address.clone(),
        connected,
        initialized,
        reason_code: reason_code.to_string(),
        reason: reason.to_string(),
    }
}

fn validate_probe(probe: &CalyxAvailabilityProbe) -> Result<(), EngineError> {
    validate_single_line("probe_id", &probe.probe_id)?;
    if probe.timeout_ms == 0 || probe.timeout_ms > 30_000 {
        return invalid("timeout_ms", "timeout must be in 1..=30000 ms");
    }
    for tool in &probe.required_tools {
        validate_single_line("required_tools", tool)?;
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return invalid(field, "value must be non-empty single-line text");
    }
    Ok(())
}

fn invalid<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(ui_error(field, reason))
}

fn ui_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.offline_degradation.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "offline_degradation_tests.rs"]
mod tests;
