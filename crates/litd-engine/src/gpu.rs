//! GPU adapter/device discovery via wgpu. Fail-loud: if no compatible GPU
//! exists, callers get a structured error rather than a software fallback.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::error::EngineError;
pub use crate::gpu_adapter::{AdapterPreference, AdapterSelection, select_adapter_index};

static NEXT_GPU_CONTEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A human/log-friendly snapshot of a GPU adapter wgpu reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterReport {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub driver_info: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuContextMode {
    Headless,
    SurfaceCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuContextReport {
    pub context_id: u64,
    pub mode: GpuContextMode,
    pub adapter: AdapterReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuDeviceLostReport {
    pub context_id: u64,
    pub reason: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct GpuContext {
    context_id: u64,
    mode: GpuContextMode,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    report: AdapterReport,
    device_lost: Arc<Mutex<Option<GpuDeviceLostReport>>>,
}

fn report_of(info: &wgpu::AdapterInfo) -> AdapterReport {
    AdapterReport {
        name: info.name.clone(),
        backend: format!("{:?}", info.backend),
        device_type: format!("{:?}", info.device_type),
        driver: info.driver.clone(),
        driver_info: info.driver_info.clone(),
    }
}

impl GpuContext {
    pub async fn new_headless() -> Result<Self, EngineError> {
        Self::new_headless_on(wgpu::Backends::PRIMARY).await
    }

    pub async fn new_headless_on(backends: wgpu::Backends) -> Result<Self, EngineError> {
        Self::new_headless_on_with_preference(backends, AdapterPreference::DiscretePreferred).await
    }

    /// Headless context that lets the caller force an adapter class (#1793): e.g.
    /// `ForceIntegrated` exercises the integrated path on a discrete-primary box.
    pub async fn new_headless_with_preference(
        preference: AdapterPreference,
    ) -> Result<Self, EngineError> {
        Self::new_headless_on_with_preference(wgpu::Backends::PRIMARY, preference).await
    }

    pub async fn new_headless_on_with_preference(
        backends: wgpu::Backends,
        preference: AdapterPreference,
    ) -> Result<Self, EngineError> {
        let instance = instance_on(backends);
        if instance.enumerate_adapters(backends).is_empty() {
            return Err(EngineError::NoAdapter(backends));
        }
        Self::request(
            instance,
            backends,
            None,
            GpuContextMode::Headless,
            preference,
        )
        .await
    }

    /// Surface context honoring `LITD_GPU_ADAPTER` (#1793): the editor picks the
    /// discrete GPU by default but an operator can force the integrated adapter.
    pub async fn new_for_surface(
        instance: wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> Result<Self, EngineError> {
        Self::new_for_surface_with_preference(instance, surface, AdapterPreference::from_env()?)
            .await
    }

    pub async fn new_for_surface_with_preference(
        instance: wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        preference: AdapterPreference,
    ) -> Result<Self, EngineError> {
        Self::request(
            instance,
            wgpu::Backends::PRIMARY,
            Some(surface),
            GpuContextMode::SurfaceCompatible,
            preference,
        )
        .await
    }

    pub fn context_id(&self) -> u64 {
        self.context_id
    }

    pub fn mode(&self) -> GpuContextMode {
        self.mode
    }

    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn adapter_report(&self) -> &AdapterReport {
        &self.report
    }

    pub fn report(&self) -> GpuContextReport {
        GpuContextReport {
            context_id: self.context_id,
            mode: self.mode,
            adapter: self.report.clone(),
        }
    }

    pub fn device_lost_report(&self) -> Option<GpuDeviceLostReport> {
        read_device_lost_report(&self.device_lost)
    }

    async fn request(
        instance: wgpu::Instance,
        backends: wgpu::Backends,
        compatible_surface: Option<&wgpu::Surface<'_>>,
        mode: GpuContextMode,
        preference: AdapterPreference,
    ) -> Result<Self, EngineError> {
        // #1793: explicit, logged selection instead of wgpu's opaque power
        // preference. Enumerate, keep only surface-compatible adapters, then rank
        // by the caller's preference and log the winner plus every rejected one.
        let adapters = instance.enumerate_adapters(backends);
        if adapters.is_empty() {
            return Err(EngineError::NoAdapter(backends));
        }
        let eligible: Vec<usize> = adapters
            .iter()
            .enumerate()
            .filter(|(_, adapter)| {
                compatible_surface.is_none_or(|surface| adapter.is_surface_supported(surface))
            })
            .map(|(index, _)| index)
            .collect();
        if eligible.is_empty() {
            return Err(EngineError::NoSurfaceAdapter);
        }
        let infos: Vec<wgpu::AdapterInfo> =
            eligible.iter().map(|&i| adapters[i].get_info()).collect();
        let selection = select_adapter_index(preference, &infos)?;
        let adapter_index = eligible[selection.chosen_index];
        tracing::info!(
            preference = selection.preference,
            chosen = %selection.chosen_label,
            rejected = ?selection.rejected,
            surface_filtered = compatible_surface.is_some(),
            "litd gpu adapter selected (#1793 explicit selection)"
        );
        let adapter = adapters
            .into_iter()
            .nth(adapter_index)
            .expect("adapter_index is a valid enumerated index");
        let report = report_of(&adapter.get_info());
        let context_id = NEXT_GPU_CONTEXT_ID.fetch_add(1, Ordering::Relaxed);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("litd-shared-gpu-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await?;
        let device_lost = Arc::new(Mutex::new(None));
        let callback_state = Arc::clone(&device_lost);
        let callback_adapter = report.name.clone();
        let callback_backend = report.backend.clone();
        device.set_device_lost_callback(move |reason, message| {
            let lost = GpuDeviceLostReport {
                context_id,
                reason: format!("{reason:?}"),
                message: message.clone(),
            };
            if let Ok(mut state) = callback_state.lock() {
                *state = Some(lost);
            }
            tracing::error!(
                context_id,
                adapter = %callback_adapter,
                backend = %callback_backend,
                reason = ?reason,
                message = %message,
                "litd shared gpu context lost"
            );
        });
        tracing::info!(
            context_id,
            adapter = %report.name,
            backend = %report.backend,
            mode = ?mode,
            "litd shared gpu context initialized"
        );
        Ok(Self {
            context_id,
            mode,
            instance,
            adapter,
            device,
            queue,
            report,
            device_lost,
        })
    }
}

/// Default LitD instance policy: keep internal indirect-call validation, but
/// do not request external API validation layers unless WGPU_* env flags opt in.
pub fn engine_instance_descriptor() -> wgpu::InstanceDescriptor {
    wgpu::InstanceDescriptor {
        flags: engine_instance_flags(),
        ..wgpu::InstanceDescriptor::default()
    }
    .with_env()
}

fn engine_instance_flags() -> wgpu::InstanceFlags {
    wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL
}

fn instance() -> wgpu::Instance {
    instance_on(wgpu::Backends::PRIMARY)
}

fn instance_on(backends: wgpu::Backends) -> wgpu::Instance {
    let mut descriptor = engine_instance_descriptor();
    descriptor.backends = backends;
    wgpu::Instance::new(&descriptor)
}

/// Prefer a discrete GPU; tie-break toward Vulkan (our primary Windows backend,
/// per the engine doctrine) over DX12, then anything else.
fn rank(info: &wgpu::AdapterInfo) -> (u8, u8) {
    let device = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 3,
        wgpu::DeviceType::IntegratedGpu => 2,
        wgpu::DeviceType::VirtualGpu => 1,
        _ => 0,
    };
    let backend = match info.backend {
        wgpu::Backend::Vulkan => 2,
        wgpu::Backend::Dx12 => 1,
        _ => 0,
    };
    (device, backend)
}

fn enumerate_on(backends: wgpu::Backends) -> Vec<AdapterReport> {
    instance()
        .enumerate_adapters(backends)
        .iter()
        .map(|a| report_of(&a.get_info()))
        .collect()
}

fn primary_on(backends: wgpu::Backends) -> Result<AdapterReport, EngineError> {
    let adapters = instance().enumerate_adapters(backends);
    adapters
        .iter()
        .max_by_key(|a| rank(&a.get_info()))
        .map(|a| report_of(&a.get_info()))
        .ok_or(EngineError::NoAdapter(backends))
}

/// Every adapter wgpu can see on the primary backends (Vulkan / DX12 / Metal).
pub fn enumerate_adapters() -> Vec<AdapterReport> {
    enumerate_on(wgpu::Backends::PRIMARY)
}

/// Pick the best adapter (discrete GPU, Vulkan-preferred), or fail loud if none exist.
pub fn primary_adapter() -> Result<AdapterReport, EngineError> {
    primary_on(wgpu::Backends::PRIMARY)
}

/// Read the device-lost report, recovering the guarded state even if the mutex
/// was poisoned. #1464: a poisoned mutex means a thread panicked while holding
/// this lock — most likely *around* the device-loss callback — so returning
/// `None` ("no device lost") there masks the loss exactly when it matters.
/// Recover the guarded state instead, and log the poison so the prior panic is
/// observable.
fn read_device_lost_report(
    lock: &Mutex<Option<GpuDeviceLostReport>>,
) -> Option<GpuDeviceLostReport> {
    match lock.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => {
            tracing::error!(
                code = "GPU_DEVICE_LOST_LOCK_POISONED",
                "device-lost mutex was poisoned by a prior panic; recovering the guarded state"
            );
            poisoned.into_inner().clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// EDGE / FAIL-LOUD (#1464): a poisoned device-lost mutex must still surface
    /// a recorded device-loss report, not mask it as "no device lost".
    #[test]
    fn poisoned_device_lost_mutex_still_reports_the_loss() {
        let report = GpuDeviceLostReport {
            context_id: 42,
            reason: "synthetic-device-loss".to_string(),
            message: "gpu removed under test".to_string(),
        };
        let lock = Arc::new(Mutex::new(Some(report.clone())));

        // Poison the mutex by panicking while holding the guard.
        let poisoner = Arc::clone(&lock);
        let joined = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("intentional panic to poison the device-lost mutex");
        })
        .join();
        assert!(joined.is_err(), "the poisoner thread must have panicked");
        assert!(lock.is_poisoned(), "the mutex must be poisoned");

        // Before #1464 this returned None (lock().ok() -> None); now it recovers.
        assert_eq!(read_device_lost_report(&lock), Some(report));
    }

    /// HAPPY PATH: this workstation has a real GPU, so PRIMARY must yield one,
    /// and the chosen primary must be a discrete GPU.
    #[test]
    fn primary_backends_find_a_discrete_gpu() {
        let all = enumerate_on(wgpu::Backends::PRIMARY);
        assert!(
            !all.is_empty(),
            "expected at least one GPU adapter on PRIMARY backends"
        );
        let primary = primary_adapter().expect("a real GPU must be selectable on this machine");
        assert_eq!(
            primary.device_type, "DiscreteGpu",
            "primary should be the discrete GPU, got {primary:?}"
        );
    }

    /// EDGE / FAIL-LOUD: with no backends enabled there are zero adapters, and we
    /// must return the structured NoAdapter error — never a silent fallback.
    #[test]
    fn empty_backends_fail_loud_with_no_adapter() {
        assert!(enumerate_on(wgpu::Backends::empty()).is_empty());
        match primary_on(wgpu::Backends::empty()) {
            Err(EngineError::NoAdapter(b)) => assert_eq!(b, wgpu::Backends::empty()),
            other => panic!("expected NoAdapter error, got {other:?}"),
        }
    }

    #[test]
    fn litd_instance_flags_do_not_request_external_validation_by_default() {
        let flags = engine_instance_flags();
        assert!(!flags.contains(wgpu::InstanceFlags::VALIDATION));
        assert!(flags.contains(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL));
    }

    #[test]
    fn shared_context_empty_backends_fail_loud_with_no_adapter() {
        match pollster::block_on(GpuContext::new_headless_on(wgpu::Backends::empty())) {
            Err(EngineError::NoAdapter(backends)) => assert_eq!(backends, wgpu::Backends::empty()),
            other => panic!("expected NoAdapter, got {other:?}"),
        }
    }
}
