//! Per-frame CPU/GPU profiling capture with source-of-truth readback.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::EngineError;

pub const PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSystem {
    Sim,
    Nav,
    Ai,
    Render,
    Stream,
}

impl ProfileSystem {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sim => "sim",
            Self::Nav => "nav",
            Self::Ai => "ai",
            Self::Render => "render",
            Self::Stream => "stream",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuSpanId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuSpanProfile {
    pub id: u64,
    pub system: ProfileSystem,
    pub name: String,
    pub start_ns: u64,
    pub end_ns: u64,
    pub duration_ns: u64,
    pub depth: u32,
    pub parent_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuPassProfile {
    pub name: String,
    pub begin_tick: u64,
    pub end_tick: u64,
    pub timestamp_period_nanos_x1000: u64,
    pub duration_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameProfile {
    pub schema_version: u32,
    pub enabled: bool,
    pub frame_index: u64,
    pub frame_start_ns: u64,
    pub frame_end_ns: u64,
    pub frame_time_ns: u64,
    pub cpu_spans: Vec<CpuSpanProfile>,
    pub gpu_passes: Vec<GpuPassProfile>,
    pub cpu_top_level_ns: u64,
    pub cpu_span_total_ns: u64,
    pub gpu_total_ns: u64,
    pub systems_present: Vec<ProfileSystem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileJsonReadback {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub profile: FrameProfile,
}

#[derive(Debug, Clone)]
struct ActiveCpuSpan {
    id: u64,
    system: ProfileSystem,
    name: String,
    start_ns: u64,
    depth: u32,
    parent_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct FrameProfiler {
    enabled: bool,
    started: bool,
    frame_index: u64,
    frame_start_ns: u64,
    active: Vec<ActiveCpuSpan>,
    completed: Vec<CpuSpanProfile>,
    gpu_passes: Vec<GpuPassProfile>,
    next_span_id: u64,
}

impl FrameProfiler {
    pub fn enabled() -> Self {
        Self::new(true)
    }

    pub fn disabled() -> Self {
        Self::new(false)
    }

    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            started: false,
            frame_index: 0,
            frame_start_ns: 0,
            active: Vec::new(),
            completed: Vec::new(),
            gpu_passes: Vec::new(),
            next_span_id: 0,
        }
    }

    pub fn begin_frame_at(
        &mut self,
        frame_index: u64,
        frame_start_ns: u64,
    ) -> Result<(), EngineError> {
        if self.started && !self.active.is_empty() {
            return Err(invalid_operation(
                "begin_frame_at",
                "cannot begin a new frame while CPU spans are still active",
            ));
        }
        self.started = true;
        self.frame_index = frame_index;
        self.frame_start_ns = frame_start_ns;
        self.active.clear();
        self.completed.clear();
        self.gpu_passes.clear();
        self.next_span_id = 0;
        Ok(())
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    pub fn active_span_count(&self) -> usize {
        self.active.len()
    }

    pub fn completed_span_count(&self) -> usize {
        self.completed.len()
    }

    pub fn gpu_pass_count(&self) -> usize {
        self.gpu_passes.len()
    }

    pub fn begin_cpu_span_at(
        &mut self,
        system: ProfileSystem,
        name: impl Into<String>,
        start_ns: u64,
    ) -> Result<Option<CpuSpanId>, EngineError> {
        if !self.enabled {
            return Ok(None);
        }
        self.require_started("begin_cpu_span_at")?;
        let name = clean_name("cpu_span.name", name.into())?;
        if start_ns < self.frame_start_ns {
            return Err(invalid_config(
                "cpu_span.start_ns",
                "span start must be at or after frame_start_ns",
            ));
        }
        let id = self.next_span_id;
        self.next_span_id = self
            .next_span_id
            .checked_add(1)
            .ok_or_else(|| invalid_config("cpu_span.id", "span id overflowed"))?;
        let parent_id = self.active.last().map(|span| span.id);
        let depth = u32::try_from(self.active.len())
            .map_err(|_| invalid_config("cpu_span.depth", "span nesting depth exceeded u32"))?;
        self.active.push(ActiveCpuSpan {
            id,
            system,
            name,
            start_ns,
            depth,
            parent_id,
        });
        Ok(Some(CpuSpanId(id)))
    }

    pub fn end_cpu_span_at(&mut self, span_id: CpuSpanId, end_ns: u64) -> Result<(), EngineError> {
        if !self.enabled {
            return Ok(());
        }
        self.require_started("end_cpu_span_at")?;
        let active = self.active.pop().ok_or_else(|| {
            invalid_operation("end_cpu_span_at", "no active CPU span exists to end")
        })?;
        if active.id != span_id.0 {
            return Err(invalid_operation(
                "end_cpu_span_at",
                "CPU spans must end in strict LIFO order",
            ));
        }
        let duration_ns = end_ns.checked_sub(active.start_ns).ok_or_else(|| {
            invalid_config("cpu_span.end_ns", "span end must be at or after span start")
        })?;
        self.completed.push(CpuSpanProfile {
            id: active.id,
            system: active.system,
            name: active.name,
            start_ns: active.start_ns,
            end_ns,
            duration_ns,
            depth: active.depth,
            parent_id: active.parent_id,
        });
        Ok(())
    }

    pub fn record_gpu_pass_ticks(
        &mut self,
        name: impl Into<String>,
        begin_tick: u64,
        end_tick: u64,
        timestamp_period_nanos_x1000: u64,
    ) -> Result<(), EngineError> {
        if !self.enabled {
            return Ok(());
        }
        self.require_started("record_gpu_pass_ticks")?;
        let name = clean_name("gpu_pass.name", name.into())?;
        if timestamp_period_nanos_x1000 == 0 {
            return Err(invalid_config(
                "gpu_pass.timestamp_period_nanos_x1000",
                "GPU timestamp period must be positive",
            ));
        }
        let tick_delta = end_tick.checked_sub(begin_tick).ok_or_else(|| {
            invalid_config("gpu_pass.end_tick", "GPU end tick must be >= begin tick")
        })?;
        let duration_ps = u128::from(tick_delta)
            .checked_mul(u128::from(timestamp_period_nanos_x1000))
            .ok_or_else(|| invalid_config("gpu_pass.duration_ns", "GPU duration overflowed"))?;
        let duration_ns = u64::try_from(duration_ps.div_ceil(1000))
            .map_err(|_| invalid_config("gpu_pass.duration_ns", "GPU duration exceeded u64"))?;
        self.gpu_passes.push(GpuPassProfile {
            name,
            begin_tick,
            end_tick,
            timestamp_period_nanos_x1000,
            duration_ns,
        });
        Ok(())
    }

    pub fn finish_frame_at(&mut self, frame_end_ns: u64) -> Result<FrameProfile, EngineError> {
        self.require_started("finish_frame_at")?;
        if !self.active.is_empty() {
            return Err(invalid_operation(
                "finish_frame_at",
                "cannot finish a frame while CPU spans are still active",
            ));
        }
        let frame_time_ns = frame_end_ns
            .checked_sub(self.frame_start_ns)
            .ok_or_else(|| {
                invalid_config("frame_end_ns", "frame end must be at or after frame start")
            })?;
        let mut cpu_spans = self.completed.clone();
        cpu_spans.sort_by_key(|span| span.id);
        for span in &cpu_spans {
            if span.end_ns > frame_end_ns {
                return Err(invalid_config(
                    "cpu_span.end_ns",
                    "span end must be inside the frame",
                ));
            }
        }
        let mut systems = BTreeSet::new();
        let mut cpu_top_level_ns = 0u64;
        let mut cpu_span_total_ns = 0u64;
        for span in &cpu_spans {
            systems.insert(span.system);
            cpu_span_total_ns =
                checked_add("cpu_span_total_ns", cpu_span_total_ns, span.duration_ns)?;
            if span.depth == 0 {
                cpu_top_level_ns =
                    checked_add("cpu_top_level_ns", cpu_top_level_ns, span.duration_ns)?;
            }
        }
        let mut gpu_total_ns = 0u64;
        for pass in &self.gpu_passes {
            gpu_total_ns = checked_add("gpu_total_ns", gpu_total_ns, pass.duration_ns)?;
        }
        let profile = FrameProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            enabled: self.enabled,
            frame_index: self.frame_index,
            frame_start_ns: self.frame_start_ns,
            frame_end_ns,
            frame_time_ns,
            cpu_spans: if self.enabled { cpu_spans } else { Vec::new() },
            gpu_passes: if self.enabled {
                self.gpu_passes.clone()
            } else {
                Vec::new()
            },
            cpu_top_level_ns: if self.enabled { cpu_top_level_ns } else { 0 },
            cpu_span_total_ns: if self.enabled { cpu_span_total_ns } else { 0 },
            gpu_total_ns: if self.enabled { gpu_total_ns } else { 0 },
            systems_present: if self.enabled {
                systems.into_iter().collect()
            } else {
                Vec::new()
            },
        };
        self.started = false;
        Ok(profile)
    }

    fn require_started(&self, op: &'static str) -> Result<(), EngineError> {
        if self.started {
            Ok(())
        } else {
            Err(invalid_operation(op, "begin_frame_at must be called first"))
        }
    }
}

pub fn write_profile_json(
    path: impl AsRef<Path>,
    profile: &FrameProfile,
) -> Result<ProfileJsonReadback, EngineError> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(persistence_failed(
            "profile.write",
            "path must be non-empty",
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            persistence_failed(
                "profile.write.parent",
                format!("failed to create {}: {error}", parent.display()),
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(profile).map_err(|error| {
        persistence_failed(
            "profile.encode",
            format!("failed to encode profile JSON: {error}"),
        )
    })?;
    fs::write(path, &bytes).map_err(|error| {
        persistence_failed(
            "profile.write",
            format!("failed to write {}: {error}", path.display()),
        )
    })?;
    let readback = fs::read(path).map_err(|error| {
        persistence_failed(
            "profile.readback",
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    let decoded: FrameProfile = serde_json::from_slice(&readback).map_err(|error| {
        persistence_failed(
            "profile.decode",
            format!("failed to decode {}: {error}", path.display()),
        )
    })?;
    if &decoded != profile {
        return Err(EngineError::ProfileReadbackMismatch {
            field: "profile".to_string(),
            expected: "in-memory frame profile".to_string(),
            actual: "decoded JSON profile differed".to_string(),
        });
    }
    Ok(ProfileJsonReadback {
        path: path.to_path_buf(),
        bytes: u64::try_from(readback.len())
            .map_err(|_| persistence_failed("profile.readback", "readback length exceeded u64"))?,
        sha256: sha256_hex(&readback),
        profile: decoded,
    })
}

fn clean_name(field: &'static str, value: String) -> Result<String, EngineError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(invalid_config(field, "name must be non-empty"));
    }
    Ok(trimmed.to_string())
}

fn checked_add(field: &'static str, lhs: u64, rhs: u64) -> Result<u64, EngineError> {
    lhs.checked_add(rhs)
        .ok_or_else(|| invalid_config(field, "profile duration total overflowed"))
}

fn invalid_config(field: &'static str, reason: impl Into<String>) -> EngineError {
    EngineError::ProfileInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    }
}

fn invalid_operation(op: &'static str, reason: impl Into<String>) -> EngineError {
    EngineError::ProfileInvalidOperation {
        op: op.to_string(),
        reason: reason.into(),
    }
}

fn persistence_failed(op: &'static str, reason: impl Into<String>) -> EngineError {
    EngineError::ProfilePersistenceFailed {
        op: op.to_string(),
        reason: reason.into(),
    }
}
