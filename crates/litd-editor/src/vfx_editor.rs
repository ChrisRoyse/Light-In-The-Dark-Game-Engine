use litd_hash::sha256_hex;
use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::{Aabb, Vec2, Vec3},
    render::{
        batching::{BatchingSettings, Frustum, ScreenRect},
        vfx::{
            VfxCurve, VfxCurvePoint, VfxEffectAsset, VfxEffectStoreWriteback, VfxEmitter,
            VfxGpuParticleSettings, VfxRenderWorkloadSettings, VfxSceneEffectInstance,
            VfxSpawnModule, VfxUpdateModule, build_vfx_render_workload, render_vfx_particles_gpu,
            validate_effect, write_effect_to_content_store,
        },
    },
};
use serde::{Deserialize, Serialize};

#[path = "vfx_editor_defaults.rs"]
mod vfx_editor_defaults;

use vfx_editor_defaults::{default_fireball_effect, default_size_curve};

#[derive(Debug, Clone)]
pub struct EditorVfxRuntime {
    store_root: PathBuf,
    effect: VfxEffectAsset,
    selected_emitter: usize,
    revision: u32,
    save_count: u32,
    preview_refresh_count: u32,
    last_error: String,
    last_save: Option<EditorVfxSaveReadback>,
    last_preview: Option<EditorVfxPreviewReadback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorVfxSaveReadback {
    pub object_path: String,
    pub digest: String,
    pub sha256: String,
    pub bytes: u64,
    pub deduplicated: bool,
    pub effect_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorVfxPreviewReadback {
    pub image_path: String,
    pub image_sha256: String,
    pub image_bytes: u64,
    pub adapter_name: String,
    pub backend: String,
    pub drawn_particles: u32,
    pub sprite_particles: u32,
    pub mesh_particles: u32,
    pub draw_calls: u32,
    pub lit_pixel_ratio: f32,
    pub hot_pixel_ratio: f32,
    pub source_effect_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorVfxEvidence {
    pub effect_id: String,
    pub display_name: String,
    pub selected_emitter: String,
    pub emitter_count: usize,
    pub total_max_particles: u32,
    pub spawn_module_count: usize,
    pub initialize_module_count: usize,
    pub update_module_count: usize,
    pub output_module_count: usize,
    pub draw_group_count: usize,
    pub revision: u32,
    pub save_count: u32,
    pub preview_refresh_count: u32,
    pub store_root: String,
    pub last_object_path: String,
    pub last_digest: String,
    pub last_object_sha256: String,
    pub last_preview_path: String,
    pub last_preview_sha256: String,
    pub last_preview_lit_ratio: f32,
    pub last_preview_draw_calls: u32,
    pub last_error: String,
    /// Whether the current in-editor effect passes `validate_effect`. When
    /// false, the module counts are 0 because the effect is *invalid*, not
    /// because it is empty (#1464) — read `validation_error` for the cause.
    pub effect_valid: bool,
    /// The `validate_effect` error message when `effect_valid` is false, else
    /// empty.
    pub validation_error: String,
}

impl EditorVfxRuntime {
    pub fn from_env() -> Result<Self, EngineError> {
        let root = env::var("LITD_EDITOR_VFX_STORE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("target/editor/vfx-content-store"));
        Self::with_store_root(root)
    }

    pub fn with_store_root(root: impl Into<PathBuf>) -> Result<Self, EngineError> {
        let effect = default_fireball_effect()?;
        validate_effect(&effect)?;
        Ok(Self {
            store_root: root.into(),
            effect,
            selected_emitter: 0,
            revision: 0,
            save_count: 0,
            preview_refresh_count: 0,
            last_error: String::new(),
            last_save: None,
            last_preview: None,
        })
    }

    pub fn effect(&self) -> &VfxEffectAsset {
        &self.effect
    }

    pub fn evidence(&self) -> EditorVfxEvidence {
        // #1464: surface an invalid effect loudly instead of rendering its
        // module counts as zeros indistinguishable from an empty effect.
        let (report, effect_valid, validation_error) = match validate_effect(&self.effect) {
            Ok(report) => (Some(report), true, String::new()),
            Err(error) => {
                tracing::error!(
                    code = "EDITOR_VFX_EVIDENCE_INVALID_EFFECT",
                    effect_id = %self.effect.effect_id,
                    %error,
                    "VFX evidence panel is reporting an invalid effect"
                );
                (None, false, error.to_string())
            }
        };
        let selected = self
            .effect
            .emitters
            .get(self.selected_emitter)
            .map(|emitter| emitter.display_name.clone())
            .unwrap_or_else(|| "none".to_string());
        EditorVfxEvidence {
            effect_id: self.effect.effect_id.clone(),
            display_name: self.effect.display_name.clone(),
            selected_emitter: selected,
            emitter_count: self.effect.emitters.len(),
            total_max_particles: report.as_ref().map(|r| r.total_max_particles).unwrap_or(0),
            spawn_module_count: report.as_ref().map(|r| r.spawn_module_count).unwrap_or(0),
            initialize_module_count: report
                .as_ref()
                .map(|r| r.initialize_module_count)
                .unwrap_or(0),
            update_module_count: report.as_ref().map(|r| r.update_module_count).unwrap_or(0),
            output_module_count: report.as_ref().map(|r| r.output_module_count).unwrap_or(0),
            draw_group_count: report.as_ref().map(|r| r.draw_group_count).unwrap_or(0),
            revision: self.revision,
            save_count: self.save_count,
            preview_refresh_count: self.preview_refresh_count,
            store_root: self.store_root.display().to_string(),
            last_object_path: self
                .last_save
                .as_ref()
                .map(|save| save.object_path.clone())
                .unwrap_or_else(|| "none".to_string()),
            last_digest: self
                .last_save
                .as_ref()
                .map(|save| save.digest.clone())
                .unwrap_or_else(|| "none".to_string()),
            last_object_sha256: self
                .last_save
                .as_ref()
                .map(|save| save.sha256.clone())
                .unwrap_or_else(|| "none".to_string()),
            last_preview_path: self
                .last_preview
                .as_ref()
                .map(|preview| preview.image_path.clone())
                .unwrap_or_else(|| "none".to_string()),
            last_preview_sha256: self
                .last_preview
                .as_ref()
                .map(|preview| preview.image_sha256.clone())
                .unwrap_or_else(|| "none".to_string()),
            last_preview_lit_ratio: self
                .last_preview
                .as_ref()
                .map(|preview| preview.lit_pixel_ratio)
                .unwrap_or(0.0),
            last_preview_draw_calls: self
                .last_preview
                .as_ref()
                .map(|preview| preview.draw_calls)
                .unwrap_or(0),
            last_error: self.last_error.clone(),
            effect_valid,
            validation_error,
        }
    }

    pub fn set_spawn_rate(&mut self, particles_per_second: f32) -> Result<(), EngineError> {
        if !particles_per_second.is_finite() || particles_per_second <= 0.0 {
            return vfx_editor_error("spawn_rate", "particles per second must be positive");
        }
        let emitter = self.selected_emitter_mut()?;
        if let Some(VfxSpawnModule::ConstantRate {
            particles_per_second: rate,
        }) = emitter.spawn.first_mut()
        {
            *rate = particles_per_second;
            self.mark_edited();
            return Ok(());
        }
        vfx_editor_error(
            "spawn_rate",
            "selected emitter has no constant-rate spawn module",
        )
    }

    pub fn remove_size_over_life(&mut self) -> Result<bool, EngineError> {
        let emitter = self.selected_emitter_mut()?;
        let before = emitter.update.len();
        emitter
            .update
            .retain(|module| !matches!(module, VfxUpdateModule::SizeOverLife { .. }));
        let changed = emitter.update.len() != before;
        if changed {
            validate_effect(&self.effect)?;
            self.mark_edited();
        }
        Ok(changed)
    }

    pub fn restore_size_over_life(&mut self) -> Result<(), EngineError> {
        let emitter = self.selected_emitter_mut()?;
        if !emitter
            .update
            .iter()
            .any(|module| matches!(module, VfxUpdateModule::SizeOverLife { .. }))
        {
            emitter.update.push(VfxUpdateModule::SizeOverLife {
                curve: default_size_curve(),
            });
            self.mark_edited();
        }
        validate_effect(&self.effect).map(|_| ())
    }

    pub fn set_invalid_size_curve_for_fsv(&mut self) -> Result<(), EngineError> {
        let emitter = self.selected_emitter_mut()?;
        let curve = VfxCurve {
            points: vec![
                VfxCurvePoint { t: 0.7, value: 1.0 },
                VfxCurvePoint { t: 0.4, value: 0.2 },
            ],
        };
        if let Some(module) = emitter
            .update
            .iter_mut()
            .find(|module| matches!(module, VfxUpdateModule::SizeOverLife { .. }))
        {
            *module = VfxUpdateModule::SizeOverLife { curve };
        } else {
            emitter.update.push(VfxUpdateModule::SizeOverLife { curve });
        }
        self.mark_edited();
        Ok(())
    }

    pub fn save_effect(&mut self) -> Result<EditorVfxSaveReadback, EngineError> {
        self.capture_error(|this| {
            let writeback = write_effect_to_content_store(&this.store_root, &this.effect)?;
            let readback = save_readback(&writeback);
            this.last_save = Some(readback.clone());
            this.save_count += 1;
            Ok(readback)
        })
    }

    pub fn refresh_preview(&mut self) -> Result<EditorVfxPreviewReadback, EngineError> {
        self.capture_error(|this| {
            validate_effect(&this.effect)?;
            let workload = this.preview_workload()?;
            let frame = pollster::block_on(render_vfx_particles_gpu(
                VfxGpuParticleSettings {
                    width: 960,
                    height: 540,
                    max_particles: 120_000,
                },
                &workload,
            ))?;
            let preview_dir = this.store_root.join("previews");
            fs::create_dir_all(&preview_dir).map_err(|error| {
                vfx_editor_error_value(
                    "preview.dir",
                    format!("failed to create {}: {error}", preview_dir.display()),
                )
            })?;
            let path = preview_dir.join(format!(
                "{}-r{}.bmp",
                safe_file_stem(&this.effect.effect_id),
                this.revision
            ));
            frame.image.write_bmp(&path)?;
            let bytes = fs::read(&path).map_err(|error| {
                vfx_editor_error_value(
                    "preview.readback",
                    format!("failed to read {}: {error}", path.display()),
                )
            })?;
            let readback = EditorVfxPreviewReadback {
                image_path: path.display().to_string(),
                image_sha256: sha256_hex(&bytes),
                image_bytes: bytes.len() as u64,
                adapter_name: frame.stats.adapter_name,
                backend: frame.stats.backend,
                drawn_particles: frame.stats.drawn_particles,
                sprite_particles: frame.stats.sprite_particles,
                mesh_particles: frame.stats.mesh_particles,
                draw_calls: frame.stats.draw_calls,
                lit_pixel_ratio: frame.stats.metrics.lit_pixel_ratio,
                hot_pixel_ratio: frame.stats.metrics.hot_pixel_ratio,
                source_effect_id: this.effect.effect_id.clone(),
            };
            this.last_preview = Some(readback.clone());
            this.preview_refresh_count += 1;
            Ok(readback)
        })
    }

    fn preview_workload(
        &self,
    ) -> Result<litd_engine::render::vfx::VfxRenderWorkloadStats, EngineError> {
        let bounds = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(3.0))?;
        let frustum = Frustum::orthographic(Aabb::from_center_half_extents(
            Vec3::ZERO,
            Vec3::splat(8.0),
        )?)?;
        let scene = [VfxSceneEffectInstance {
            instance_id: 1,
            effect: self.effect.clone(),
            world_bounds: bounds,
            screen_rect: ScreenRect::new(Vec2::new(0.22, 0.16), Vec2::new(0.78, 0.86))?,
            nearest_depth: 0.35,
            distance_meters: 12.0,
        }];
        build_vfx_render_workload(
            VfxRenderWorkloadSettings {
                target_width: 960,
                target_height: 540,
                max_effect_instances: 8,
                max_simulated_particles: 120_000,
                max_particle_buffer_bytes: 64 * 1024 * 1024,
                particle_attribute_stride_bytes: 64,
                far_lod_start_meters: 80.0,
                far_lod_particle_scale: 0.25,
                batching: BatchingSettings {
                    hzb_enabled: false,
                    ..BatchingSettings::default()
                },
            },
            &frustum,
            &[],
            &scene,
        )
    }

    fn selected_emitter_mut(&mut self) -> Result<&mut VfxEmitter, EngineError> {
        self.effect
            .emitters
            .get_mut(self.selected_emitter)
            .ok_or_else(|| vfx_editor_error_value("selected_emitter", "selected emitter missing"))
    }

    fn mark_edited(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.last_preview = None;
    }

    fn capture_error<T>(
        &mut self,
        run: impl FnOnce(&mut Self) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        match run(self) {
            Ok(value) => {
                self.last_error.clear();
                Ok(value)
            }
            Err(error) => {
                self.last_error = error.to_string();
                Err(error)
            }
        }
    }
}

fn save_readback(writeback: &VfxEffectStoreWriteback) -> EditorVfxSaveReadback {
    EditorVfxSaveReadback {
        object_path: writeback.object_path.display().to_string(),
        digest: writeback.descriptor.digest.clone(),
        sha256: writeback.readback.sha256.clone(),
        bytes: writeback.readback.bytes,
        deduplicated: writeback.deduplicated,
        effect_id: writeback.readback.effect.effect_id.clone(),
    }
}

fn safe_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn vfx_editor_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(vfx_editor_error_value(field, reason))
}

fn vfx_editor_error_value(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.vfx.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "vfx_editor_tests.rs"]
mod tests;
