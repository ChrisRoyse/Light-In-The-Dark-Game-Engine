# Platform — UI and HUD

> Expands [PRD §5.4 (Audio, UI, input)](../../PRD.md#54-audio-ui-input), requirement **R-UI-1**.
> The [PRD](../../PRD.md) is the source of truth; this document elaborates, it does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Parent requirement** | R-UI-1: in-game HUD on G3N GUI widgets, exposed via `g.UI()` mirroring WC3 frame natives' capability after deduplication |

---

## 1. Scope and architectural position

The HUD is presentation: it reads simulation state through the same read-only boundary as
the renderer ([PRD §4.1](../../PRD.md#41-architecture-two-layers-one-implementation)) and
never mutates it. When the player clicks a command-card button, the HUD does not call into
the sim — it emits an input event that flows through the deterministic command pipeline
defined in [Input §8](./input.md). This keeps the HUD entirely outside the determinism
boundary (G5): a replay renders the same HUD because the sim state is the same, not because
HUD interactions were recorded.

This document refines R-UI-1 into sub-requirements **R-UI-1.1 … R-UI-1.6**: the WC3 HUD
layout, its construction on G3N widgets, the deduplicated public UI API, resolution scaling,
and performance constraints.

## 2. The WC3 HUD layout (R-UI-1.1)

- **R-UI-1.1:** The default in-game HUD reproduces the WC3 screen layout — the arrangement
  RTS players have pattern-matched for two decades. Every region below is a named, anchored
  panel that games built on the engine may restyle, hide, or replace through the public API
  (§4); the *default* skin ships with the engine using CC0 art.

```
┌──────────────────────────────────────────────────────────────────┐
│ [Menu] [Allies] [Log]                  [Gold] [Lumber] [Food] ▲  │  top bar
│                                                                  │
│                                                                  │
│                        ( game viewport )                         │
│                                                                  │
│ [idle worker]                                                    │
│ [control-group bar 0–9]                                          │
├──────────┬──────────────┬──────────────────────────┬─────────────┤
│          │              │                          │             │
│ minimap  │   portrait   │   info panel             │  command    │
│ frame    │  (selected)  │   (selection grid /      │  card       │
│          │              │    unit stats / queue)   │  4 × 3      │
└──────────┴──────────────┴──────────────────────────┴─────────────┘
```

| Region | Contents | Behavior |
|---|---|---|
| **Resource bar** (top-right) | Gold, lumber, food (used/cap), upkeep indicator | Read-only labels bound to player state; flashes + UI error sound on insufficient-resource feedback ([Audio §4](./audio.md)) |
| **Menu cluster** (top-left) | Menu, allies, message log buttons | Opens modal panels; pauses sim only in single-player per game rules, not engine policy |
| **Minimap frame** (bottom-left) | Minimap render target, ping overlay, camera-bounds rectangle | The minimap *image* is a custom render pass owned by `litd/render` ([PRD §8](../../PRD.md#8-risks), fog-of-war row); the HUD owns the frame, click-to-move-camera, right-click-to-order, and alt-ping interactions, all routed through [Input](./input.md) |
| **Portrait** (bottom, left-of-center) | Animated portrait of the primary selected unit (the `Portrait` animation clip, R-AST-3), name, level | Falls back to a static icon when the model lacks a `Portrait` clip |
| **Info panel** (bottom-center) | Context view of the current selection: single unit → stats (life, mana, armor, attack); multi-select → selection grid of unit buttons (subgroup-aware, see [Input §3.4](./input.md)); building → production queue with cancel buttons | Selection-grid buttons re-emit selection commands through the input pipeline |
| **Command card** (bottom-right) | **4 × 3 grid** of order buttons for the active subgroup: move/stop/hold/attack/patrol, abilities, build menus | Button positions and hotkeys come from data tables (R-AST-1); grid position ↔ hotkey mapping defined in [Input §5](./input.md) |
| **Control-group bar** (above bottom panel, optional) | Buttons for non-empty control groups 0–9 with unit-count badges | Mirrors the state defined in [Input §4](./input.md); click = recall, double-click = recall + center camera |
| **Idle-worker button** (above minimap) | Cycles idle workers | Emits a select-and-center command |

## 3. Construction on G3N GUI widgets (R-UI-1.2)

- **R-UI-1.2:** The HUD is built exclusively on G3N's integrated GUI toolkit
  ([G3N README](https://github.com/g3n/engine): integrated GUI with no external
  dependencies) — `gui.Panel`, `gui.Button`, `gui.Label`, `gui.Image`, plus engine-side
  composites we implement on those primitives (nine-slice panel, icon-grid, progress bar).
  No immediate-mode GUI library, no HTML/CSS layer, no second windowing dependency.

Engine-side composites live in `litd/render/hud` and are *internal*; the public API (§4)
never exposes G3N types, per R-API-6. Notable implementation constraints:

- **Draw-call discipline.** HUD rendering must fit inside the ≤ 300 draw-call frame budget
  (R-RND-3). All HUD iconography — command icons, resource icons, portrait frames,
  nine-slice borders — packs into a single UI atlas texture so panels batch; the target is
  **≤ 20 draw calls for the entire HUD**. The minimap and portrait are each one textured
  quad fed by their own small render-to-texture pass, counted in the render budget
  ([Budgets and Benchmarks](../08-performance/budgets-and-benchmarks.md)).
- **Zero steady-state allocation.** Label updates (resource counts, life values) are the
  classic Go allocation trap (string formatting per frame). HUD labels update **only on
  observed state change**, through preallocated byte buffers and `strconv.AppendInt`-style
  formatting — never `fmt.Sprintf` in the frame path
  ([GC Discipline §3](../08-performance/gc-discipline.md), R-GC-3).
- **Dirty-flag refresh.** The HUD diffes against a small snapshot of the displayed values
  (resources, selection version, queue version) once per frame; widgets repaint only when
  their backing value changed. A static HUD costs near-zero CPU.

## 4. Public UI API: WC3 frame natives, deduplicated (R-UI-1.3)

The WC3 frame API (`framehandle`, `BlzCreateFrame`, `BlzCreateFrameByType`,
`BlzFrameSetPoint`/`SetAbsPoint`/`SetSize`/`SetText`/`SetTexture`/`SetVisible`/`SetEnable`/
`SetValue`/`SetAlpha`, `BlzFrameGetChild`, `BlzGetFrameByName`, the `originframe` accessors,
plus the older `leaderboard`, `multiboard`, `timerdialog`, and `dialog` handle families) is
one of the most duplicated regions of the source surface. Applying the
[PRD §4.2](../../PRD.md#42-deduplication-policy-the-complex-version-only-rule) rules:

| Source pattern | Rule | Canonical result |
|---|---|---|
| `BlzFrameSetPoint` vs `BlzFrameSetAbsPoint` vs `BlzFrameClearAllPoints` + re-set | D2/D3 | One anchor model: `Frame.SetAnchor(rel Anchor, target Frame, offset Vec2)`; absolute placement is anchoring to the root frame — the general ("complex") form subsumes the special case |
| `BlzCreateFrame` (named template) vs `BlzCreateFrameByType` | D3 | `ui.NewFrame(kind FrameKind, opts ...FrameOpt)`; styles/templates are an options field, not a second constructor |
| Get/set pairs (`BlzFrameGetText`/`SetText`, visible, enable, alpha, value) | D5 | Typed accessor pairs on `Frame`: `Text()/SetText`, `Visible()/SetVisible`, `Enabled()/SetEnabled`, `Alpha()/SetAlpha`, `Value()/SetValue` |
| `leaderboard` + `multiboard` handle families (two overlapping tabular-display APIs) | D3 | One `ui.Board` type (rows × columns of icon+text cells) covering both capabilities; `api-manifest.json` maps every native of both families onto it |
| `timerdialog` | D2 | `ui.Board`/label composition plus a `helpers.TimerDisplay` convenience (D4 — real logic, layered on top) |
| `DialogCreate`/`DialogAddButton`/`DialogDisplay` (modal dialogs) | D2 | `ui.ShowDialog(DialogSpec) <-chan DialogResult` — one call, options struct, result delivered as an event |
| Frame event natives (`BlzTriggerRegisterFrameEvent`) | per R-API-4 | `frame.OnClick(func(...))` etc. — Go closures replace the trigger zoo |

- **R-UI-1.3:** Every frame-related native and BJ wrapper in the 2,521-function surface is
  mapped to this canonical `g.UI()` surface or tombstoned with a reason in
  `api-manifest.json` (R-AST-4); the M5 audit report covers UI like every other subsystem.
  Capability is preserved — anything a WC3 map could draw with frames, a LitD game can draw
  with `g.UI()` — but the surface collapses to roughly a dozen types: `UI`, `Frame`,
  `Button`, `Label`, `Image`, `Bar`, `Board`, `DialogSpec`, `Anchor`, `FrameKind`, plus the
  named default-HUD accessors below.

The default HUD regions are reachable as named frames, the analogue of WC3's "origin
frames":

```go
ui := g.UI()
ui.CommandCard().SetVisible(false)              // hide the default card entirely
ui.ResourceBar().Frame("gold").SetTexture(...)  // reskin one element
ui.Minimap().SetAnchor(litd.TopRight, ui.Root(), litd.Vec2{})  // move the minimap

custom := ui.NewFrame(litd.FrameButton,
    litd.WithParent(ui.Root()),
    litd.WithSize(120, 32),
    litd.WithText("Surrender"))
custom.OnClick(func() { g.Defeat(g.LocalPlayer(), "surrendered") })
```

Per R-API-6 nothing in these signatures is a G3N type; `litd/render/hud` is the only place
the two meet.

## 5. Resolution scaling (R-UI-1.4)

WC3 laid frames out in a fixed 0.8 × 0.6 virtual coordinate space designed for 4:3 displays
and stretched or pillar-boxed from there. We keep the *virtual coordinate space* idea
(resolution-independent layout) but design for modern aspect ratios:

- **R-UI-1.4:** HUD layout is computed in a **virtual canvas of 1280 × 720 reference
  units**, scaled uniformly by `scale = windowHeight / 720` so HUD elements occupy a
  constant fraction of screen *height* at any resolution. Horizontal space beyond 16:9
  (ultrawide) extends the viewport, not the HUD: panels are anchored to screen corners and
  edges (the §2 regions to bottom-left/bottom-center/bottom-right/top-right), so they spread
  naturally without stretching. Aspect ratios narrower than 4:3 are unsupported.
- Supported window range: **1024 × 768 minimum** through 4K. On the reference machine's
  common 1366 × 768 panel the scale factor is ≈ 1.07; the default skin's fonts and icons
  are authored to remain legible at scale 1.0.
- Scale quantization: nine-slice borders and icon cells snap to integer pixel sizes after
  scaling to avoid texture shimmer; text renders at the final pixel size (no scaled
  bitmaps). An explicit user `uiScale` multiplier (0.75–1.5) stacks on top for
  accessibility and is the engine's only DPI accommodation in v1.
- Live resize: the HUD relayouts on window-resize events; relayout is allowed to allocate
  (it is an explicit user action, like map load — outside R-GC-1's steady-state clause).

## 6. Performance constraints summary (R-UI-1.5)

- **R-UI-1.5:** The HUD is included in every render-budget gate from M4 onward:
  - ≤ 20 draw calls for the full default HUD (inside R-RND-3's 300);
  - zero heap allocations per frame at steady state with a full HUD and 500-unit selection
    churn (R-GC-1; enforced via `testing.AllocsPerRun` on the HUD update path,
    [GC Discipline §5](../08-performance/gc-discipline.md));
  - HUD update + paint ≤ 1 ms/frame on the reference machine, measured by the scripted
    render benchmark scene which includes selection-change and resource-tick HUD churn
    ([Budgets and Benchmarks §4](../08-performance/budgets-and-benchmarks.md)).

## 7. Default skin assets (R-UI-1.6)

- **R-UI-1.6:** All default-skin art (panel borders, button frames, resource icons, command
  icons) is CC0, consistent with G4. Primary sources:
  [Kenney UI packs](https://kenney.nl/assets?q=ui) (UI Pack, Game Icons) and
  [game-icons.net](https://game-icons.net/) entries verified CC0/CC-BY (CC-BY entries are
  development placeholders only, same replacement rule as
  [Audio §7](./audio.md)). Icons pack into the single UI atlas of §3 at build time by the
  asset pipeline; `tools/assetcheck` validates atlas residency and license metadata.

## 8. Acceptance criteria

1. The default HUD reproduces every §2 region, bound to live sim state, in the M6 vertical
   slice.
2. The M5 audit report shows every frame/leaderboard/multiboard/timerdialog/dialog native
   mapped to a `g.UI()` symbol or tombstoned — zero unmapped, zero duplicated
   ([PRD §4.2](../../PRD.md#42-deduplication-policy-the-complex-version-only-rule) acceptance criterion applied to UI).
3. HUD interactions reach the sim only as serialized commands ([Input §8](./input.md));
   a replayed command stream reproduces identical sim state with the HUD hidden (G5).
4. Render benchmark scene passes R-UI-1.5's draw-call, allocation, and frame-time gates on
   the reference machine.
5. Layout verified correct (no overlap, no off-screen panels, legible text) at 1024 × 768,
   1366 × 768, 1920 × 1080, 2560 × 1080, and 3840 × 2160 via automated screenshot
   comparison in the render harness.

## 9. Related documents

- [Input](./input.md) — command card hotkeys, selection grid behavior, the command pipeline HUD clicks feed into.
- [Audio](./audio.md) — UI sound domain, per-widget sound hooks, alert stingers.
- [Budgets and Benchmarks](../08-performance/budgets-and-benchmarks.md) — frame-time and draw-call gates the HUD lives inside.
- [GC Discipline](../08-performance/gc-discipline.md) — zero-allocation label updates, dirty-flag patterns.
- [PRD §5.4](../../PRD.md#54-audio-ui-input) — parent requirement R-UI-1.
