use basis_universal::{BasisTextureFormat, ColorSpace};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureRole {
    BaseColor,
    Normal,
    MetallicRoughness,
    UiSprite,
}

impl TextureRole {
    pub fn color_space(self) -> TextureColorSpace {
        match self {
            Self::BaseColor | Self::UiSprite => TextureColorSpace::Srgb,
            Self::Normal | Self::MetallicRoughness => TextureColorSpace::Linear,
        }
    }

    pub(super) fn basis_format(self) -> BasisTextureFormat {
        match self {
            Self::BaseColor | Self::UiSprite => BasisTextureFormat::ETC1S,
            Self::Normal | Self::MetallicRoughness => BasisTextureFormat::UASTC4x4,
        }
    }

    pub(super) fn basis_format_label(self) -> &'static str {
        match self.basis_format() {
            BasisTextureFormat::ETC1S => "ETC1S/BasisLZ",
            BasisTextureFormat::UASTC4x4 => "UASTC4x4",
        }
    }

    pub(super) fn basis_quality(self) -> u32 {
        match self {
            Self::BaseColor | Self::UiSprite => 192,
            Self::Normal | Self::MetallicRoughness => basis_universal::UASTC_QUALITY_DEFAULT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureColorSpace {
    Srgb,
    Linear,
}

impl TextureColorSpace {
    pub(super) fn basis_color_space(self) -> ColorSpace {
        match self {
            Self::Srgb => ColorSpace::Srgb,
            Self::Linear => ColorSpace::Linear,
        }
    }
}
