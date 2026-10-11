use super::*;
use litd_engine::render::editor_world::EditorViewportRect;

impl EditorShellRuntime {
    #[cfg(test)]
    pub fn evidence(&self) -> EditorShellRuntimeEvidence {
        self.evidence_for_render_target(
            self.plan.settings.viewport_width.round().max(1.0) as u32,
            self.plan.settings.viewport_height.round().max(1.0) as u32,
            1.0,
        )
        .expect("shell evidence for an existing validated plan must stay valid")
    }

    pub fn evidence_for_render_target(
        &self,
        physical_width: u32,
        physical_height: u32,
        pixels_per_point: f32,
    ) -> Result<EditorShellRuntimeEvidence, EngineError> {
        validate_pixels_per_point(pixels_per_point)?;
        let minimap = self.panel_rect(DockPanelKind::Minimap);
        let status = self.panel_rect(DockPanelKind::StatusStrip);
        let viewport_capture =
            self.viewport_rect_for_surface(physical_width, physical_height, pixels_per_point)?;
        let camera = self.plan.viewport.camera;
        let viewport_aspect =
            (self.plan.viewport.rect_px.size().x / self.plan.viewport.rect_px.size().y).max(0.1);
        let selected_asset = self.selected_asset_entry();
        let missing_asset = if self.asset_vault_sources.starts_with("not_loaded_for_") {
            self.asset_vault_sources.clone()
        } else {
            "not_loaded_for_objects_mode".to_string()
        };
        let status_summary = self.status_summary();
        let ai_coauthor = self.ai_coauthor_evidence();
        let validation = self.world_validation_summary();
        let world_creation = self.world_creation_evidence();
        let object_editor_validation = self
            .object_editor_document
            .objects
            .get(&self.object_editor_selected_object)
            .map(litd_editor::object_editor::validate_unit_object_for_lowering);
        let object_manager_index =
            litd_editor::object_manager::build_object_manager_index(&self.object_manager_project)?;
        let object_manager_selected_reference_count = litd_editor::object_manager::find_references(
            &object_manager_index,
            &self.object_manager_selected_id,
        )
        .len();
        Ok(EditorShellRuntimeEvidence {
            frontend_kind: "egui_wgpu_operational_editor",
            layout_units: "egui_points",
            logical_surface_size: format!(
                "{:.1}x{:.1}",
                self.plan.settings.viewport_width, self.plan.settings.viewport_height
            ),
            physical_surface_size: format!("{physical_width}x{physical_height}"),
            pixels_per_point,
            active_mode: self.plan.active_mode,
            mode_tabs: self.plan.mode_tab_summary(),
            active_inspector_title: self.plan.active_inspector.title.clone(),
            active_inspector_tool_id: self.plan.active_inspector.tool_id.clone(),
            dock_panel_count: self.plan.dock_panels.len(),
            dock_panels: self.plan.dock_panel_summary(),
            dock_panels_physical: self.physical_dock_panel_summary(pixels_per_point)?,
            viewport_rect: rect_summary(self.plan.viewport.rect_px),
            viewport_rect_physical: physical_rect_summary(
                self.plan.viewport.rect_px,
                pixels_per_point,
            )?,
            viewport_capture_region: format!(
                "{},{},{}x{}",
                viewport_capture.x,
                viewport_capture.y,
                viewport_capture.width,
                viewport_capture.height
            ),
            minimap_rect: minimap.map(rect_summary).unwrap_or_default(),
            minimap_rect_physical: minimap
                .map(|rect| physical_rect_summary(rect, pixels_per_point))
                .transpose()?
                .unwrap_or_default(),
            status_rect: status.map(rect_summary).unwrap_or_default(),
            status_rect_physical: status
                .map(|rect| physical_rect_summary(rect, pixels_per_point))
                .transpose()?
                .unwrap_or_default(),
            status_health: status_summary.health.to_string(),
            status_message: status_summary.message,
            status_error_count: status_summary.error_count,
            camera: format!(
                "target={:.1},{:.1},{:.1};distance={:.1};yaw={:.1};pitch={:.1}",
                camera.target_world.x,
                camera.target_world.y,
                camera.target_world.z,
                camera.distance,
                camera.yaw_degrees,
                camera.pitch_degrees,
            ),
            play_state: self.play_state_label().to_string(),
            play_camera_rig: main_editor_play::play_camera_rig_label(self.play_camera_rig())
                .to_string(),
            play_session: self.play_session_summary(),
            saved_edit_snapshot: self.saved_edit_snapshot_summary(),
            play_transition_count: self.play_transition_count,
            play_rig_switch_count: self.play_rig_switch_count,
            selected_asset_index: if selected_asset.is_some() {
                self.selected_asset_index()
            } else {
                0
            },
            asset_drop_preview: self.asset_drop_preview.clone(),
            asset_drop_count: self.asset_drop_count,
            last_asset_drop: self.last_asset_drop.clone(),
            last_asset_drop_error: self.last_asset_drop_error.clone(),
            asset_thumbnail_count: self.asset_entries.len(),
            asset_thumbnail_warm_target_count:
                main_editor_content_browser::CONTENT_BROWSER_THUMBNAIL_WARM_TARGET,
            asset_thumbnail_loaded_count: self.thumbnail_cache.loaded_count(),
            asset_thumbnail_error_count: self.thumbnail_cache.error_count(),
            selected_asset_thumbnail: selected_asset
                .map(|asset| asset.thumbnail.relative_path().to_string())
                .unwrap_or_else(|| missing_asset.clone()),
            reference_images: self.plan.reference_image_list(),
            mode_switch_count: self.mode_switch_count,
            inspector_docked: self.inspector_docked(),
            inspector_popout_count: self.inspector_popout_count,
            inspector_reattach_count: self.inspector_reattach_count,
            operational_control_count: self.operational_control_count(),
            moderation_last_action: self.moderation_last_action.clone(),
            moderation_report_count: self.moderation_report_count,
            moderation_state_path: self.moderation_state_path.clone(),
            moderation_state_sha256: self.moderation_state_sha256.clone(),
            object_editor_object_count: self.object_editor_document.objects.len(),
            object_editor_selected_object: self.object_editor_selected_object.clone(),
            object_editor_lower_unit_valid: object_editor_validation
                .map(|validation| validation.valid)
                .unwrap_or(false),
            object_editor_last_action: self.object_editor_last_action.clone(),
            object_manager_entry_count: object_manager_index.entries.len(),
            object_manager_counts: object_manager_counts(&object_manager_index.counts),
            object_manager_selected_id: self.object_manager_selected_id.clone(),
            object_manager_selected_reference_count,
            object_manager_orphan_count: object_manager_index.orphan_ids.len(),
            object_manager_fingerprint: object_manager_index.fingerprint,
            world_scene_primitives: self.world_scene_primitives,
            world_scene_degenerate_triangle_count: self.world_scene_degenerate_triangle_count,
            foliage: self.foliage.clone(),
            foliage_brush_action_count: self.foliage_brush_action_count(),
            foliage_brush_last_action: self.foliage_brush_last_action().to_string(),
            world_layer_state: self.world_layers.summary(),
            world_terrain_visible: self.world_layers.terrain,
            world_foliage_visible: self.world_layers.foliage,
            world_doodads_visible: self.world_layers.doodads,
            world_units_visible: self.world_layers.units,
            world_water_visible: self.world_layers.water,
            world_blight_visible: self.world_layers.blight,
            world_shadows_visible: self.world_layers.shadows,
            world_lighting_visible: self.world_layers.lighting,
            world_weather_visible: self.world_layers.weather,
            world_sky_visible: self.world_layers.sky,
            world_fog_effects_visible: self.world_layers.fog_effects,
            world_grid_visible: self.world_layers.grid,
            world_camera_bounds_visible: self.world_layers.camera_bounds,
            world_pathing_ground_visible: self.world_layers.pathing_ground,
            world_pathing_naval_visible: self.world_layers.pathing_naval,
            world_regions_visible: self.world_layers.regions,
            world_camera_objects_visible: self.world_layers.camera_objects,
            world_info_display_visible: self.world_layers.info_display,
            world_letterbox_mode: self.world_layers.letterbox_mode,
            world_lock_to_game_camera: self.world_layers.lock_to_game_camera,
            world_scene_reload_count: self.world_scene_reload_count,
            minimap_source_id: self.minimap.source_id().to_string(),
            minimap_source_path: self.minimap.source_path().to_string(),
            minimap_source_kind: self.minimap.source_kind().to_string(),
            minimap_world_bounds: self.minimap.world_bounds_summary(),
            minimap_feature_count: self.minimap.feature_count(),
            minimap_area_count: self.minimap.area_count(),
            minimap_marker_count: self.minimap.marker_count(),
            minimap_ed11_rift_beacon_marker_count: self
                .minimap
                .marker_count_by_kind("ed11_blue_rift_beacon")
                + self.minimap.marker_count_by_kind("ed11_lava_rift_beacon"),
            minimap_ed11_depth_fog_card_marker_count: self
                .minimap
                .marker_count_by_kind("ed11_depth_fog_card"),
            minimap_ed11_selection_halo_marker_count: self
                .minimap
                .marker_count_by_kind("ed11_selection_halo"),
            minimap_ed11_foliage_patch_marker_count: self
                .minimap
                .marker_count_by_kind("ed11_foliage_patch"),
            minimap_ed11_branch_fissure_area_count: self
                .minimap
                .area_count_by_kind("ed11_blue_branch_fissure")
                + self.minimap.area_count_by_kind("ed11_lava_branch_fissure"),
            minimap_ring_count: self.minimap.ring_count(),
            minimap_layer_state: self.minimap_layers.summary(),
            minimap_visible_area_count: if self.minimap_layers.areas {
                self.minimap.area_count()
            } else {
                0
            },
            minimap_visible_marker_count: if self.minimap_layers.markers {
                self.minimap.marker_count()
            } else {
                0
            },
            minimap_visible_ring_count: if self.minimap_layers.rings {
                self.minimap.ring_count()
            } else {
                0
            },
            minimap_navigation_count: self.minimap_navigation_count,
            last_minimap_navigation: self
                .last_minimap_navigation_xz
                .map(|xz| format!("{:.1},{:.1}", xz[0], xz[1]))
                .unwrap_or_else(|| "none".to_string()),
            minimap_viewport_world_rect: self.minimap.viewport_world_rect_summary(
                [camera.target_world.x, camera.target_world.z],
                camera.distance,
                viewport_aspect,
            )?,
            selected_asset_label: selected_asset
                .map(|asset| asset.label.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_source: selected_asset
                .map(|asset| asset.source.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_id: selected_asset
                .map(|asset| asset.asset_id.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_type: selected_asset
                .map(|asset| asset.asset_type.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_title: selected_asset
                .map(|asset| asset.title.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_license: selected_asset
                .map(|asset| asset.license.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_license_url: selected_asset
                .map(|asset| asset.license_url.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_source_url: selected_asset
                .map(|asset| asset.source_url.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_relative_path: selected_asset
                .map(|asset| asset.relative_path.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_bytes_on_disk: selected_asset
                .map(|asset| asset.bytes_on_disk)
                .unwrap_or(0),
            selected_asset_sha256: selected_asset
                .map(|asset| asset.sha256.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            selected_asset_status: selected_asset
                .map(|asset| asset.status.clone())
                .unwrap_or_else(|| missing_asset.clone()),
            asset_vault_record_count: self.asset_vault_record_count,
            asset_vault_usable_record_count: self.asset_vault_usable_record_count,
            asset_vault_sources: self.asset_vault_sources.clone(),
            asset_vault_record_types: self.asset_vault_record_types.clone(),
            asset_vault_asset_types: self.asset_vault_asset_types.clone(),
            asset_vault_licenses: self.asset_vault_licenses.clone(),
            asset_vault_semantic_role_counts: self.asset_vault_semantic_role_counts.clone(),
            asset_vault_semantic_profile_count: self.asset_vault_semantic_profile_count,
            asset_vault_semantic_classified_profile_count: self
                .asset_vault_semantic_classified_profile_count,
            asset_vault_semantic_unclassified_profile_count: self
                .asset_vault_semantic_unclassified_profile_count,
            asset_vault_semantic_tag_count: self.asset_vault_semantic_tag_count,
            asset_vault_download_bytes_on_disk: self.asset_vault_download_bytes_on_disk,
            asset_vault_extraction_bytes_on_disk: self.asset_vault_extraction_bytes_on_disk,
            asset_vault_issue1242_p0_status: self.asset_vault_issue1242_p0_status,
            asset_vault_manifests: self.asset_vault_manifests.clone(),
            state_path: self.shell_state.path_display(),
            state_loaded_from_disk: self.shell_state.loaded_from_disk(),
            state_save_count: self.shell_state.save_count(),
            map_build_status: self.map_build_status().to_string(),
            map_build_operation_count: self.map_build_operation_count,
            world_creation_status: world_creation.status,
            world_creation_create_count: world_creation.create_count,
            world_creation_document_id: world_creation.document_id,
            world_creation_summary: world_creation.summary,
            world_creation_terrain_cell_count: world_creation.terrain_cell_count,
            world_creation_default_tile_count: world_creation.default_tile_count,
            world_creation_store: world_creation.store_summary,
            world_creation_aster: world_creation.aster_summary,
            world_creation_last_error: world_creation.last_error,
            world_validation_use_case: format!("{:?}", validation.use_case),
            world_validation_status: if validation.blocking {
                "blocking".to_string()
            } else {
                "ok".to_string()
            },
            world_validation_issue_count: validation.issue_count,
            world_validation_error_count: validation.error_count,
            world_validation_warning_count: validation.warning_count,
            world_validation_run_count: self.world_validation_run_count,
            world_validation_summary: validation.summary.clone(),
            ai_coauthor_state: ai_coauthor.state_summary,
            ai_coauthor_store: ai_coauthor.store_summary,
            ai_coauthor_aster: ai_coauthor.aster_summary,
            ai_coauthor_last_error: ai_coauthor.last_error,
            editor_intelligence: self.editor_intelligence.evidence(),
            vfx: self.vfx_editor.evidence(),
        })
    }

    pub fn viewport_rect_for_surface(
        &self,
        surface_width: u32,
        surface_height: u32,
        pixels_per_point: f32,
    ) -> Result<EditorViewportRect, EngineError> {
        validate_pixels_per_point(pixels_per_point)?;
        let rect = scale_rect(self.plan.viewport.rect_px, pixels_per_point)?;
        EditorViewportRect::from_float_bounds(
            rect.min.x,
            rect.min.y,
            rect.max.x,
            rect.max.y,
            surface_width,
            surface_height,
        )
    }

    fn physical_dock_panel_summary(&self, pixels_per_point: f32) -> Result<String, EngineError> {
        validate_pixels_per_point(pixels_per_point)?;
        self.plan
            .dock_panels
            .iter()
            .map(|panel| {
                Ok(format!(
                    "{:?}:{}",
                    panel.kind,
                    physical_rect_summary(panel.rect_px, pixels_per_point)?
                ))
            })
            .collect::<Result<Vec<_>, EngineError>>()
            .map(|panels| panels.join("|"))
    }
}

fn object_manager_counts(counts: &std::collections::BTreeMap<String, usize>) -> String {
    ["unit", "ability", "triggers", "regions", "assets", "total"]
        .into_iter()
        .map(|key| format!("{key}:{}", counts.get(key).copied().unwrap_or(0)))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn logical_surface_size_from_physical(
    physical_width: u32,
    physical_height: u32,
    pixels_per_point: f32,
) -> Result<(f32, f32), EngineError> {
    validate_pixels_per_point(pixels_per_point)?;
    if physical_width == 0 || physical_height == 0 {
        return invalid_shell_runtime(
            "surface_size",
            "physical surface width and height must be non-zero",
        );
    }
    Ok((
        physical_width as f32 / pixels_per_point,
        physical_height as f32 / pixels_per_point,
    ))
}

pub fn validate_pixels_per_point(pixels_per_point: f32) -> Result<(), EngineError> {
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return invalid_shell_runtime(
            "pixels_per_point",
            "egui pixels_per_point must be finite and positive",
        );
    }
    Ok(())
}

fn physical_rect_summary(rect: UiRect, pixels_per_point: f32) -> Result<String, EngineError> {
    Ok(rect_summary(scale_rect(rect, pixels_per_point)?))
}

fn scale_rect(rect: UiRect, scale: f32) -> Result<UiRect, EngineError> {
    validate_pixels_per_point(scale)?;
    UiRect::new(rect.min * scale, rect.max * scale)
}
