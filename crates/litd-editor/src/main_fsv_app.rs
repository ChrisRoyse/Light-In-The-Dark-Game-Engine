use litd_engine::app::LifecycleReport;
use tracing::{debug, error};
use winit::event_loop::ActiveEventLoop;

use crate::{
    EditorApp, WINDOW_TITLE,
    main_fsv::{OwnedEditorFsvSnapshot, write_fsv_lifecycle_stats, write_fsv_stats},
    main_lifecycle::EditorExitReason,
};

impl EditorApp {
    // #1485 finding 5b: reads the frame index straight from `self.last_snapshot` (the latest
    // frame, moved in — never cloned). Only the index gates exit, so this cannot miss the
    // target frame; the full snapshot is borrowed for the write AFTER the lifecycle mutations,
    // so the immutable borrow of `last_snapshot` never conflicts with `&mut self`.
    pub(super) fn handle_fsv_exit(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(target_frames) = self.fsv.target_frames else {
            return false;
        };
        let Some(frame_index) = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| snapshot.render_stats.frame_index)
        else {
            return false;
        };
        if frame_index < target_frames {
            return false;
        }

        self.lifecycle
            .record_exit_request(EditorExitReason::FsvTargetReached);
        let lifecycle = self.shutdown_runtime(EditorExitReason::FsvTargetReached);
        if let Some(snapshot) = &self.last_snapshot {
            self.write_fsv_stats(snapshot, &lifecycle, "wrote app/runtime FSV stats");
        }
        event_loop.exit();
        true
    }

    pub(super) fn write_periodic_fsv_stats(&mut self) {
        let Some(interval) = self.fsv.stats_interval_frames else {
            return;
        };
        if self.fsv.stats_path.is_none() {
            return;
        }
        let Some(frame) = self
            .last_snapshot
            .as_ref()
            .map(|snapshot| snapshot.render_stats.frame_index)
        else {
            return;
        };
        if frame < self.fsv.last_periodic_stats_frame.saturating_add(interval) {
            return;
        }
        self.fsv.last_periodic_stats_frame = frame;
        let lifecycle = self.runtime.lifecycle_report();
        if let Some(snapshot) = &self.last_snapshot {
            self.write_fsv_stats(snapshot, &lifecycle, "wrote periodic app/runtime FSV stats");
        }
    }

    pub(super) fn write_latest_fsv_stats(&self, lifecycle: &LifecycleReport) {
        if let Some(snapshot) = &self.last_snapshot {
            self.write_fsv_stats(snapshot, lifecycle, "wrote lifecycle app/runtime FSV stats");
        } else {
            self.write_fsv_lifecycle_stats(lifecycle, "wrote lifecycle-only FSV stats");
        }
    }

    fn write_fsv_lifecycle_stats(
        &self,
        lifecycle: &LifecycleReport,
        success_message: &'static str,
    ) {
        if let Some(path) = &self.fsv.stats_path {
            let editor_lifecycle = self.lifecycle.evidence();
            match write_fsv_lifecycle_stats(
                path,
                WINDOW_TITLE,
                &self.build_identity,
                lifecycle,
                &editor_lifecycle,
            ) {
                Ok(()) => debug!(
                    path = %path.display(),
                    exit_reason = editor_lifecycle.last_exit_reason,
                    success_message
                ),
                Err(err) => {
                    error!(path = %path.display(), error = %err, "failed to write lifecycle-only FSV stats")
                }
            }
        }
    }

    fn write_fsv_stats(
        &self,
        snapshot: &OwnedEditorFsvSnapshot,
        lifecycle: &LifecycleReport,
        success_message: &'static str,
    ) {
        if let Some(path) = &self.fsv.stats_path {
            // #1485 finding 3: rebuild the full world evidence on demand HERE (snapshot cadence),
            // not every frame. The renderer is NOT dropped by shutdown_runtime, so its last-frame
            // evidence matches the frame captured in `snapshot`. If no frame ever rendered (renderer
            // absent), fall back to lifecycle-only stats rather than emit a partial snapshot.
            let Some(world) = self
                .renderer
                .as_ref()
                .and_then(|renderer| renderer.world_frame_evidence())
            else {
                self.write_fsv_lifecycle_stats(lifecycle, success_message);
                return;
            };
            let borrowed = snapshot.borrowed(&world);
            let editor_lifecycle = self.lifecycle.evidence();
            match write_fsv_stats(
                path,
                WINDOW_TITLE,
                &self.build_identity,
                &borrowed,
                lifecycle,
                &editor_lifecycle,
            ) {
                Ok(()) => debug!(
                    path = %path.display(),
                    frame = snapshot.render_stats.frame_index,
                    success_message
                ),
                Err(err) => {
                    error!(path = %path.display(), frame = snapshot.render_stats.frame_index, error = %err, "failed to write app/runtime FSV stats")
                }
            }
        }
    }
}
