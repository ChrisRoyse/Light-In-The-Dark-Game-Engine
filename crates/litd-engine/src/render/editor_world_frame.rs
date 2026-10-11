use std::ops::Range;

use crate::{
    EngineError,
    determinism_boundary::{HotPathPhase, enter_hot_path},
    render::lighting::{DayNightLightingPlan, build_day_night_lighting_plan},
};

use super::{
    EditorViewportRect, EditorWorldCameraState, EditorWorldLayerVisibility,
    EditorWorldRenderEvidence, EditorWorldRenderer, EditorWorldSceneMetrics,
    editor_world_evidence::EditorWorldUniform,
    editor_world_evidence::{
        ATMOSPHERE_MODEL, ATMOSPHERE_PROFILE, default_editor_world_lighting_input,
    },
    editor_world_layers::{hidden_index_count, hidden_layer_count, visible_index_ranges},
    editor_world_visual_preset::default_editor_world_visual_preset,
    world_error,
};

/// Inputs captured from the last `prepare_frame_metrics` so [`EditorWorldRenderer::last_frame_evidence`]
/// can rebuild the full evidence byte-identically at snapshot cadence. The lighting PLAN (not the
/// raw input) is stored so the rebuild never re-runs the fallible `build_day_night_lighting_plan`.
pub(crate) struct LastFramePrep {
    viewport: EditorViewportRect,
    layers: EditorWorldLayerVisibility,
    lighting_plan: DayNightLightingPlan,
}

#[path = "editor_world_frame_args.rs"]
mod editor_world_frame_args;
pub use editor_world_frame_args::*;

impl EditorWorldRenderer {
    /// Editor entry point — returns the full [`EditorWorldRenderEvidence`]. Thin wrapper over
    /// [`prepare_frame_metrics`](Self::prepare_frame_metrics) + [`last_frame_evidence`](Self::last_frame_evidence):
    /// the per-frame GPU work is identical; this variant additionally materializes the evidence.
    pub fn prepare_frame(
        &mut self,
        args: PrepareFrameArgs<'_>,
    ) -> Result<EditorWorldRenderEvidence, EngineError> {
        let PrepareFrameArgs {
            device,
            queue,
            surface_width,
            surface_height,
            viewport,
            camera,
            layers,
        } = args;
        self.prepare_frame_metrics(PrepareFrameMetricsArgs {
            device,
            queue,
            surface_width,
            surface_height,
            viewport,
            camera,
            layers,
        })?;
        Ok(self.last_frame_evidence().expect(
            "last_frame_evidence is Some immediately after a successful prepare_frame_metrics",
        ))
    }

    /// Shipping/game entry point (#1485 finding 3) — does the per-frame GPU work but returns only
    /// the cheap [`EditorWorldFrameMetrics`] the frame loop consumes, storing the frame inputs so
    /// the allocation-heavy [`EditorWorldRenderEvidence`] can be rebuilt on demand at snapshot
    /// cadence via [`last_frame_evidence`](Self::last_frame_evidence) — instead of building ~25
    /// heap strings + several `format!`s every frame.
    pub fn prepare_frame_metrics(
        &mut self,
        args: PrepareFrameMetricsArgs<'_>,
    ) -> Result<EditorWorldFrameMetrics, EngineError> {
        let PrepareFrameMetricsArgs {
            device,
            queue,
            surface_width,
            surface_height,
            viewport,
            camera,
            layers,
        } = args;
        camera.validate()?;
        if viewport.x + viewport.width > surface_width
            || viewport.y + viewport.height > surface_height
        {
            return world_error("viewport_rect", "viewport exceeds current surface size");
        }
        let lighting_plan =
            build_day_night_lighting_plan(default_editor_world_lighting_input(layers), &[])?;
        self.depth
            .resize_if_needed(device, surface_width, surface_height);
        self.post
            .resize_if_needed(device, surface_width, surface_height, self.surface_format);
        let uniform =
            EditorWorldUniform::new(camera, viewport.aspect_ratio(), layers, &lighting_plan);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
        // Compute the frame-stable visible index ranges ONCE (#1485 finding 4) and reuse for the
        // post uniform update and the metrics.
        let visible_ranges = self.visible_ranges(layers);
        let visible_index_count = self.visible_index_count(&visible_ranges, layers);
        self.post.update_uniform(queue, layers, visible_index_count);
        let metrics = EditorWorldFrameMetrics {
            renderer_kind: "litd_engine_wgpu_editor_world_view",
            draw_calls: self.render_draw_calls(&visible_ranges, layers),
            visible_index_count,
            visible_triangle_count: visible_index_count / 3,
            triangle_count: self.scene.triangle_count,
            viewport,
        };
        // Capture the inputs so `last_frame_evidence` rebuilds the full evidence byte-identically
        // at snapshot cadence without redoing GPU work or the fallible lighting-plan build.
        self.last_frame = Some(LastFramePrep {
            viewport,
            layers,
            lighting_plan,
        });
        Ok(metrics)
    }

