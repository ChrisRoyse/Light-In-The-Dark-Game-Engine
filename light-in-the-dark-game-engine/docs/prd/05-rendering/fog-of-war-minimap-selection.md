# Fog of War, Minimap, and Selection Rendering

> Covers the custom render features G3N does not ship — identified as a top risk in [PRD §8](../../PRD.md#8-risks): *"some natives need engine features G3N lacks, e.g. fog of war … fog-of-war, minimap, selection circles are custom shaders/render passes — scheduled inside M4."* All features must fit the ≤300 draw-call budget (**R-RND-3**) and zero-alloc steady state (**R-GC-1..3**, [PRD §5.3.1](../../PRD.md#531-go-garbage-collection-discipline)), and respect the sim/render firewall of [PRD §4.1](../../PRD.md#41-architecture-two-layers-one-implementation).
>
> Related: [Camera and Culling](./camera-and-culling.md) · [Batching and Draw Calls](./batching-and-draw-calls.md) · [Materials and Lighting](./materials-and-lighting.md) · [Terrain](./terrain.md)

---

## 1. Architectural rule: sim computes truth, render draws it

All three features follow the same shape, dictated by determinism (R-SIM-2) and the "simulation never imports render" rule:

- **The sim owns gameplay-true state**: the visibility grid (fog), entity positions/ownership (blips), selection sets and hit points (circles/bars). Visibility is *gameplay data* — it gates targeting, attack acquisition, and the `IsVisibleToPlayer`-family API natives — so it is computed deterministically in the tick, in fixed-point/ordered math, with zero involvement from the GPU.
- **The render layer is a pure consumer**: each frame (or at lower cadence where noted) it copies the relevant sim state into preallocated CPU-side buffers and uploads/draws. Nothing here ever feeds back into the sim.

This also means all three features work headlessly (R-SIM-4): CI can assert visibility-grid correctness without a GPU.

## 2. Fog of war

### 2.1 Sim side: the visibility grid

- The sim maintains a per-player **visibility grid** aligned to the pathing grid ([Terrain §5](./terrain.md)), at fog resolution (default: one fog cell = 4×4 pathing cells; for a 128×128-cell map, a 128×128 fog grid — WC3's own fog granularity). Each cell holds a 2-bit state: **hidden** (never seen), **explored** (seen before, no current vision — "black mask" lifted, fogged), **visible** (currently in sight).
- Each tick (or each Nth tick; fog updates at 4–5 Hz are imperceptible and WC3-authentic), unit sight ranges stamp visible-cell sets via precomputed radius masks; cliff-level line-of-sight blocking uses terrain height per [Terrain §5](./terrain.md). All buffers preallocated at map load (R-GC-2).
- The grid is authoritative for gameplay queries and for the replay/lockstep state hash.

### 2.2 Render side: texture-based ground overlay

G3N has no fog-of-war concept; we add one with a texture + shader term, the standard RTS technique:

1. The local player's visibility grid is copied each fog update into a preallocated `image.RGBA`-backed (or single-channel) CPU buffer — one byte per fog cell encoding the dim factor (hidden = 0, explored = dim, visible = full).
2. That buffer uploads into a persistent **fog texture** (128×128 for the default map — trivially small) via G3N's `texture.Texture2D` data-update path (`SetData`/`SetFromRGBA`, `repoes/engine/texture/texture2D.go`). The texture object is created once; updates rewrite its contents — zero steady-state allocation, negligible bandwidth.
3. Terrain and world materials sample the fog texture **in-shader**: world-space XZ → fog UV is a single affine transform (map extents are constants). Sampling with bilinear filtering plus a render-side temporal smoothing pass (exponential blend between the previous and current fog values, done on the CPU buffer at update time) gives soft fog edges and the gradual reveal feel without any extra render pass.

### 2.3 What gets fogged, and how

- **Terrain**: the terrain shader multiplies its output color by the fog factor (hidden → near-black; explored → ~40% dim, desaturated). Terrain in *explored* state stays drawn — the player remembers the map.
- **Units and dynamic entities**: not handled per-pixel. The render sync pass ([Batching §6](./batching-and-draw-calls.md)) already walks visible entities; it looks up each entity's fog cell and simply **does not draw** enemy/neutral entities outside *visible* cells (gameplay-correct hiding comes from the sim's visibility queries; this is the render mirror of it). Own units are never fogged.
- **Buildings in explored fog**: WC3 shows a "last seen" ghost of enemy buildings under fog. *Revised 2026-06-11 per the standing directive (decisions.md, second session): full WC3 parity in v1.* The sim keeps a per-player last-seen record for buildings (type, position, owner at last sighting — small fixed-size store, lockstep-safe because it derives purely from the visibility grid); the renderer draws the snapshot, not the live entity, until the cell is re-scouted. A destroyed building keeps its ghost until re-sighted, exactly as WC3.
- **Hidden (never-seen) area**: fully black via the same fog term; no separate black-mask geometry needed.

### 2.4 Shader integration and cost

The fog term is one texture sample + one multiply appended to the shared terrain/unit shaders (both PBR and unlit paths — [Materials §5](./materials-and-lighting.md)), implemented as a patch to the vendored G3N shader sources (`repoes/engine/renderer/shaders`) alongside the team-color uniform ([Batching §5](./batching-and-draw-calls.md)). **Draw-call cost: zero** — fog is a term in existing draws, not an overlay pass. (An alternative single fullscreen overlay quad was rejected: it cannot dim correctly around tall geometry like cliffs and buildings.)

Budget: 0 draw calls, one 128² texture (~16–64 KB), one upload per fog update (≤ 5 Hz), 0 allocs/frame.

## 3. Minimap

### 3.1 Terrain base: render-to-texture, rarely

The minimap background is a top-down image of the terrain:

- At **map load**, the render layer performs one orthographic top-down render of the terrain chunk meshes into an offscreen framebuffer (G3N's `gls` layer exposes framebuffer objects — `GenFramebuffer`/`BindFramebuffer` in `gls-desktop.go`; a small render-to-texture helper is added in `litd/render`, as G3N has no high-level RTT wrapper). Resolution: 256×256 (matching minimap screen size on a 1080p HUD).
- The result is cached as the **minimap base texture**. It is re-rendered only on terrain-mutating events (destructible doodad death, WC3-style terrain deformation if ever supported) — at most once per tick, typically once per match. Fallback if RTT proves troublesome on some driver: a CPU-side color rasterization from tile/splat data ([Terrain §4](./terrain.md)), which is fully deterministic and headless-friendly.

### 3.2 Entity blips and fog mask: per-frame composition

On top of the cached base, each frame draws:

1. **Fog mask**: the §2.2 fog texture, sampled by the minimap shader to darken hidden/explored areas. Free reuse — same texture, second consumer.
2. **Entity blips**: one **CPU-written blip buffer**. The render sync pass already iterates live entities; it additionally writes team-colored pixels (units: 2×2 px; buildings: 3×3–4×4 px) into a preallocated 256×256 RGBA buffer (cleared by `copy` from a zero buffer), respecting per-player visibility, then uploads to a persistent blip texture. At ≤ 1,000 entities this is microseconds of CPU work and avoids per-blip quads entirely.
3. **Viewport indicator**: the camera frustum's ground footprint ([Camera §4.2](./camera-and-culling.md)) drawn as a 4-segment line loop (one G3N `LineStrip` graphic with a preallocated 5-vertex buffer, updated in place).
4. **Ping/alert markers** (attack warnings, minimap pings from the API): a small pooled set of animated quads (≤ 8).

The minimap itself is **one textured GUI quad** whose shader composites base × fog × blips, plus the viewport lines and pings: **≤ 4–6 draw calls total**, inside the ~15-call overlay sub-budget of [Batching §1](./batching-and-draw-calls.md).

### 3.3 Interaction

Minimap is also an input surface (R-INP-1 / WC3 parity): click-to-move-camera (sets the camera anchor — [Camera §2.3](./camera-and-culling.md)), right-click-to-order, ping. UV→world mapping is the inverse of the blip transform; it lives in `litd/render` and feeds *commands* into the input system, preserving the one-way sim/render flow.

## 4. Selection circles and health bars

### 4.1 Selection circles: ground-projected decal quads

- Each selectable entity owns a **decal quad** in a preallocated pool (R-GC-2): a unit-sized quad with a ring texture (one shared 64² ring texture for all circles; radius from the unit's selection scale in game data — [Validation §3](../06-assets/validation-and-data.md)), tinted green/yellow/red by ownership (self/ally/enemy) via the same per-graphic uniform channel as team color ([Batching §5](./batching-and-draw-calls.md)).
- Quads are placed at the entity's position, oriented to the ground. On flat ground a small constant **polygon offset / depth bias** suffices (the tight near/far planes of [Camera §5.2](./camera-and-culling.md) leave abundant depth precision). On sloped or cliff-edge terrain, the quad is draped by sampling the terrain height at its 4 corners ([Terrain §5](./terrain.md)) — cheap, and visually adequate at RTS camera distance. Full projected-decal shaders are unnecessary in v1.
- All circles share one material/texture → they render as one consecutive batch; with the M3 instancing patch ([Batching §4](./batching-and-draw-calls.md)) they collapse to a single instanced draw.

### 4.2 Health bars: camera-facing billboards

- Health/mana bars are **billboards** above each entity — quads that face the camera. With a *fixed-yaw, fixed-pitch* camera, "billboarding" degenerates to a constant rotation: every bar shares one orientation, recomputed only if zoom mode changes. G3N's `graphic/sprite.go` provides camera-facing sprites, but our constant-orientation pooled quads are cheaper and allocation-free.
- Bar fill is **shader-driven**: one shared quad geometry + a `fill` and `color` per-graphic uniform (green→red ramp), no per-bar texture work, no geometry rewrites.
- Visibility policy (WC3 parity): bars show for selected units, on Alt-held for all, and damaged-unit policy per user setting. Worst case (Alt with 500 units) is the stress case: pooled quads, single shared material, single batch — and the instancing patch turns it into one draw.
- Same pooled-billboard infrastructure serves **blob shadows** ([Materials §6.1](./materials-and-lighting.md)) and floating combat text holders.

### 4.3 Budget accounting

| Feature | Draw calls (batched / instanced) | Steady-state allocs |
|---|---|---|
| Fog of war | 0 (shader term) | 0 (persistent texture, pooled buffers) |
| Minimap | ≤ 6 / ≤ 6 | 0 (persistent RTT + blip buffer) |
| Selection circles | ~1–3 per material batch / 1 | 0 (quad pool) |
| Health bars + blob shadows | ~2–4 / 2 | 0 (quad pool) |

Total fits the ~15-call decal/overlay allocation in [Batching §1](./batching-and-draw-calls.md) with headroom even without instancing.

## 5. Gameplay edge cases the visibility grid must carry

These are sim-side rules (the render layer just reflects them), listed here because they constrain the grid representation in §2.1:

- **Shared/allied vision**: a player's *rendered* fog is the union of their own grid and allies' grids (WC3 shared-vision semantics). The render layer composes the union at fog-upload time from per-player grids; the sim keeps grids strictly per-player because gameplay queries (targeting legality) are per-player.
- **Flying units and high ground**: sight stamping uses the caster's effective height (terrain cliff level + flying altitude class), so air units see over cliffs. The line-of-sight rule consumes cliff levels from [Terrain §5](./terrain.md).
- **Invisibility and true sight** (WC3 natives): orthogonal to the fog grid — an invisible unit in a *visible* cell is hidden by an entity-level flag unless a true-sight source covers it. The render sync's draw/skip decision (§2.3) tests both: cell visibility AND entity-level detectability. The fog texture itself never encodes invisibility.
- **Reveal effects** (map pings, scry-style abilities, victory reveal): temporary stamp sources with lifetimes, going through the same stamping path — no special render handling.
- **Night/day sight ranges**: stamping radius switches with the game clock (data-driven per unit, [Validation §3.3](../06-assets/validation-and-data.md) `sight_day`/`sight_night`).

All of the above are deterministic sim features; none add render cost beyond what §2 already prices.

## 6. Memory and bandwidth accounting

| Resource | Size | Lifetime |
|---|---|---|
| Per-player visibility grids (12 players × 128×128 × 2 bits) | ~48 KB total | match, preallocated |
| Fog upload buffer + smoothing buffer | 2 × 16 KB | match, preallocated |
| Fog texture (GPU) | 128×128 R8 ≈ 16 KB | match |
| Minimap base RTT + blip texture + zero-clear source | 3 × 256×256 RGBA ≈ 768 KB | match |
| Decal/billboard pools (circles, bars, shadows; sized to max entities) | < 2 MB CPU+GPU | match, preallocated |

Total well under a single unit atlas — negligible against the 1.5 GB budget. Per-frame upload bandwidth: one 16 KB fog texture at ≤ 5 Hz, one 256 KB blip texture at 60 Hz (~15 MB/s worst case) — acceptable even on UHD 620; if measurement disagrees, blip updates drop to 20 Hz (sim cadence) with zero visible difference, since blips move at sim rate anyway.

## 7. Milestone placement and acceptance

- All three features are **M4 scope** (per the PRD §8 risk mitigation: "scheduled inside M4"), because the API surface (M5) needs visibility natives and selection events working underneath.
- M4 exit adds to the render benchmark: fog enabled, minimap live, 500 units Alt-bars on — budgets of [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram) must hold with all overlays active.
- Headless CI (independent of M4 GPU work): visibility-grid determinism test — same command stream → identical fog-grid hashes across runs (extends the M1/M3 state-hash harness).
- API-surface dependency: the JASS visibility/fog-modifier native family (`IsVisibleToPlayer`, fog-modifier handles, minimap ping natives) maps onto the sim grid and the §3.2 ping pool; the M5 audit ([PRD §4.2](../../PRD.md#42-deduplication-policy-the-complex-version-only-rule)) verifies every such native has its backing feature here.

## 7.1 Implementation order inside M4

The three features have a deliberate dependency order:

1. **Visibility grid (sim)** — can land in M3 alongside pathfinding (it shares the grid infrastructure of [Terrain §5](./terrain.md)) and is required before any render work; ships with its determinism test.
2. **Decal/billboard pools** — selection circles and health bars first: they have no sim dependencies beyond selection state, they exercise the pooled-quad infrastructure that blob shadows ([Materials §6.1](./materials-and-lighting.md)) and pings reuse, and they make M4's interim builds playable for testing.
3. **Fog shader term** — lands with the terrain material work ([Terrain](./terrain.md)), since both patch the same vendored shader sources.
4. **Minimap** — last; it consumes the fog texture (§3.2.1), the camera footprint ([Camera §4.2](./camera-and-culling.md)), and the entity sync pass, all of which exist by then. The CPU-rasterized base fallback (§3.1) means minimap work cannot be blocked by RTT driver issues.

This ordering keeps every interim M4 build testable and isolates the only genuinely novel GPU work (RTT) at the end with a fallback in hand.

## 8. Requirements traceability

| Requirement / risk | Where satisfied |
|---|---|
| PRD §8 "features G3N lacks" risk | §2 (fog), §3 (minimap), §4 (selection/bars) — all designed as additions, none requiring G3N internals beyond the shared shader patch and an RTT helper |
| R-RND-3 (draw-call budget) | §4.3 accounting table; ~15-call overlay sub-budget of [Batching §1](./batching-and-draw-calls.md) |
| R-GC-1/2 (zero-alloc, pooled) | persistent textures and pooled quads throughout §2–§4; §6 memory table |
| R-SIM-2/4 (determinism, headless) | §1 architecture rule; §7 headless grid-hash CI test |
| R-INP-1 (minimap interaction) | §3.3 |

## 9. Open items

1. Fog update cadence (every tick vs every 4th tick) — pick by feel in M4; both fit budget.
2. Explored-fog "ghost building" snapshots — v1 scope per the standing directive (last-seen store in sim, snapshot rendering; see §2.3).
3. Whether minimap terrain base uses GPU RTT or CPU rasterization as the *default* (both implemented paths per §3.1) — decide on driver-compatibility evidence from M4 testing on Intel UHD.
