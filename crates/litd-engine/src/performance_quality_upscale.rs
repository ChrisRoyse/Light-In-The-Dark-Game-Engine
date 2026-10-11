//! Upscale-aware internal-render-resolution derivation for the engine graphics
//! config (#1741). Kept in its own module so `performance_quality.rs` stays under
//! the 500-line ratchet. This is the real prod consumer that ties the FSR
//! `UpscaleSettings` to a render path: the produced-game runtime renderer reads
//! `GraphicsQualitySettings::upscale` and calls `internal_render_dimensions` to
//! size the world render at the reduced internal resolution.

use crate::{
    EngineError,
    render::upscale::{UpscalePlan, build_upscale_plan, upscale_error},
};

use super::GraphicsQualitySettings;

impl GraphicsQualitySettings {
    /// The FSR upscale plan for this config's target resolution, or `None` when
    /// upscaling is disabled or resolves to native (1x). Reads `self.upscale`, so
    /// a config with an invalid upscale knob fails loud here.
    pub fn upscale_plan(&self) -> Result<Option<UpscalePlan>, EngineError> {
        if !self.upscale.enabled {
            return Ok(None);
        }
        let plan = build_upscale_plan(self.upscale, self.target_width, self.target_height)?;
        if plan.is_native() {
            Ok(None)
        } else {
            Ok(Some(plan))
        }
    }

    /// Internal (pre-upscale) render resolution the runtime renders the world at:
    /// the FSR-derived reduced resolution when upscaling is active, else the native
    /// target resolution. This is the value the produced-game runtime renderer
    /// reads from the graphics config to size its render path.
    pub fn internal_render_dimensions(&self) -> Result<(u32, u32), EngineError> {
        match self.upscale_plan()? {
            Some(plan) => Ok((plan.render_width, plan.render_height)),
            None => {
                if self.target_width == 0 || self.target_height == 0 {
                    return Err(upscale_error(
                        "graphics_quality.target_resolution",
                        "target width and height must be non-zero",
                    ));
                }
                Ok((self.target_width, self.target_height))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::GraphicsQualityTier;
    use crate::EngineError;
    use crate::render::upscale::{UpscaleQualityMode, UpscaleSettings};

    #[test]
    fn default_ultra_config_derives_quality_upscale_internal_resolution() {
        // Ultra target is 3840x2160; default upscale is Quality (1.5x per dim).
        let settings = GraphicsQualityTier::Ultra.settings();
        assert!(settings.upscale.enabled);
        assert_eq!(settings.upscale.mode, UpscaleQualityMode::Quality);
        let (w, h) = settings.internal_render_dimensions().unwrap();
        assert_eq!((w, h), (2560, 1440));
        let plan = settings.upscale_plan().unwrap().expect("non-native plan");
        assert_eq!((plan.render_width, plan.render_height), (2560, 1440));
    }

    #[test]
    fn low_config_1080p_quality_upscale_renders_at_720p() {
        let settings = GraphicsQualityTier::Low.settings();
        // Low tier targets 1920x1080; default Quality upscale -> 1280x720 internal.
        assert_eq!(
            (settings.target_width, settings.target_height),
            (1920, 1080)
        );
        let (w, h) = settings.internal_render_dimensions().unwrap();
        assert_eq!((w, h), (1280, 720));
    }

    #[test]
    fn disabled_upscale_renders_at_native_target() {
        let mut settings = GraphicsQualityTier::High.settings();
        settings.upscale.enabled = false;
        assert!(settings.upscale_plan().unwrap().is_none());
        let (w, h) = settings.internal_render_dimensions().unwrap();
        assert_eq!((w, h), (settings.target_width, settings.target_height));
    }

    #[test]
    fn native_mode_is_treated_as_no_upscale() {
        let mut settings = GraphicsQualityTier::High.settings();
        settings.upscale = UpscaleSettings {
            mode: UpscaleQualityMode::Native,
            ..UpscaleSettings::default()
        };
        assert!(settings.upscale_plan().unwrap().is_none());
        let (w, h) = settings.internal_render_dimensions().unwrap();
        assert_eq!((w, h), (settings.target_width, settings.target_height));
    }

    #[test]
    fn invalid_upscale_knob_fails_loud_through_config() {
        let mut settings = GraphicsQualityTier::Ultra.settings();
        settings.upscale.sharpness = 5.0;
        match settings.internal_render_dimensions() {
            Err(EngineError::UpscaleInvalidConfig { field, .. }) => assert_eq!(field, "sharpness"),
            other => panic!("expected UpscaleInvalidConfig, got {other:?}"),
        }
    }
}
