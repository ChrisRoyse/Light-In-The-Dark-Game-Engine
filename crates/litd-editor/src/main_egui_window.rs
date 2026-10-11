use tracing::warn;
use winit::{dpi::PhysicalSize, window::Window};

pub const MIN_EDITOR_LOGICAL_WIDTH: f64 = 960.0;
pub const MIN_EDITOR_LOGICAL_HEIGHT: f64 = 540.0;

pub fn minimum_physical_size(pixels_per_point: f32) -> PhysicalSize<u32> {
    PhysicalSize::new(
        (MIN_EDITOR_LOGICAL_WIDTH as f32 * pixels_per_point).ceil() as u32,
        (MIN_EDITOR_LOGICAL_HEIGHT as f32 * pixels_per_point).ceil() as u32,
    )
}

pub fn corrected_minimum_size(
    requested_size: PhysicalSize<u32>,
    pixels_per_point: f32,
) -> Option<PhysicalSize<u32>> {
    let minimum = minimum_physical_size(pixels_per_point);
    if requested_size.width >= minimum.width && requested_size.height >= minimum.height {
        None
    } else {
        Some(PhysicalSize::new(
            requested_size.width.max(minimum.width),
            requested_size.height.max(minimum.height),
        ))
    }
}

pub fn request_minimum_inner_size(
    window: &Window,
    requested_size: PhysicalSize<u32>,
    corrected_size: PhysicalSize<u32>,
) {
    window.set_min_inner_size(Some(corrected_size));
    let returned_size = window.request_inner_size(corrected_size);
    warn!(
        requested_width = requested_size.width,
        requested_height = requested_size.height,
        corrected_width = corrected_size.width,
        corrected_height = corrected_size.height,
        returned_size = ?returned_size,
        "editor surface below minimum; requested corrected window size"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_size_scales_with_dpi() {
        assert_eq!(minimum_physical_size(1.5), PhysicalSize::new(1440, 810));
    }

    #[test]
    fn undersized_surface_requests_corrected_size() {
        assert_eq!(
            corrected_minimum_size(PhysicalSize::new(150, 150), 1.5),
            Some(PhysicalSize::new(1440, 810))
        );
        assert_eq!(
            corrected_minimum_size(PhysicalSize::new(1920, 1080), 1.5),
            None
        );
    }
}
