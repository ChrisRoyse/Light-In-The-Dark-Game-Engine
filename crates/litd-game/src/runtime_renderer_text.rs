use litd_engine::{
    EngineError,
    render::{
        editor_world::EditorWorldCameraState,
        font::FontAtlasBuildRequest,
        runtime_font::{
            RUNTIME_FONT_LICENSE, RUNTIME_FONT_LICENSE_NOTICE_SHA256, RUNTIME_FONT_SOURCE_BYTES,
            RUNTIME_FONT_SOURCE_VERSION, build_runtime_font_atlas,
        },
        runtime_text::{FontAtlasCache, RuntimeTextScene, build_runtime_text_plan},
        runtime_text_gpu::RuntimeTextGpuRenderer,
        ui::UiRenderSettings,
    },
};

use crate::{runtime_host_module::RuntimeTextSnapshot, runtime_state_text::RuntimeTextState};

const MAX_RUNTIME_TEXT_QUADS: usize = 4_096;
const RUNTIME_FONT_PX: f32 = 40.0;
const RUNTIME_FONT_CELL_PX: u32 = 64;
const RUNTIME_FONT_SPREAD_PX: u32 = 8;
const RUNTIME_FONT_COLUMNS: u32 = 16;

pub(super) struct GameRuntimeTextRenderer {
    cache: FontAtlasCache,
    request: FontAtlasBuildRequest,
    gpu: RuntimeTextGpuRenderer,
    state: RuntimeTextState,
}

impl GameRuntimeTextRenderer {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
    ) -> Result<Self, EngineError> {
        // The packaged host embeds and verifies its licensed font bytes. There is
        // no system-font lookup or fallback.
        let (request, build) = build_runtime_font_atlas(
            (32_u8..=126).map(char::from).collect(),
            RUNTIME_FONT_PX,
            RUNTIME_FONT_CELL_PX,
            RUNTIME_FONT_SPREAD_PX,
            RUNTIME_FONT_COLUMNS,
        )?;
        let mut cache = FontAtlasCache::new();
        let atlas = cache.insert_prebuilt(request.clone(), build)?;
        let gpu = RuntimeTextGpuRenderer::new(
            device,
            queue,
            target_format,
            atlas.as_ref(),
            MAX_RUNTIME_TEXT_QUADS,
        )?;
        let state = RuntimeTextState::new(
            atlas.atlas.source_font_path.clone(),
            RUNTIME_FONT_SOURCE_VERSION.to_string(),
            RUNTIME_FONT_SOURCE_BYTES,
            atlas.atlas.source_font_sha256.clone(),
            RUNTIME_FONT_LICENSE.to_string(),
            RUNTIME_FONT_LICENSE_NOTICE_SHA256.to_string(),
            atlas.atlas.fnv1a64.clone(),
        );
        Ok(Self {
            cache,
            request,
            gpu,
            state,
        })
    }

    pub(super) fn render(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        viewport: [u32; 2],
        camera: EditorWorldCameraState,
        snapshot: &RuntimeTextSnapshot,
    ) -> Result<RuntimeTextState, EngineError> {
        self.state.begin_frame(
            snapshot.session_tick,
            snapshot.labels.len(),
            snapshot.damage_numbers.len(),
        );
        let settings = UiRenderSettings {
            viewport_width: viewport[0] as f32,
            viewport_height: viewport[1] as f32,
        };
        let view_projection = match camera
            .checked_view_projection(settings.viewport_width / settings.viewport_height)
        {
            Ok(view_projection) => view_projection,
            Err(error) => return Err(self.record_error("camera.projection", error)),
        };
        let scene = RuntimeTextScene {
            settings,
            view_projection,
            labels: snapshot.labels.clone(),
            damage_numbers: snapshot.damage_numbers.clone(),
        };
        let plan = match build_runtime_text_plan(&mut self.cache, &self.request, &scene) {
            Ok(plan) => plan,
            Err(error) => return Err(self.record_error("plan.build", error)),
        };
        self.state.record_plan(
            plan.fnv1a64.clone(),
            plan.rendered_count,
            plan.empty_count,
            plan.off_screen_count,
            plan.quads.len(),
        );
        let gpu = match self.gpu.render(queue, encoder, target, viewport, &plan) {
            Ok(gpu) => gpu,
            Err(error) => return Err(self.record_error("gpu.submit", error)),
        };
        self.state
            .record_submission(gpu.quad_count, gpu.vertex_count);
        Ok(self.state.clone())
    }

    pub(super) fn state(&self) -> &RuntimeTextState {
        &self.state
    }

    fn record_error(&mut self, stage: &'static str, error: EngineError) -> EngineError {
        self.state.record_error(stage, error.to_string());
        error
    }
}
