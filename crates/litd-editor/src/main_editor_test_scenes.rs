use litd_editor::asset_scene::{EditorAssetSceneReport, EditorWorldMapReport};
use litd_engine::EngineError;

pub(super) fn default_scene_report_for_tests() -> Result<EditorAssetSceneReport, EngineError> {
    static REPORT: std::sync::OnceLock<EditorAssetSceneReport> = std::sync::OnceLock::new();
    if let Some(report) = REPORT.get() {
        return Ok(report.clone());
    }
    let loaded = litd_editor::asset_scene::load_default_editor_world_asset_scene()?;
    let report = loaded.report;
    if REPORT.set(report.clone()).is_err() {
        return Ok(REPORT
            .get()
            .expect("default scene report was initialized by a racing test")
            .clone());
    }
    Ok(report)
}

pub(super) fn empty_minimap_scene_report_for_tests() -> Result<EditorAssetSceneReport, EngineError>
{
    let mut report = default_scene_report_for_tests()?;
    report.world_map = EditorWorldMapReport::new(
        report.world_map.source_kind.clone(),
        report.world_map.bounds_min,
        report.world_map.bounds_max,
        report.world_map.outline.clone(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )?;
    Ok(report)
}
