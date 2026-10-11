use std::{env, path::PathBuf};

use litd_editor::{
    asset_products::{
        AssetProductKind, MODEL_PRODUCT_ID_PROPERTY, MODEL_SOURCE_PATH_PROPERTY,
        MODEL_SOURCE_SHA256_PROPERTY, product_from_vault_record,
    },
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, EditorTerrainStamp},
    map_build::{StaticShadowBakeSettings, calculate_shadows_and_save_map, export_script},
    world_validation::WorldValidationUseCase,
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};

use super::*;

impl EditorShellRuntime {
    pub(crate) fn show_map_build_menu(&mut self, ui: &mut egui::Ui, width: f32) {
        egui::ComboBox::from_id_salt("editor_map_build")
            .width(width)
            .selected_text("Build")
            .show_ui(ui, |ui| {
                if ui.button("Validate World").clicked()
                    && let Err(err) = self.validate_active_world_command()
                {
                    self.record_map_build_error(&err);
                    tracing::error!(error = %err, "world validation command failed");
                }
                if ui.button("Export Script").clicked()
                    && let Err(err) = self.export_active_map_script()
                {
                    self.record_map_build_error(&err);
                    tracing::error!(error = %err, "map script export failed");
                }
                if ui.button("Bake Shadows").clicked()
                    && let Err(err) = self.calculate_shadows_and_save_active_map()
                {
                    self.record_map_build_error(&err);
                    tracing::error!(error = %err, "static shadow bake/save failed");
                }
                ui.label(self.map_build_status.as_str());
            });
    }

    pub(crate) fn map_build_status(&self) -> &str {
        &self.map_build_status
    }

    fn export_active_map_script(&mut self) -> Result<(), EngineError> {
        let document = self.active_map_build_document()?;
        self.require_world_validation(&document, WorldValidationUseCase::Publish)?;
        let path = map_build_dir().join("active_map.lua");
        let export = export_script(&document, &path)?;
        self.map_build_operation_count = self.map_build_operation_count.saturating_add(1);
        self.map_build_status = format!(
            "script:{}:{}",
            export.summary.document_fingerprint,
            export.script_sha256.chars().take(12).collect::<String>()
        );
        Ok(())
    }

    fn calculate_shadows_and_save_active_map(&mut self) -> Result<(), EngineError> {
        let document = self.active_map_build_document()?;
        self.require_world_validation(&document, WorldValidationUseCase::Publish)?;
        let root = map_build_dir();
        let receipt = calculate_shadows_and_save_map(
            &document,
            &root.join("active_static_shadows.ron"),
            &root.join("active_saved_map.ron"),
            StaticShadowBakeSettings::default(),
        )?;
        self.map_build_operation_count = self.map_build_operation_count.saturating_add(1);
        self.map_build_status = format!(
            "shadows:{}:{}",
            receipt.saved_map.document_fingerprint,
            receipt
                .shadow_receipt
                .bake
                .cells_sha256
                .chars()
                .take(12)
                .collect::<String>()
        );
        Ok(())
    }

    pub(super) fn active_map_build_document(&self) -> Result<EditorDocument, EngineError> {
        let selected =
            self.selected_asset_entry()
                .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                    field: "editor.map_build.shell.selected_asset".to_string(),
                    reason: "asset vault is not loaded; map build requires a real selected asset"
                        .to_string(),
                })?;
        let catalog = AssetVaultCatalog::load_default_cached()?;
        let record = catalog
            .records()
            .iter()
            .find(|record| record.relative_path == selected.relative_path)
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.map_build.shell.selected_asset".to_string(),
                reason: format!(
                    "selected asset {} is absent from the real asset vault",
                    selected.relative_path
                ),
            })?;
        let product = product_from_vault_record(record)?;
        if product.product_kind != AssetProductKind::Mesh {
            return Err(EngineError::UiRenderInvalidConfig {
                field: "editor.map_build.shell.selected_asset.kind".to_string(),
                reason: format!(
                    "selected asset {} is {:?}; map build doodad source requires Mesh",
                    selected.relative_path, product.product_kind
                ),
            });
        }
        let mut document = EditorDocument::new(
            "calyx://world/editor-shell-active-map",
            "editor-shell-active-map-rev-1",
        )?;
        document
            .asset_products
            .insert(product.product_id.clone(), product.clone());
        document.upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))?;
        document.upsert_object(
            EditorObject::new(
                "selected_doodad",
                "Doodad",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(12.0, 0.0, -18.0)),
            )
            .with_property("static_shadow", "true")
            .with_property("shadow_footprint_tiles", "3x2")
            .with_property("shadow_height_tiles", "4")
            .with_property(MODEL_PRODUCT_ID_PROPERTY, product.product_id.clone())
            .with_property(MODEL_SOURCE_SHA256_PROPERTY, product.source_sha256.clone())
            .with_property(
                MODEL_SOURCE_PATH_PROPERTY,
                product.source_relative_path.clone(),
            ),
        )?;
        document.upsert_object(
            EditorObject::new(
                "validation_player_unit",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
            )
            .with_property("faction", "Dawnward")
            .with_property(MODEL_PRODUCT_ID_PROPERTY, product.product_id.clone())
            .with_property(MODEL_SOURCE_SHA256_PROPERTY, product.source_sha256.clone())
            .with_property(
                MODEL_SOURCE_PATH_PROPERTY,
                product.source_relative_path.clone(),
            ),
        )?;
        document.upsert_terrain_stamp(EditorTerrainStamp::new(
            "terrain_ramp",
            "Ramp",
            Transform::from_translation(Vec3::new(-8.0, 0.0, 6.0)),
            5,
            3,
            2.25,
        ))?;
        document.validate()?;
        Ok(document)
    }

    fn record_map_build_error(&mut self, err: &EngineError) {
        self.map_build_status = format!("error:{}:{}", err.code(), err);
    }
}

fn map_build_dir() -> PathBuf {
    env::var("LITD_EDITOR_MAP_BUILD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/editor-map-build"))
}
