# Sound & Music — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §5.4 R-AUD-1](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~49** | `sound` CRUD/playback/3D params, music control, volume groups, environment/EAX, MIDI leftovers |
| `blizzard.j` BJs | **~59** | `PlaySoundBJ`/`...AtPointBJ`/`...OnUnitBJ`, percent-volume wrappers, `bj_lastPlayedSound`, cinematic transmission (`DoTransmissionBasicsXYBJ`) machinery |

## Representative JASS signatures

```jass
native CreateSound        takes string fileName, boolean looping, boolean is3D, boolean stopwhenoutofrange, integer fadeInRate, integer fadeOutRate, string eaxSetting returns sound
native StartSound         takes sound soundHandle returns nothing
native StopSound          takes sound soundHandle, boolean killWhenDone, boolean fadeOut returns nothing
native SetSoundVolume     takes sound soundHandle, integer volume returns nothing
native SetSoundPosition   takes sound soundHandle, real x, real y, real z returns nothing
native AttachSoundToUnit  takes sound soundHandle, unit whichUnit returns nothing
native PlayMusic          takes string musicName returns nothing
native SetMusicVolume     takes integer volume returns nothing
native VolumeGroupSetVolume takes volumegroup vgroup, real scale returns nothing

function PlaySoundBJ takes sound soundHandle returns nothing
function PlaySoundAtPointBJ takes sound soundHandle, real volumePercent, location loc, real z returns nothing
function SetMusicVolumeBJ takes real volumePercent returns nothing
```

## Canonical Go surface

```go
type Sound struct{ /* opaque handle into audio system */ }
type SoundCue string // asset key from data tables

// One canonical constructor; the 7-positional-arg CreateSound becomes options (R-API-3):
func (g *Game) NewSound(cue SoundCue, opts ...SoundOption) Sound
// opts: Looping(), Spatial3D(), StopOutOfRange(), FadeIn(d), FadeOut(d)

func (s Sound) Play()
func (s Sound) PlayAt(pos Vec2, z float64)      // PlaySoundAtPointBJ collapse
func (s Sound) PlayOn(u Unit)                   // AttachSoundToUnit + start
func (s Sound) Stop(fadeOut bool)
func (s Sound) SetVolume(v float64)             // 0..1, replaces 0..127 int + percent BJ
func (s Sound) SetPitch(p float64)
func (s Sound) SetPosition(pos Vec2, z float64)
func (s Sound) IsPlaying() bool                 // GetSoundIsPlaying/GetSoundIsLoading

// Music:
func (g *Game) PlayMusic(track string, opts ...MusicOption) // + PlayThematicMusic via Thematic()
func (g *Game) StopMusic(fadeOut bool)
func (g *Game) SetMusicVolume(v float64)

// Channel mixing (volumegroup):
func (g *Game) SetChannelVolume(ch AudioChannel, v float64) // VolumeGroupSetVolume
func (g *Game) ResetChannelVolumes()
```

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthroughs dropped | `PlaySoundBJ` → `Sound.Play()` |
| **D2** | percent/scale wrappers collapse onto one normalized 0..1 float | `SetMusicVolumeBJ(percent)` + `SetMusicVolume(0-127)` → `SetMusicVolume(0..1)` |
| **D3** | XY/Loc/unit-attached play variants → `PlayAt`/`PlayOn` with `Vec2` | `PlaySoundAtPointBJ`, `PlaySoundOnUnitBJ` |
| **D4** | cinematic transmission logic kept once | `DoTransmissionBasicsXYBJ` (portrait + subtitle + sound + wait) → `helpers.Transmit(speaker, text, dur, opts)` |
| **D5** | n/a (no state-table pairs) | — |

Tombstoned: MIDI natives (`SetMidiMusic*` — dead format), EAX environment strings
(proprietary; replaced by an `AudioEnvironment` preset enum mapping to OpenAL reverb),
`GetFileName`-style introspection oddities.

## Subsystem dependencies

- **render/audio** (primary): this is a **presentation-only category** — OpenAL via G3N (R-AUD-1), `.ogg` only. Sound playback must NEVER feed back into sim state (no `IsPlaying` branch may gate gameplay — see hazard 1).
- **sim**: emits sound *events* (unit died, ability cast) that the audio layer maps to cues via data tables; sim itself is silent (headless mode, R-SIM-4).
- **asset**: CC0 `.ogg` libraries; cue → file mapping + channel + default gain in `data/` tables (R-AST-1); validator checks formats.

## Porting hazards

1. **Determinism leak**: `GetSoundIsPlaying` reads audio-driver state — if a map script branches on it, replays diverge. LitD: `IsPlaying` is available only on the render-context API, not callable from sim handlers; document loudly.
2. **3D positional sounds during fog**: WC3 ducks/mutes sounds in unseen areas. Audio layer must consult the local player's visibility grid ([visibility-and-fog](visibility-and-fog.md)) — read-only dependency.
3. **Channel taxonomy**: WC3 volume groups (unit speech, combat, spells, UI, music, ambient) map to an `AudioChannel` enum; the data tables must tag every cue or mixing controls silently do nothing.
4. **Transmission helper scope**: `DoTransmission*` ties together portrait UI, subtitles, camera, and waits — it spans three subsystems. Keep it in `helpers`, built strictly on public API calls, as the test case proving the public API is sufficient (dogfooding gate).
5. **Sound handle exhaustion patterns**: WC3 maps recreate `sound` handles per play because handles are single-shot-ish. LitD `Sound` is reusable; pool OpenAL sources internally (R-GC-2) with a hard cap + LRU steal for low-tier hardware.
