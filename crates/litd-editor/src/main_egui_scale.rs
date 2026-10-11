use litd_engine::EngineError;
use winit::dpi::PhysicalSize;

const REFERENCE_EDITOR_WIDTH: f32 = 1920.0;
const REFERENCE_EDITOR_HEIGHT: f32 = 1080.0;
const MAX_EDITOR_CHROME_ZOOM: f32 = 2.5;

pub fn non_zero_size(size: PhysicalSize<u32>) -> Result<PhysicalSize<u32>, EngineError> {
    if size.width == 0 || size.height == 0 {
        return Err(EngineError::InvalidSurfaceSize {
            width: size.width,
            height: size.height,
        });
    }
    Ok(size)
}

pub fn drawable_size(size: PhysicalSize<u32>) -> Option<PhysicalSize<u32>> {
    if size.width == 0 || size.height == 0 {
        None
    } else {
        Some(size)
    }
}

#[cfg(test)]
pub fn editor_chrome_zoom_factor(size: PhysicalSize<u32>) -> Result<f32, EngineError> {
    editor_chrome_zoom_factor_with_text_scale(size, 1.0)
}

#[cfg(test)]
pub fn editor_chrome_zoom_factor_with_text_scale(
    size: PhysicalSize<u32>,
    text_scale: f32,
) -> Result<f32, EngineError> {
    editor_chrome_zoom_factor_from_logical_dimensions(
        size.width as f32,
        size.height as f32,
        text_scale,
    )
}

/// Calculate chrome zoom from logical window dimensions.
///
/// Winit reports `inner_size` in physical pixels, while the automatic zoom is
/// a logical-editor policy. Callers must remove the OS DPI scale before this
/// calculation; applying that scale here and again in `target_pixels_per_point`
/// makes the minimum-size correction grow the window indefinitely on high-DPI
/// displays.
pub fn editor_chrome_zoom_factor_for_window(
    size: PhysicalSize<u32>,
    window_scale_factor: f64,
    text_scale: f32,
) -> Result<f32, EngineError> {
    if !window_scale_factor.is_finite() || window_scale_factor <= 0.0 {
        return Err(EngineError::UiRenderInvalidConfig {
            field: "editor.window_scale_factor".to_string(),
            reason: format!("expected a positive finite scale, got {window_scale_factor}"),
        });
    }
    editor_chrome_zoom_factor_from_logical_dimensions(
        size.width as f32 / window_scale_factor as f32,
        size.height as f32 / window_scale_factor as f32,
        text_scale,
    )
}

fn editor_chrome_zoom_factor_from_logical_dimensions(
    logical_width: f32,
    logical_height: f32,
    text_scale: f32,
) -> Result<f32, EngineError> {
    litd_editor::editor_preferences::validate_editor_text_scale(text_scale)?;
    if !logical_width.is_finite()
        || !logical_height.is_finite()
        || logical_width <= 0.0
        || logical_height <= 0.0
    {
        return Err(EngineError::InvalidSurfaceSize {
            width: logical_width.max(0.0) as u32,
            height: logical_height.max(0.0) as u32,
        });
    }
    let width_scale = logical_width / REFERENCE_EDITOR_WIDTH;
    let height_scale = logical_height / REFERENCE_EDITOR_HEIGHT;
    let automatic_zoom = width_scale
        .min(height_scale)
        .clamp(1.0, MAX_EDITOR_CHROME_ZOOM);
    let zoom = automatic_zoom * text_scale;
    if !zoom.is_finite() {
        return Err(EngineError::UiRenderInvalidConfig {
            field: "editor.chrome_zoom".to_string(),
            reason: format!(
                "computed non-finite chrome zoom from logical dimensions {}x{}",
                logical_width, logical_height
            ),
        });
    }
    Ok(zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_sized_window_is_not_drawable() {
        assert_eq!(drawable_size(PhysicalSize::new(0, 720)), None);
        assert_eq!(drawable_size(PhysicalSize::new(1280, 0)), None);
        assert_eq!(
            drawable_size(PhysicalSize::new(1280, 720)),
            Some(PhysicalSize::new(1280, 720))
        );
    }

    #[test]
    fn editor_chrome_zoom_keeps_reference_and_small_windows_at_native_scale() {
        assert_eq!(
            editor_chrome_zoom_factor(PhysicalSize::new(1920, 1080)).unwrap(),
            1.0
        );
        assert_eq!(
            editor_chrome_zoom_factor(PhysicalSize::new(1280, 720)).unwrap(),
            1.0
        );
    }

    #[test]
    fn editor_chrome_zoom_removes_os_scale_before_auto_zoom() {
        let zoom =
            editor_chrome_zoom_factor_for_window(PhysicalSize::new(3200, 1800), 2.5, 1.0).unwrap();
        assert_eq!(zoom, 1.0);
    }

    #[test]
    fn editor_chrome_zoom_rejects_invalid_os_scale() {
        let error = editor_chrome_zoom_factor_for_window(PhysicalSize::new(1920, 1080), 0.0, 1.0)
            .unwrap_err()
            .to_string();
        assert!(error.contains("editor.window_scale_factor"));
    }

    #[test]
    fn editor_chrome_zoom_scales_large_windows_from_the_limiting_axis() {
        let zoom = editor_chrome_zoom_factor(PhysicalSize::new(5107, 2083)).unwrap();

        assert!((zoom - 1.9287037).abs() < 0.0001);
    }

    #[test]
    fn editor_chrome_zoom_caps_extreme_desktop_spans() {
        assert_eq!(
            editor_chrome_zoom_factor(PhysicalSize::new(7680, 4320)).unwrap(),
            MAX_EDITOR_CHROME_ZOOM
        );
    }

    #[test]
    fn editor_chrome_zoom_fails_loud_for_zero_surface() {
        match editor_chrome_zoom_factor(PhysicalSize::new(0, 1080)) {
            Err(EngineError::InvalidSurfaceSize { width, height }) => {
                assert_eq!(width, 0);
                assert_eq!(height, 1080);
            }
            other => panic!("expected InvalidSurfaceSize, got {other:?}"),
        }
    }

    #[test]
    fn editor_chrome_zoom_composes_persisted_accessibility_scale() {
        assert_eq!(
            editor_chrome_zoom_factor_with_text_scale(PhysicalSize::new(1920, 1080), 1.5).unwrap(),
            1.5
        );
        let error = editor_chrome_zoom_factor_with_text_scale(PhysicalSize::new(1920, 1080), 2.1)
            .unwrap_err()
            .to_string();
        assert!(error.contains("text_scale"));
    }
}
