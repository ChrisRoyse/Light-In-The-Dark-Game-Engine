# UI: Frames, Dialogs, Boards & Quests — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §5.4 R-UI-1](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~190** | the largest native block after units: ~90 `BlzFrame*`/`BlzCreateFrame` natives, dialogs, leaderboards, multiboards, timer dialogs, quests, text messages, cursor/selection/mouse, trackables |
| `blizzard.j` BJs | **~88** | `DisplayTextToForce`, board row/column macros, quest BJs, `GetLastCreated*BJ` patterns |

## Representative JASS signatures

```jass
native BlzCreateFrame       takes string name, framehandle owner, integer priority, integer createContext returns framehandle
native BlzFrameSetVisible   takes framehandle frame, boolean visible returns nothing
native BlzGetFrameByName    takes string name, integer createContext returns framehandle
native DialogCreate         takes nothing returns dialog
native DialogAddButton      takes dialog whichDialog, string buttonText, integer hotkey returns button
native CreateLeaderboard    takes nothing returns leaderboard
native CreateMultiboard     takes nothing returns multiboard
native CreateTimerDialog    takes timer t returns timerdialog
native CreateQuest          takes nothing returns quest
native DisplayTimedTextToPlayer takes player toPlayer, real x, real y, real duration, string message returns nothing

function DialogDisplayBJ takes boolean flag, dialog whichDialog, player whichPlayer returns nothing
function MultiboardSetItemColorBJ takes multiboard mb, integer col, integer row, real red, real green, real blue, real transparency returns nothing
function QuestMessageBJ takes force f, integer messageType, string message returns nothing
```

## Canonical Go surface

Exposed via `g.UI()` (R-UI-1), built on G3N GUI widgets but with zero G3N types in
signatures (R-API-6):

```go
type UI struct{ /* render-layer facade */ }
func (g *Game) UI() *UI

// Frames — the general mechanism (Blz frame API is the "complex version"; boards
// and dialogs are presets over it where possible):
type Frame struct{ /* opaque widget handle */ }
func (ui *UI) NewFrame(kind FrameKind, parent Frame, opts ...FrameOption) Frame
func (ui *UI) FrameByName(name string) Frame
func (f Frame) SetVisible(b bool)
func (f Frame) SetText(s string)
func (f Frame) SetPoint(p FramePoint, rel Frame, relPoint FramePoint, offset Vec2)
func (f Frame) SetSize(w, h float64)
func (f Frame) OnClick(h func(p Player))    // frame events → closures (R-API-4)

// Modal dialogs:
type Dialog struct{ /* ... */ }
func (ui *UI) NewDialog(title string) Dialog
func (d Dialog) AddButton(text string, hotkey rune) DialogButton
func (d Dialog) Show(p Player, b bool)

// Boards:
func (ui *UI) NewLeaderboard(title string) Leaderboard
func (ui *UI) NewMultiboard(title string, rows, cols int) Multiboard
func (m Multiboard) Cell(row, col int) MultiboardCell  // D5: cell object replaces 15 SetItem* natives
func (c MultiboardCell) SetText(s string)
func (c MultiboardCell) SetColor(col Color)

func (ui *UI) NewTimerDialog(t Timer, title string) TimerDialog

// Quests & messages:
func (ui *UI) NewQuest(title, desc string, opts ...QuestOption) Quest
func (q Quest) SetCompleted(b bool)
func (ui *UI) Print(to []Player, msg string, opts ...TextOption) // all DisplayText* variants
```

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthrough BJs dropped | `DialogDisplayBJ` → `Dialog.Show` |
| **D2** | positional/timed text variants collapse | `DisplayTextToPlayer`/`DisplayTimedTextToPlayer`/`...ToForce` → `Print(to, msg, At(x,y), For(d))` |
| **D3** | force vs player vs all-players targets → `[]Player` slices | `QuestMessageBJ(force, ...)` → `Print(players, ...)` |
| **D4** | board row/col management macros kept once | `MultiboardSetItemColorBJ` per-cell loops → `Multiboard.Cell` object |
| **D5** | `BlzFrameGet*`/`BlzFrameSet*` pairs and `MultiboardSetItem{Value,Color,Icon,Width,Style}` families → typed accessors on `Frame`/`MultiboardCell` | ~40 natives become ~12 methods |

Tombstoned: `.fdf`/TOC loading (`BlzLoadTOCFile` — Blizzard UI format; LitD frames are
defined in code/Go options), trackables (superseded by `Frame.OnClick`/hover events),
`EnableUserUI`-era leftovers superseded by frame visibility.

## Subsystem dependencies

- **render** (primary): entirely presentation — G3N GUI widget tree. **Per-player display**: every show/print call carries target players; render applies only for `LocalPlayer()`.
- **sim**: dialog *pauses* in WC3 (modal dialogs paused the game) — decide and document; dialog button clicks and frame clicks are *input commands* that enter the sim command stream like any other input (deterministic, replayable).
- **asset**: fonts (open-licensed), icon atlas, 9-slice textures; UI skin tables in `data/`.

## Porting hazards

1. **Biggest async-desync surface**: clicks happen on one client at render rate. Every UI callback that mutates sim state must be routed as a player command into the next tick (R-EXEC-1), never executed inline in the render thread.
2. **Frame API is Reforged-era and huge** (~90 natives) — many map 1:1 onto G3N widget properties, but some (sprite frames, model frames, `BlzFrameSetTexture` with blp paths) assume Blizzard asset formats. Tombstone format-specific ones; keep capability via `FrameKind` presets.
3. **Leaderboard/multiboard exclusivity** (WC3 shows only one board at a time) is a quirk, not capability — drop the constraint, note divergence.
4. **Localized strings** (`GetLocalizedString`, hotkey lookup) need a Go i18n table mechanism early or every UI string call site churns later.
5. **Quest log, hints, "F9" UX** are WC3 chrome — implement as a default HUD layout built *on the public frame API* (dogfooding, same rule as the transmission helper).
6. **Text tags (floating world text) are NOT here** — they're world-space render objects in [effects-and-graphics](effects-and-graphics.md); only screen-space UI lives in this category.
