//! Core app runtime: module lifecycle, fixed/variable timestep, and frame pacing.

use std::time::{Duration, Instant};

use crate::{
    EngineError,
    determinism_boundary::{HotPathPhase, enter_hot_path},
};

const SIXTY_HZ_FRAME: Duration = Duration::from_nanos(16_666_667);

#[derive(Debug, Clone, Copy)]
pub struct AppConfig {
    pub fixed_delta: Duration,
    pub target_frame_time: Duration,
    pub max_fixed_steps_per_frame: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            fixed_delta: SIXTY_HZ_FRAME,
            target_frame_time: SIXTY_HZ_FRAME,
            max_fixed_steps_per_frame: 5,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameTick {
    pub frame_index: u64,
    pub variable_delta: Duration,
    pub fixed_delta: Duration,
    pub fixed_steps: u32,
    pub dropped_fixed_steps: u32,
    pub accumulated_time: Duration,
    pub target_frame_time: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct AppFrameStats {
    pub frame_index: u64,
    pub variable_delta_ns: u128,
    pub fixed_delta_ns: u128,
    pub fixed_steps: u32,
    pub dropped_fixed_steps: u32,
    pub accumulated_time_ns: u128,
    pub target_frame_ns: u128,
    pub sleep_hint_ns: u128,
    pub module_fixed_updates: u32,
    pub module_updates: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct LifecycleReport {
    pub module_count: usize,
    pub started: bool,
    pub clean_shutdown: bool,
    pub module_start_calls: u64,
    pub module_fixed_update_calls: u64,
    pub module_update_calls: u64,
    pub module_shutdown_calls: u64,
}

pub trait AppModule {
    fn name(&self) -> &'static str;

    fn on_start(&mut self) -> Result<(), EngineError> {
        Ok(())
    }

    fn on_fixed_update(&mut self, _tick: &FrameTick) -> Result<(), EngineError> {
        Ok(())
    }

    fn on_update(&mut self, _tick: &FrameTick) -> Result<(), EngineError> {
        Ok(())
    }

    fn on_shutdown(&mut self) -> Result<(), EngineError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct ModuleRegistry {
    modules: Vec<Box<dyn AppModule>>,
    started: bool,
    clean_shutdown: bool,
    module_start_calls: u64,
    module_fixed_update_calls: u64,
    module_update_calls: u64,
    module_shutdown_calls: u64,
}

impl ModuleRegistry {
    pub fn register<M>(&mut self, module: M) -> Result<(), EngineError>
    where
        M: AppModule + 'static,
    {
        let name = module.name();
        if self.started {
            return Err(EngineError::ModuleRegistrationClosed(name.to_string()));
        }
        if self.modules.iter().any(|existing| existing.name() == name) {
            return Err(EngineError::DuplicateModule(name.to_string()));
        }
        self.modules.push(Box::new(module));
        self.clean_shutdown = false;
        Ok(())
    }

    pub fn start(&mut self) -> Result<LifecycleReport, EngineError> {
        if self.started {
            return Ok(self.report());
        }
        for module in &mut self.modules {
            module.on_start()?;
            self.module_start_calls += 1;
        }
        self.started = true;
        self.clean_shutdown = false;
        Ok(self.report())
    }

    pub fn run_fixed_updates(&mut self, tick: &FrameTick) -> Result<u32, EngineError> {
        let mut calls = 0;
        for _ in 0..tick.fixed_steps {
            for module in &mut self.modules {
                module.on_fixed_update(tick)?;
                calls += 1;
                self.module_fixed_update_calls += 1;
            }
        }
        Ok(calls)
    }

    pub fn run_updates(&mut self, tick: &FrameTick) -> Result<u32, EngineError> {
        let mut calls = 0;
        for module in &mut self.modules {
            module.on_update(tick)?;
            calls += 1;
            self.module_update_calls += 1;
        }
        Ok(calls)
    }

    pub fn shutdown(&mut self) -> Result<LifecycleReport, EngineError> {
        if !self.started {
            return Ok(self.report());
        }
        for module in self.modules.iter_mut().rev() {
            module.on_shutdown()?;
            self.module_shutdown_calls += 1;
        }
        self.started = false;
        self.clean_shutdown = true;
        Ok(self.report())
    }

    pub fn report(&self) -> LifecycleReport {
        LifecycleReport {
            module_count: self.modules.len(),
            started: self.started,
            clean_shutdown: self.clean_shutdown,
            module_start_calls: self.module_start_calls,
            module_fixed_update_calls: self.module_fixed_update_calls,
            module_update_calls: self.module_update_calls,
            module_shutdown_calls: self.module_shutdown_calls,
        }
    }
}

pub struct FrameClock {
    config: AppConfig,
    frame_index: u64,
    last_frame_at: Option<Instant>,
    next_frame_at: Option<Instant>,
    accumulated_time: Duration,
}

impl FrameClock {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            frame_index: 0,
            last_frame_at: None,
            next_frame_at: None,
            accumulated_time: Duration::ZERO,
        }
    }

    pub fn tick(&mut self, now: Instant) -> FrameTick {
        let variable_delta = self
            .last_frame_at
            .map(|last| now.saturating_duration_since(last))
            .unwrap_or(Duration::ZERO);
        self.accumulated_time += variable_delta;

        let mut fixed_steps = 0;
        let mut dropped_fixed_steps = 0;
        while self.accumulated_time >= self.config.fixed_delta {
            self.accumulated_time -= self.config.fixed_delta;
            if fixed_steps < self.config.max_fixed_steps_per_frame {
                fixed_steps += 1;
            } else {
                dropped_fixed_steps += 1;
            }
        }

        self.frame_index += 1;
        self.last_frame_at = Some(now);
        self.next_frame_at = Some(now + self.config.target_frame_time);

        FrameTick {
            frame_index: self.frame_index,
            variable_delta,
            fixed_delta: self.config.fixed_delta,
            fixed_steps,
            dropped_fixed_steps,
            accumulated_time: self.accumulated_time,
            target_frame_time: self.config.target_frame_time,
        }
    }

    pub fn sleep_hint(&self, now: Instant) -> Duration {
        self.next_frame_at
            .map(|next| next.saturating_duration_since(now))
            .unwrap_or(self.config.target_frame_time)
    }
}

pub struct AppRuntime {
    clock: FrameClock,
    modules: ModuleRegistry,
}

impl AppRuntime {
    pub fn new(config: AppConfig) -> Self {
        Self {
            clock: FrameClock::new(config),
            modules: ModuleRegistry::default(),
        }
    }

    pub fn modules_mut(&mut self) -> &mut ModuleRegistry {
        &mut self.modules
    }

    pub fn start(&mut self) -> Result<LifecycleReport, EngineError> {
        self.modules.start()
    }

    pub fn frame(&mut self, now: Instant) -> Result<AppFrameStats, EngineError> {
        let report = self.modules.report();
        if !report.started {
            return Err(EngineError::AppRuntimeNotStarted);
        }

        let _hot_path = enter_hot_path(HotPathPhase::SimTick);
        let tick = self.clock.tick(now);
        let module_fixed_updates = self.modules.run_fixed_updates(&tick)?;
        let module_updates = self.modules.run_updates(&tick)?;
        let sleep_hint = self.clock.sleep_hint(now);

        Ok(AppFrameStats {
            frame_index: tick.frame_index,
            variable_delta_ns: tick.variable_delta.as_nanos(),
            fixed_delta_ns: tick.fixed_delta.as_nanos(),
            fixed_steps: tick.fixed_steps,
            dropped_fixed_steps: tick.dropped_fixed_steps,
            accumulated_time_ns: tick.accumulated_time.as_nanos(),
            target_frame_ns: tick.target_frame_time.as_nanos(),
            sleep_hint_ns: sleep_hint.as_nanos(),
            module_fixed_updates,
            module_updates,
        })
    }

    pub fn sleep_hint(&self, now: Instant) -> Duration {
        self.clock.sleep_hint(now)
    }

    pub fn shutdown(&mut self) -> Result<LifecycleReport, EngineError> {
        self.modules.shutdown()
    }

    pub fn lifecycle_report(&self) -> LifecycleReport {
        self.modules.report()
    }
}

impl Default for AppRuntime {
    fn default() -> Self {
        Self::new(AppConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CountingModule;

    impl AppModule for CountingModule {
        fn name(&self) -> &'static str {
            "counting"
        }
    }

    #[test]
    fn frame_clock_runs_fixed_steps_and_keeps_remainder() {
        let config = AppConfig::default();
        let mut clock = FrameClock::new(config);
        let start = Instant::now();
        let first = clock.tick(start);
        assert_eq!(first.frame_index, 1);
        assert_eq!(first.fixed_steps, 0);

        let second = clock.tick(start + Duration::from_millis(50));
        assert_eq!(second.frame_index, 2);
        assert_eq!(second.fixed_steps, 2);
        assert!(second.accumulated_time < config.fixed_delta);
        assert_eq!(
            clock.sleep_hint(start + Duration::from_millis(50)),
            config.target_frame_time
        );
    }

    #[test]
    fn frame_clock_drops_excess_fixed_steps() {
        let config = AppConfig {
            fixed_delta: Duration::from_millis(10),
            target_frame_time: Duration::from_millis(16),
            max_fixed_steps_per_frame: 3,
        };
        let mut clock = FrameClock::new(config);
        let start = Instant::now();
        clock.tick(start);
        let tick = clock.tick(start + Duration::from_millis(100));
        assert_eq!(tick.fixed_steps, 3);
        assert_eq!(tick.dropped_fixed_steps, 7);
    }

    #[test]
    fn module_registry_runs_lifecycle() {
        let mut runtime = AppRuntime::default();
        runtime.modules_mut().register(CountingModule).unwrap();
        let started = runtime.start().unwrap();
        assert!(started.started);
        assert_eq!(started.module_start_calls, 1);

        let now = Instant::now();
        runtime.frame(now).unwrap();
        let stats = runtime.frame(now + Duration::from_millis(34)).unwrap();
        assert_eq!(stats.module_updates, 1);
        assert_eq!(stats.module_fixed_updates, 2);

        let stopped = runtime.shutdown().unwrap();
        assert!(stopped.clean_shutdown);
        assert_eq!(stopped.module_shutdown_calls, 1);
    }

    #[test]
    fn duplicate_module_names_fail_loud() {
        let mut registry = ModuleRegistry::default();
        registry.register(CountingModule).unwrap();
        match registry.register(CountingModule) {
            Err(EngineError::DuplicateModule(name)) => assert_eq!(name, "counting"),
            other => panic!("expected DuplicateModule, got {other:?}"),
        }
    }

    #[test]
    fn frame_before_start_fails_loud() {
        let mut runtime = AppRuntime::default();
        match runtime.frame(Instant::now()) {
            Err(EngineError::AppRuntimeNotStarted) => {}
            other => panic!("expected AppRuntimeNotStarted, got {other:?}"),
        }
    }
}
