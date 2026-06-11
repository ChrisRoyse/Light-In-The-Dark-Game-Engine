# Platform — Audio

> Expands [PRD §5.4 (Audio, UI, input)](../../PRD.md#54-audio-ui-input), requirement **R-AUD-1**.
> The [PRD](../../PRD.md) is the source of truth; this document elaborates, it does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Parent requirement** | R-AUD-1: OpenAL via G3N; `.ogg` only; 3D positional for world sounds, 2D for UI |

---

## 1. Scope and architectural position

Audio is a **presentation-layer concern**. It lives in `litd/render`'s sibling responsibility
area (`litd/asset` loads it, the presentation side plays it) and is subject to the same hard
rule as rendering: the simulation never imports it, and nothing in the audio path may feed
back into simulation state ([PRD §4.1](../../PRD.md#41-architecture-two-layers-one-implementation)).
A headless sim run (R-SIM-4) produces zero audio calls and identical state hashes — audio can
be globally disabled without any behavioral difference. Sounds are *triggered by* sim events
(unit attacked, building completed, ability cast) observed by the presentation layer, exactly
as the renderer observes positions.

This document refines R-AUD-1 into sub-requirements **R-AUD-1.1 … R-AUD-1.7**, covering the
backend, the codec policy, the 3D/2D split, the voice budget for 500-unit battles, and the
CC0 sourcing pipeline.

## 2. Backend: OpenAL via G3N (R-AUD-1.1)

- **R-AUD-1.1:** All audio output goes through G3N's OpenAL bindings (the engine ships
  OpenAL spatial audio support per the [G3N README](https://github.com/g3n/engine)). No
  second audio library enters the dependency tree; if the vendored G3N fork
  (`repoes/engine`) needs audio patches, we patch it there, consistent with the engine
  maintenance posture in [PRD §3.4](../../PRD.md#34-engine-viability-and-risks-g3n).

Practical consequences:

- The runtime dependency is **OpenAL Soft** (LGPL, dynamically linked — license-compatible
  with G4's open-source-only rule). Windows builds ship `soft_oal.dll`; Linux declares a
  package dependency; macOS uses the system framework or a bundled openal-soft.
- The listener is bound to the RTS camera rig — position from the camera's focus point on
  the terrain (not the camera eye, which sits high and behind), orientation fixed with the
  locked yaw of R-RND-1. Because the camera never rotates, stereo panning maps stably to
  screen-space left/right, which is what RTS players expect.
- Audio device loss or absence (CI machines, servers) degrades to a null backend: the audio
  manager runs its full bookkeeping (voice accounting, priority decisions) against a no-op
  device so the code path stays exercised in headless tests.

## 3. Codec policy: `.ogg` only (R-AUD-1.2)

- **R-AUD-1.2:** Every audio asset in `assets/` is Ogg Vorbis (`.ogg`). No WAV, no MP3, no
  FLAC, no Opus in v1. The asset-validation CLI (`tools/assetcheck`,
  [Tooling](../09-roadmap/tooling.md)) rejects any other container or codec at build time,
  mirroring the R-FMT-2 pattern for models.

Rationale: Vorbis is patent-free and royalty-free (G4), decodes cheaply on the dual-core
reference CPU, and compresses well enough that the full sound library fits comfortably
inside the 300 MB binary-plus-assets budget ([PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)).
Opus would be marginally better technically but adds a second decoder dependency for no
practical gain at our bitrates.

Channel-layout rules enforced by the validator:

| Asset class | Channels | Sample rate | Loading |
|---|---|---|---|
| World SFX (attacks, deaths, footsteps, spells) | **Mono** (required — OpenAL only spatializes mono sources) | 44.1 kHz | Fully decoded to buffers at map load |
| UI SFX (clicks, errors, alerts) | Mono or stereo | 44.1 kHz | Fully decoded at startup |
| Music | Stereo | 44.1 kHz | **Streamed** (decoded incrementally; never fully resident) |
| Unit voice lines (acknowledgements, "work complete") | Mono | 44.1 kHz | Decoded at map load with the owning unit type |

A stereo file in a world-SFX directory is a build error, not a runtime fallback — the same
fail-at-build philosophy as R-AST-2.

## 4. 3D world sounds vs 2D UI sounds (R-AUD-1.3)

- **R-AUD-1.3:** The engine exposes exactly two playback domains, and every sound asset is
  classified into one of them in its data-table entry (R-AST-1):
  - **World (3D positional):** attached to a world position or a unit's render entity.
    Distance attenuation (inverse-distance-clamped model), stereo panning from the fixed
    camera, subject to the voice budget and culling of §5. Examples: weapon impacts, death
    cries, building construction, spell effects.
  - **UI (2D):** played flat, full volume, no attenuation, exempt from distance culling.
    Examples: button clicks, error/"not enough gold" feedback, alert stingers
    ("our town is under attack"), control-group selection sounds, music.

Boundary cases are resolved by a single rule: **if the player must hear it regardless of
camera position, it is UI.** The under-attack alert is UI (a stinger) even though the attack
itself also produces world sounds at its location; unit acknowledgement voice lines
("Yes, milord") are UI, matching WC3 behavior — you hear your selection respond even when
the camera is elsewhere. The minimap ping sound is UI; the pinged location's ambient battle
is world. The HUD spec ([UI and HUD](./ui-and-hud.md)) lists the UI sound hooks per widget,
and the input spec ([Input](./input.md)) defines which gestures emit feedback sounds.

Because the camera's pitch and zoom are clamped (R-RND-1), attenuation constants are tuned
once against the fixed zoom range: reference distance ≈ one screen-height of world units,
max audible distance ≈ 1.5 screens beyond the viewport edge. Off-screen world sounds within
that margin still play (quietly) so approaching fights are audible before they are visible.

## 5. Voice budget for 500-unit battles (R-AUD-1.4, R-AUD-1.5)

A 500-unit battle ([PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
worst case) can generate hundreds of sound-trigger events per second. Playing them all is
neither feasible (OpenAL Soft mixes in software on our reference machine; every active voice
costs CPU) nor desirable (it sounds like noise). WC3 solved this with aggressive coalescing;
we adopt the same posture as a hard budget.

- **R-AUD-1.4 (voice budget):** ≤ **32 simultaneous voices** total, partitioned:
  **24 world**, **6 UI**, **2 music/ambience streams**. The partition prevents a huge battle
  from ever starving UI feedback. The audio manager preallocates all 32 OpenAL sources at
  startup and never creates or destroys sources mid-match — sources are pooled and recycled,
  consistent with R-GC-2 ([GC discipline](../08-performance/gc-discipline.md)). The
  steady-state audio path performs **zero heap allocations** (R-GC-1 applies to the frame
  path, which includes audio dispatch).

- **R-AUD-1.5 (admission control):** when a sound event arrives and no voice is free in its
  partition, the manager admits, coalesces, or drops it by these rules, applied in order:

  1. **Distance cull.** World events farther than the max audible distance from the listener
     are dropped before any voice accounting. In a 500-unit map-wide war, most events die here.
  2. **Duplicate coalescing (the WC3 rule).** Per sound asset, at most **N concurrent
     instances** (default 3, data-table-configurable) and a minimum **retrigger window**
     (default 50 ms — one sim tick) between starts. A 40-footman volley produces 3 sword
     impacts, not 40. Excess triggers within the window are merged into the existing
     instance: the manager may bump its gain slightly (capped) to convey density, never
     restart it.
  3. **Priority eviction.** Each sound class carries a priority from the data tables:
     `Alert > AbilityCast > Death > AttackImpact > Footstep/Ambient`. If a new event
     outranks the lowest-priority *currently playing* world voice, that voice is stolen
     (5 ms fade-out, then reuse). Equal priority: the new event wins only if it is closer
     to the listener than the quietest current instance.
  4. **Drop.** Anything that survives culling but cannot claim a voice is dropped silently.
     Dropping a sound is always correct — audio is presentation, never information the sim
     depends on.

All admission decisions are computed in the presentation layer from interpolated render
state; they are intentionally **non-deterministic with respect to the sim** (frame-rate
dependent) and must never write anything the sim reads.

The 32-voice ceiling is a starting budget, validated by the M4 render benchmark scene
([Budgets and Benchmarks §4](../08-performance/budgets-and-benchmarks.md)): the audio
manager's per-frame CPU cost (dispatch + OpenAL mixing share) must fit inside the render
frame budget on the reference machine. If profiling shows headroom, the budget may rise; it
may never be raised on the strength of high-tier hardware.

## 6. Memory and loading (R-AUD-1.6)

- **R-AUD-1.6:** Decoded audio resident in RAM is budgeted at **≤ 48 MB** per match,
  counted inside the 1.5 GB match budget. Per-map sound sets load with the map (inside the
  10 s map-load budget) and are immutable afterward; music streams in 64 KB chunks from a
  pooled ring of stream buffers. `tools/assetcheck` sums the decoded size of a map's
  declared sound set and fails the build if the budget is exceeded.

## 7. CC0 sourcing (R-AUD-1.7)

Matching the $0 asset posture of [PRD §3.3](../../PRD.md#33-assets-cc0-low-poly-fantasy-rts-packs-zero-cost):

- **R-AUD-1.7:** All shipped audio is **CC0** (public-domain dedication). Attribution-only
  licenses (CC-BY) are acceptable for development placeholders but must be replaced or
  cleared before any release build; the asset manifest records license per file and the
  validator rejects unknown licenses.

Primary sources, all verified CC0:

| Source | Contents | Notes |
|---|---|---|
| [Kenney — Audio packs](https://kenney.nl/assets?q=audio) (Impact Sounds, RPG Audio, UI Audio, Interface Sounds, Music Jingles) | UI clicks, impacts, fantasy SFX, stingers | Same vendor as the Castle Kit models (§3.3); house-style consistency for free |
| [freesound.org, CC0 filter](https://freesound.org/search/?license=Creative+Commons+0) | Combat foley, ambience, voice raw material | Per-file license check is mandatory — freesound mixes licenses; only the CC0 filter results qualify |
| [OpenGameArt, CC0 filter](https://opengameart.org/art-search-advanced?field_art_licenses_tid%5B%5D=4) | Fantasy SFX and music | Same per-file verification rule |

Unit voice lines (the WC3 acknowledgement flavor) are out of scope for v1 sourcing — the
engine supports them (§3 table), but the bundled demo assets ship with neutral SFX
acknowledgements until recorded CC0 voice sets exist.

## 8. Public API surface

Following the deduplication rules of
[PRD §4.2](../../PRD.md#42-deduplication-policy-the-complex-version-only-rule), the WC3
sound natives (`CreateSound`, `PlaySoundBJ`, `SetSoundPosition`, `KillSoundWhenDone`, the
`sound`/`soundtype` handle pair, and the volume-group natives) collapse into a small typed
surface on `g.Audio()`:

```go
a := g.Audio()
a.PlayAt("sfx/sword_impact", u.Position())      // world, 3D — subject to §5 budget
a.PlayUI("ui/click")                             // UI, 2D — exempt from distance rules
a.SetMusic("music/skirmish_theme")               // streamed; crossfades by default
a.SetVolume(litd.VolWorld, 0.8)                  // volume groups: World, UI, Music
```

There are no manually managed sound handles, no `KillSoundWhenDone` ritual — fire-and-forget
playback with pooled voices, lifetime owned by the manager (the audio analogue of R-API-2's
"no `RemoveLocation`"). Looping/positional ambient emitters return a small value handle with
`Stop()` only. The full native-to-canonical mapping is recorded in `api-manifest.json`
(R-AST-4).

## 9. Acceptance criteria

1. Headless sim runs (R-SIM-4) emit zero audio calls; state hashes are identical with audio
   enabled and disabled (determinism, G5).
2. `tools/assetcheck` rejects: non-ogg files, stereo world SFX, sounds missing a
   domain/priority data-table entry, per-map decoded sets over 48 MB, non-CC0 licenses in a
   release manifest.
3. The M4 benchmark scene at 500 units sustains the full 32-voice budget with the audio
   manager contributing zero steady-state heap allocations
   (`testing.AllocsPerRun` gate, [GC discipline §5](../08-performance/gc-discipline.md))
   and the frame budget still green
   ([Budgets and Benchmarks](../08-performance/budgets-and-benchmarks.md)).
4. Audible behavior in a 500-unit battle: no machine-gun retriggering, UI feedback never
   starved, alerts always audible (priority class test scenes).

## 10. Related documents

- [UI and HUD](./ui-and-hud.md) — UI sound hooks per widget; alert stinger ownership.
- [Input](./input.md) — gesture feedback sounds; error-feedback ("invalid order") policy.
- [Budgets and Benchmarks](../08-performance/budgets-and-benchmarks.md) — frame budget the audio path lives inside; benchmark scenes.
- [GC Discipline](../08-performance/gc-discipline.md) — source pooling, zero-alloc dispatch path.
- [PRD §5.4](../../PRD.md#54-audio-ui-input) — parent requirement R-AUD-1.
