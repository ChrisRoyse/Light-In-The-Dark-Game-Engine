use litd_engine::EngineError;

pub fn preferred_egui_surface_format(
    surface: &wgpu::Surface<'_>,
    adapter: &wgpu::Adapter,
) -> Result<wgpu::TextureFormat, EngineError> {
    let capabilities = surface.get_capabilities(adapter);
    capabilities
        .formats
        .iter()
        .copied()
        .find(|format| {
            matches!(
                format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
            )
        })
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.renderer.surface_format".to_string(),
            reason: format!(
                "egui/wgpu requires Bgra8Unorm or Rgba8Unorm; adapter exposed {:?}",
                capabilities.formats
            ),
        })
}

pub fn color_array(color: wgpu::Color) -> [f64; 4] {
    [color.r, color.g, color.b, color.a]
}
