use litd_game::replay_viewer::{ReplayViewerConfig, ReplayViewerError};

fn build_replay_viewer(
    package: &RuntimePackageReadback,
    config: Option<&ReplayViewerConfig>,
) -> Result<Option<ReplayViewer>, EngineError> {
    let Some(config) = config else {
        return Ok(None);
    };
    let manifest = package.runtime.as_ref().ok_or_else(|| {
        runtime_host_error(
            "replay.datasets",
            "package has no single-map runtime datasets for replay playback",
        )
    })?;
    let mut viewer = ReplayViewer::load(manifest, &config.replay_path)
        .map_err(|error| replay_host_error("load", error))?;
    if let Some(start_tick) = config.start_tick {
        viewer
            .seek_to_tick(start_tick)
            .map_err(|error| replay_host_error("seek", error))?;
    }
    viewer.set_speed(config.speed);
    viewer
        .set_perspective(config.perspective)
        .map_err(|error| replay_host_error("perspective", error))?;
    viewer.set_paused(config.paused);
    Ok(Some(viewer))
}

impl RuntimeHostApp {
    /// Apply only replay-context controls, then feed the recorded tick batch for
    /// this host frame. Camera input remains in the separate RTS input state.
    fn advance_replay_frame(&mut self) -> Result<Option<ReplayViewerSnapshot>, EngineError> {
        let (Some(viewer), Some(input)) = (&mut self.replay_viewer, &self.replay_input) else {
            return Ok(None);
        };
        let controls_changed = input
            .apply(viewer)
            .map_err(|error| replay_host_error("controls", error))?;
        let snapshot = viewer
            .advance_host_frame()
            .map_err(|error| replay_host_error("advance", error))?;
        if controls_changed {
            tracing::info!(
                code = "LITD-REPLAY-CONTROL",
                paused = snapshot.paused,
                speed = snapshot.speed.label(),
                perspective = snapshot.perspective,
                state_hash = format_args!("{:016x}", snapshot.state_hash),
                "interactive replay presentation control applied"
            );
        }
        self.last_replay_snapshot = Some(snapshot.clone());
        Ok(Some(snapshot))
    }

    /// Rendering and free-camera motion are presentation-only. Read the replay
    /// source of truth again after rendering and fail if any sim field changed.
    fn verify_replay_presentation_unchanged(
        &self,
        before: &ReplayViewerSnapshot,
    ) -> Result<(), EngineError> {
        let Some(viewer) = &self.replay_viewer else {
            return Ok(());
        };
        let after = viewer
            .snapshot()
            .map_err(|error| replay_host_error("presentation_readback", error))?;
        if (
            after.applied_tick,
            after.order_cursor,
            after.state_hash,
            after.expected_final_hash,
        ) != (
            before.applied_tick,
            before.order_cursor,
            before.state_hash,
            before.expected_final_hash,
        ) {
            return Err(runtime_host_error(
                "replay.presentation_mutated_sim",
                "render/free-camera phase changed replay simulation state",
            ));
        }
        Ok(())
    }

    fn finish_replay_input(&mut self) {
        if let Some(input) = &mut self.replay_input {
            input.finish_frame();
        }
    }

    fn replay_completed_hidden(&self) -> bool {
        !self.window_visible
            && self
                .last_replay_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.complete)
    }

    /// #1894: record just_pressed QWER ability slots for the live match module.
    fn queue_ability_slot_presses(&self) {
        let mut mask = 0u8;
        for (slot, action) in ABILITY_SLOT_ACTIONS.iter().enumerate() {
            if self.input.action_state(action).just_pressed {
                mask |= 1 << slot;
            }
        }
        self.counters.queue_ability_slots(mask);
    }
}

fn replay_host_error(field: &str, error: ReplayViewerError) -> EngineError {
    runtime_host_error(format!("replay.{field}"), error.to_string())
}
