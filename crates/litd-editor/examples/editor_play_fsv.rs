use litd_engine::fsv_artifacts::env_path;
use std::{fs, path::PathBuf};

use litd_editor::{
    asset_products::{
        MODEL_PRODUCT_ID_PROPERTY, MODEL_SOURCE_PATH_PROPERTY, assign_vault_model_to_unit,
    },
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, to_stable_ron},
    play::{PlayCameraRig, PlaySession},
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_EDITOR_PLAY_FSV_STATS",
        "tmp/issue1141-editor-play-fsv.txt",
    );
    let session_path = env_path(
        "LITD_EDITOR_PLAY_FSV_SESSION",
        "tmp/issue1141-editor-play.ron",
    );

    let before = fixture_document("calyx-rev-1", 3.0)?;
    let mut session = PlaySession::start(&before, PlayCameraRig::TopDown)?;
    let mut after = fixture_document("calyx-rev-2", 8.0)?;
    after
        .objects
        .get_mut("unit_knight")
        .unwrap()
        .properties
        .insert("hot_reload".to_string(), "true".to_string());
    let hot_reload = session.hot_reload(&after)?;
    let invalid_play_code = match PlaySession::start(
        &EditorDocument::new("calyx://world/empty", "rev-empty")?,
        PlayCameraRig::TopDown,
    ) {
        Err(err @ EngineError::UiRenderInvalidConfig { .. }) => err.code(),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    };
    let unit = session
        .source_document
        .objects
        .get("unit_knight")
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.play.fsv.unit".to_string(),
            reason: "missing unit_knight after play session build".to_string(),
        })?;
    let unit_model_product_id = unit
        .properties
        .get(MODEL_PRODUCT_ID_PROPERTY)
        .cloned()
        .unwrap_or_default();
    let unit_model_source_path = unit
        .properties
        .get(MODEL_SOURCE_PATH_PROPERTY)
        .cloned()
        .unwrap_or_default();
    let presentation_mesh_id = session.build.presentation.instances[0]
        .render_instance
        .mesh_id
        .clone();
    let assigned_product = session
        .source_document
        .asset_products
        .get(&unit_model_product_id)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.play.fsv.asset_product".to_string(),
            reason: format!("missing assigned product {unit_model_product_id}"),
        })?;
    let dependency_list = assigned_product.dependencies.join("|");

    write_text(&session_path, &to_stable_ron(&session)?)?;
    write_text(
        &stats_path,
        &format!(
            "state={:?}\n\
             generation={}\n\
             camera_rig={:?}\n\
             calyx_document_id={}\n\
             source_revision={}\n\
             sim_seed={}\n\
             spawned_bodies={}\n\
             snapshot_tick={}\n\
             snapshot_bodies={}\n\
             presentation_instances={}\n\
             asset_product_count={}\n\
             unit_model_product_id={}\n\
             unit_model_source_path={}\n\
             presentation_mesh_id={}\n\
             asset_product_dependencies={}\n\
             asset_product_dependency_count={}\n\
             visible_instance_ids={}\n\
             draw_batches={}\n\
             hot_reload_needed={}\n\
             changed_object_ids={}\n\
             old_fingerprint={}\n\
             new_fingerprint={}\n\
             session_file={}\n\
             session_file_bytes={}\n\
             invalid_play_code={}\n\
             window_opened=false\n\
             renderer_touched=false\n",
            session.state,
            session.generation,
            session.build.camera_rig,
            session.build.calyx_document_id,
            session.build.source_revision_id,
            session.build.sim_seed,
            session.build.spawned_bodies.len(),
            session.build.snapshot.tick,
            session.build.snapshot.bodies.len(),
            session.build.presentation.instances.len(),
            session.source_document.asset_products.len(),
            unit_model_product_id,
            unit_model_source_path,
            presentation_mesh_id,
            dependency_list,
            assigned_product.dependencies.len(),
            session
                .build
                .visible_instance_ids
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            session.build.draw_batches,
            hot_reload.needs_reload,
            hot_reload.changed_object_ids.join("|"),
            hot_reload.old_fingerprint,
            hot_reload.new_fingerprint,
            session_path.display(),
            fs::metadata(&session_path)?.len(),
            invalid_play_code,
        ),
    )?;
    println!("wrote editor play FSV stats to {}", stats_path.display());
    Ok(())
}

fn fixture_document(revision: &str, x: f32) -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new("calyx://world/ashen-citadel", revision)?;
    document.upsert_object(
        EditorObject::new(
            "unit_knight",
            "Unit",
            None,
            Transform::from_translation(Vec3::new(x, 0.0, -6.0)),
        )
        .with_property("faction", "Dawnward"),
    )?;
    let record = AssetVaultCatalog::load_default()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path == "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf"
        })
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.play.fsv.asset_record".to_string(),
            reason: "missing Poly Haven Mountainside glTF vault record".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_knight", &record)?;
    Ok(document)
}

fn write_text(path: &PathBuf, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
