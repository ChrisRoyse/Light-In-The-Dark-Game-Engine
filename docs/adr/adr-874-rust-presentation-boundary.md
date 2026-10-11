# ADR 874: Rust Presentation Boundary Reads Sim Snapshots Only

## Status

Accepted.

## Context

Issue #874 was written during the old g3n presentation plan. Doctrine #1071 supersedes that stack: LitD now uses the proprietary Rust/wgpu engine, and g3n is not part of the runtime architecture.

The decision itself remains binding. Render and presentation code must consume immutable deterministic sim snapshots, copy the data needed for presentation, and never write back into the deterministic sim state or sim hash.

## Decision

`litd_engine::presentation::extract_presentation_frame` is the Rust/wgpu-era boundary for this slice.

It accepts `&SimSnapshot`, validates presentation styles, and returns a copied `PresentationFrame` containing render-facing instances. Presentation frame mutation is local to the copied frame. The deterministic `FixedTickSim` remains the source of truth for state and hash.

Render batching consumes `PresentationFrame::render_instances()` and does not receive mutable sim state.

## Consequences

- Sim state remains deterministic and hash-owned by `FixedTickSim`.
- Presentation/render data is derived, copied, and disposable per frame.
- Missing or duplicate presentation styles fail closed with `LITD-RENDER-BATCHING-INVALID-CONFIG`.
- Tampered snapshots fail restore with `LITD-REFLECT-FINGERPRINT-MISMATCH`.
- Future render lifecycle work should keep this boundary and add diffed create/update/destroy render-node state outside the sim hash.

## Verification

Run:

```powershell
cargo run --locked -p litd-engine --features fsv --example issue874_presentation_boundary_fsv --quiet
```

The run writes `target/fsv/issue874_presentation_boundary/runs/<run>/manual-fsv-report.json` plus `latest-manifest.json`. Manual FSV must inspect the persisted report and hashes before #874 is closed.