    /// Rebuild the full [`EditorWorldRenderEvidence`] from the last prepared frame's stored inputs
    /// plus current renderer state (#1485 finding 3). Byte-identical to the old per-frame build, but
    /// paid only when a snapshot/heartbeat actually consumes it. `None` before the first frame.
    pub fn last_frame_evidence(&self) -> Option<EditorWorldRenderEvidence> {
        let prep = self.last_frame.as_ref()?;
        let visible_ranges = self.visible_ranges(prep.layers);
        Some(self.evidence(
            prep.viewport,
            &visible_ranges,
            prep.layers,
            &prep.lighting_plan,
        ))
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        viewport: EditorViewportRect,
        clear_color: wgpu::Color,
        layers: EditorWorldLayerVisibility,
    ) {
        // Engine render dispatch is a deterministic-render hot path: guard it so
        // any Calyx/physics access on this thread during the frame is caught
        // (#1463). litd-game guards its runtime renderer the same way; the engine
        // world-frame renderer was previously unguarded.
        let _hot_path = enter_hot_path(HotPathPhase::RenderFrame);
        // Compute the frame-stable visible index ranges ONCE for this render entry point
        // and thread the slice into both opaque and additive pipeline draws, instead of
        // reallocating a fresh Vec inside each draw_with_pipeline call (#1485 finding 4).
        let visible_ranges = self.visible_ranges(layers);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("litd-editor-world-hdr-opaque-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.post.scene_view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(self.depth_attachment(wgpu::LoadOp::Clear(1.0))),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.draw_with_pipeline(&mut pass, viewport, &self.pipeline, &visible_ranges);
            self.draw_instanced_foliage(&mut pass, viewport, layers);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("litd-editor-world-additive-emissive-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.post.emissive_view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(self.depth_attachment(wgpu::LoadOp::Load)),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.draw_with_pipeline(
                &mut pass,
                viewport,
                &self.additive_pipeline,
                &visible_ranges,
            );
        }
        self.post.run_postprocess(encoder, output, clear_color);
    }

    pub fn render_graph_passes(&self) -> u32 {
        2 + self.post.postprocess_pass_count()
    }

    pub fn scene_metrics(&self) -> EditorWorldSceneMetrics {
        self.scene
    }

    fn depth_attachment(
        &self,
        load: wgpu::LoadOp<f32>,
    ) -> wgpu::RenderPassDepthStencilAttachment<'_> {
        wgpu::RenderPassDepthStencilAttachment {
            view: &self.depth.view,
            depth_ops: Some(wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }
    }

