# DEC-1746: Render technology skip list

Status: adopted
Date: 2026-08-01
Scope: Milestone 1 renderer on the minimum supported PC target

## Decision

The runtime renderer remains a rasterized, stylized-PBR pipeline in Rust and
wgpu. The M1 implementation budget is spent on authored geometry, GPU-driven
frustum and Hi-Z culling, indirect draws where reliable, baked probe lighting,
LOD, and temporal upscaling. The CPU-culled path remains the compatibility
path for devices whose indirect-draw driver is unreliable.

The following technologies are deliberately skipped for M1:

1. Nanite-style virtualized geometry and a software visibility rasterizer.
2. Mesh shaders.
3. Runtime Gaussian splatting as world geometry.
4. Runtime diffusion or neural world models.
5. Tensor-gated upscalers or neural render paths as the default tier.

## Rationale

The supported hardware target includes GTX 1650, RX 6400, and Iris Xe class
devices. The skipped paths either require unavailable or unreliable features,
add fixed GPU and memory costs that consume the low-end frame budget, or solve
a photoreal-density problem that the stylized authored world does not have.
Runtime splats and diffusion models also do not provide the authored,
deterministic, persistent, destructible state required by gameplay, replay, and
LAN lockstep.

Tensor-gated paths are not the default because they exclude the minimum
hardware. A later high-end opt-in may be evaluated only as a separate feature.

## Watch list and revisit gates

- Radiance Cascades 3D GI: revisit only with a measured improvement over the
  baked probe grid on minimum hardware.
- Virtual texturing: revisit only after shipped asset measurements show that
  texture residency exceeds the available budget.
- Triangle Splatting+: keep as a timeboxed capture-to-mesh experiment, never as
  the runtime world representation.
- Neural Texture Compression: evaluate load-time decompression only after the
  asset pipeline measures a real storage or memory need and its license passes
  the zero-paid/permissive policy.
- High-end neural upscaling: evaluate only as an explicit opt-in after the
  minimum-tier path is complete.

Every revisit requires a reproducible benchmark, license review, and a
documented decision. Hype or a vendor demo is not adoption evidence.

## Sources

- `docs2/research/3d-world-tech/COMBINED-3d-world-tech-report.md`
- Render epic #1738 and its children #1743, #1744, and #1745
- Minimum-hardware doctrine #928 and hardware gate #1470
