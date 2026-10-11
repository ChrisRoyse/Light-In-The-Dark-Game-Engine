use litd_engine::{
    EngineError,
    game_runtime::fog::{VisibilityGrid, VisibilityState},
    render::editor_world::{EditorWorldFogCell, EditorWorldFogFrame},
};

use super::GameRuntimeRenderer;
use litd_game::replay_viewer::ReplayViewerSnapshot;

impl GameRuntimeRenderer {
    pub fn set_replay_presentation(
        &mut self,
        snapshot: Option<&ReplayViewerSnapshot>,
        grid: Option<&VisibilityGrid>,
    ) -> Result<(), EngineError> {
        let (snapshot, grid) = match (snapshot, grid) {
            (None, None) => {
                if self.last_replay_fog_key.is_some() {
                    self.world_renderer
                        .clear_fog_presentation(self.context.queue());
                }
                self.replay_fog_cells.clear();
                self.last_replay_fog_key = None;
                return Ok(());
            }
            (Some(snapshot), Some(grid)) => (snapshot, grid),
            _ => {
                return Err(replay_fog_error(
                    "replay_fog.source",
                    "replay snapshot and perspective grid must be present together",
                ));
            }
        };

        let key = (snapshot.state_hash, snapshot.perspective);
        if self.last_replay_fog_key != Some(key) {
            self.replay_fog_cells
                .resize(grid.states().len(), EditorWorldFogCell::Masked);
            map_visibility_cells(grid.states(), &mut self.replay_fog_cells)?;
            self.world_renderer.set_fog_presentation(
                self.context.device(),
                self.context.queue(),
                EditorWorldFogFrame {
                    width: grid.width,
                    height: grid.height,
                    cells: &self.replay_fog_cells,
                },
            )?;
            self.last_replay_fog_key = Some(key);
        }

        let status = if snapshot.paused {
            "PAUSED"
        } else if snapshot.complete {
            "COMPLETE"
        } else {
            "PLAYING"
        };
        self.window.set_title(&format!(
            "Light in the Dark Replay - {} | {} {} | P{} | tick {} | fog {}/{}/{} | {:016x}",
            self.package_id,
            status,
            snapshot.speed.label(),
            snapshot.perspective,
            snapshot.applied_tick,
            snapshot.perspective_fog.0,
            snapshot.perspective_fog.1,
            snapshot.perspective_fog.2,
            snapshot.state_hash,
        ));
        Ok(())
    }
}

fn map_visibility_cells(
    source: &[VisibilityState],
    destination: &mut [EditorWorldFogCell],
) -> Result<(), EngineError> {
    if source.len() != destination.len() {
        return Err(replay_fog_error(
            "replay_fog.mapping",
            format!(
                "source cell count {} does not match destination cell count {}",
                source.len(),
                destination.len()
            ),
        ));
    }
    for (source, destination) in source.iter().zip(destination) {
        *destination = match source {
            VisibilityState::Masked => EditorWorldFogCell::Masked,
            VisibilityState::Explored => EditorWorldFogCell::Explored,
            VisibilityState::Visible => EditorWorldFogCell::Visible,
        };
    }
    Ok(())
}

fn replay_fog_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::EditorWorldRendererInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_mapping_preserves_all_three_states() {
        let source = [
            VisibilityState::Masked,
            VisibilityState::Explored,
            VisibilityState::Visible,
        ];
        let mut destination = [EditorWorldFogCell::Visible; 3];

        map_visibility_cells(&source, &mut destination).expect("map fog");

        assert_eq!(
            destination,
            [
                EditorWorldFogCell::Masked,
                EditorWorldFogCell::Explored,
                EditorWorldFogCell::Visible,
            ]
        );
    }

    #[test]
    fn visibility_mapping_refuses_mismatched_buffers() {
        let source = [VisibilityState::Visible; 2];
        let mut destination = [EditorWorldFogCell::Masked; 1];

        let error = map_visibility_cells(&source, &mut destination).unwrap_err();

        assert!(matches!(
            error,
            EngineError::EditorWorldRendererInvalidConfig { ref field, .. }
                if field == "replay_fog.mapping"
        ));
    }
}