    fn draw_with_pipeline<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        viewport: EditorViewportRect,
        pipeline: &'a wgpu::RenderPipeline,
        visible_ranges: &[Range<u32>],
    ) {
        pass.set_viewport(
            viewport.x as f32,
            viewport.y as f32,
            viewport.width as f32,
            viewport.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(viewport.x, viewport.y, viewport.width, viewport.height);
        if visible_ranges.is_empty() {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_bind_group(1, self.fog.bind_group(), &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for range in visible_ranges {
            pass.draw_indexed(range.clone(), 0, 0..1);
        }
    }

    fn draw_instanced_foliage<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        viewport: EditorViewportRect,
        layers: EditorWorldLayerVisibility,
    ) {
        if !layers.foliage || self.foliage_instance_count == 0 {
            return;
        }
        pass.set_viewport(
            viewport.x as f32,
            viewport.y as f32,
            viewport.width as f32,
            viewport.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(viewport.x, viewport.y, viewport.width, viewport.height);
        pass.set_pipeline(&self.foliage_pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_bind_group(1, self.fog.bind_group(), &[]);
        pass.set_vertex_buffer(
            0,
            self.foliage_vertex_buffer
                .as_ref()
                .expect("validated foliage template vertex buffer must exist")
                .slice(..),
        );
        pass.set_vertex_buffer(
            1,
            self.foliage_instance_buffer
                .as_ref()
                .expect("validated foliage instance buffer must exist")
                .slice(..),
        );
        pass.set_index_buffer(
            self.foliage_index_buffer
                .as_ref()
                .expect("validated foliage template index buffer must exist")
                .slice(..),
            wgpu::IndexFormat::Uint32,
        );
        pass.draw_indexed(
            0..self.foliage_template_index_count,
            0,
            0..self.foliage_instance_count,
        );
    }

    fn evidence(
        &self,
        viewport: EditorViewportRect,
        visible_ranges: &[Range<u32>],
        layers: EditorWorldLayerVisibility,
        lighting: &DayNightLightingPlan,
    ) -> EditorWorldRenderEvidence {
        let visible_index_count = self.visible_index_count(visible_ranges, layers);
        let hidden_indices = self.hidden_index_count(layers);
        let visual_preset = default_editor_world_visual_preset();
        EditorWorldRenderEvidence {
            renderer_kind: "litd_engine_wgpu_editor_world_view",
            visual_preset_id: visual_preset.id,
            visual_preset_label: visual_preset.label,
            visual_preset_camera_profile: visual_preset.camera_profile_id,
            visual_preset_reference_images: visual_preset.reference_image_list(),
            visual_preset_contract: visual_preset.contract_summary(),
            scene_name: self.scene_metadata.scene_name.clone(),
            scene_source_id: self.scene_metadata.source_id.clone(),
            scene_source_path: self.scene_metadata.source_path.clone(),
            scene_dependency_count: self.scene_metadata.dependency_count,
            scene_material_count: self.scene_metadata.material_count,
            scene_texture_count: self.scene_metadata.texture_count,
            scene_image_count: self.scene_metadata.image_count,
            scene_is_blank: !self.has_geometry(),
            blank_grid_visible: self.grid_strength(visible_ranges, layers) > 0.0,
            base_color_texture_source_path: self
                .scene_metadata
                .base_color_texture_source_path
                .clone(),
            base_color_texture_width: self.scene_metadata.base_color_texture_width,
            base_color_texture_height: self.scene_metadata.base_color_texture_height,
            base_color_texture_bytes: self.scene_metadata.base_color_texture_bytes,
            normal_texture_source_path: self.scene_metadata.normal_texture_source_path.clone(),
            normal_texture_width: self.scene_metadata.normal_texture_width,
            normal_texture_height: self.scene_metadata.normal_texture_height,
            normal_texture_bytes: self.scene_metadata.normal_texture_bytes,
            metallic_roughness_texture_source_path: self
                .scene_metadata
                .metallic_roughness_texture_source_path
                .clone(),
            metallic_roughness_texture_width: self.scene_metadata.metallic_roughness_texture_width,
            metallic_roughness_texture_height: self
                .scene_metadata
                .metallic_roughness_texture_height,
            metallic_roughness_texture_bytes: self.scene_metadata.metallic_roughness_texture_bytes,
            shader_label: "litd-editor-world-shader",
            post_shader_label: "litd-editor-world-post-shader",
            hdr_format: "Rgba16Float",
            additive_blend_mode: "one_plus_one_additive",
            lighting_environment_summary: lighting.summary(),
            lighting_time_of_day_hours: format!("{:.2}", lighting.input.time_of_day_hours),
            lighting_phase: lighting.phase.label(),
            lighting_weather_summary: lighting.input.weather.summary(),
            lighting_primary_light: lighting.primary_light_name.clone(),
            lighting_primary_direction_to_light: lighting.primary_direction_label(),
            lighting_primary_intensity: format!("{:.3}", lighting.primary_intensity),
            lighting_weather_attenuation: format!("{:.3}", lighting.weather_attenuation),
            lighting_ambient: format!(
                "color={:.3},{:.3},{:.3};intensity={:.3}",
                lighting.ambient_color.x,
                lighting.ambient_color.y,
                lighting.ambient_color.z,
                lighting.ambient_intensity
            ),
            lighting_fog: format!(
                "color={:.3},{:.3},{:.3};max={:.3}",
                lighting.fog_color.x, lighting.fog_color.y, lighting.fog_color.z, lighting.fog_max
            ),
            atmosphere_model: ATMOSPHERE_MODEL,
            atmosphere_profile: ATMOSPHERE_PROFILE,
            bloom_passes: self.post.bloom_pass_count(),
            bloom_intensity: self.post.bloom_intensity_label(),
            exposure: self.post.exposure_label(),
            color_grade: self.post.color_grade_label(),
            background_model: self.post.background_model_label(),
            background_profile: self.post.background_profile_label(),
            depth_format: "Depth32Float",
            viewport_rect: format!(
                "{},{}->{},{}",
                viewport.x,
                viewport.y,
                viewport.x + viewport.width,
                viewport.y + viewport.height
            ),
            vertex_count: self.scene.vertex_count,
            index_count: self.scene.index_count,
            triangle_count: self.scene.triangle_count,
            mesh_primitives: self.scene.mesh_primitives,
            layer_state: layers.summary(),
            layer_count: self.layer_count(),
            hidden_layer_count: self.hidden_layer_count(layers),
            terrain_visible: layers.terrain,
            foliage_visible: layers.foliage,
            doodads_visible: layers.doodads,
            units_visible: layers.units,
            water_visible: layers.water,
            blight_visible: layers.blight,
            shadows_visible: layers.shadows,
            lighting_visible: layers.lighting,
            weather_visible: layers.weather,
            sky_visible: layers.sky,
            fog_effects_visible: layers.fog_effects,
            grid_visible: layers.grid,
            camera_bounds_visible: layers.camera_bounds,
            pathing_ground_visible: layers.pathing_ground,
            pathing_naval_visible: layers.pathing_naval,
            regions_visible: layers.regions,
            camera_objects_visible: layers.camera_objects,
            info_display_visible: layers.info_display,
            letterbox_mode: layers.letterbox_mode,
            lock_to_game_camera: layers.lock_to_game_camera,
            foliage_index_count: self.foliage_effective_index_count(),
            foliage_template_index_count: self.foliage_template_index_count,
            foliage_instance_count: self.foliage_instance_count,
            foliage_instanced_draw_calls: self.foliage_draw_calls(layers),
            foliage_distance_fade: "alpha_blend_smoothstep_cull_start_end",
            visible_index_count,
            visible_triangle_count: visible_index_count / 3,
            hidden_index_count: hidden_indices,
            hidden_triangle_count: hidden_indices / 3,
            draw_calls: self.render_draw_calls(visible_ranges, layers),
        }
    }

    fn has_geometry(&self) -> bool {
        self.scene.index_count > 0
    }

    // The single per-entry-point compute of the frame-stable visible index ranges.
    // Callers (render / prepare_frame*) compute this ONCE and thread the slice into the
    // private helpers below, which previously each recomputed it ~6x/frame (#1485 finding 4).
    fn visible_ranges(&self, layers: EditorWorldLayerVisibility) -> Vec<Range<u32>> {
        visible_index_ranges(self.scene.index_count, &self.scene_layers, layers)
    }

    fn visible_index_count(
        &self,
        visible_ranges: &[Range<u32>],
        layers: EditorWorldLayerVisibility,
    ) -> u32 {
        let scene_indices: u32 = visible_ranges
            .iter()
            .map(|range| range.end - range.start)
            .sum();
        if layers.foliage {
            scene_indices + self.foliage_effective_index_count()
        } else {
            scene_indices
        }
    }

    fn grid_strength(
        &self,
        visible_ranges: &[Range<u32>],
        layers: EditorWorldLayerVisibility,
    ) -> f32 {
        if !layers.grid {
            0.0
        } else if self.visible_index_count(visible_ranges, layers) == 0 {
            0.42
        } else {
            0.055
        }
    }

    fn render_draw_calls(
        &self,
        visible_ranges: &[Range<u32>],
        layers: EditorWorldLayerVisibility,
    ) -> u32 {
        self.post.postprocess_pass_count()
            + visible_ranges.len() as u32 * 2
            + self.foliage_draw_calls(layers)
    }

    fn foliage_effective_index_count(&self) -> u32 {
        self.foliage_template_index_count
            .saturating_mul(self.foliage_instance_count)
    }

    fn foliage_draw_calls(&self, layers: EditorWorldLayerVisibility) -> u32 {
        u32::from(layers.foliage && self.foliage_instance_count > 0)
    }

    fn hidden_index_count(&self, layers: EditorWorldLayerVisibility) -> u32 {
        hidden_index_count(&self.scene_layers, layers)
            + if layers.foliage {
                0
            } else {
                self.foliage_effective_index_count()
            }
    }

    fn layer_count(&self) -> u32 {
        self.scene_layers.len() as u32 + u32::from(self.foliage_instance_count > 0)
    }

    fn hidden_layer_count(&self, layers: EditorWorldLayerVisibility) -> u32 {
        hidden_layer_count(&self.scene_layers, layers)
            + u32::from(!layers.foliage && self.foliage_instance_count > 0)
    }
}
