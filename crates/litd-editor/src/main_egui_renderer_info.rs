//! #1485 finding 5a: the surface-derived, allocation-heavy fields of `RendererInfo` are cached so
//! the per-frame `info()` never re-clones the adapter report nor re-runs the `format!("{:?}", …)`
//! Debug formatting for the surface format / present mode.
//!
//! The cache type itself was promoted to `litd_engine::render::SurfaceRendererInfo` (finding 5) so
//! the shipping game runtime renderer shares exactly the same fix instead of duplicating it. This
//! module re-exports it under the editor-crate path the existing call sites already use.

pub(crate) use litd_engine::render::SurfaceRendererInfo;
