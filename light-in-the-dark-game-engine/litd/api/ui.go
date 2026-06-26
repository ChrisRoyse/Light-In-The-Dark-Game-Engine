package litd

// UI surface (#245, ui-frames-and-dialogs.md). The full frame / dialog /
// leaderboard / multiboard / timer-dialog / quest system is render-only and
// built on G3N GUI widgets — it has no v1 backend and lands with the render
// UI milestone, so that whole family is tombstoned deferred-v2 in the JASS
// mapping. The one UI primitive that is pure presentation event (no widget
// tree, no render state) and therefore shippable + headlessly verifiable now
// is the text-message surface: Game.Print / Game.ClearMessages, which resolve
// their recipient list and forward one UIMessageEvent per addressed player to
// the optional Game.OnUI sink. Like audio/camera it is sim-inert: headless
// (nil sink) every call is a deterministic no-op and can never touch the hash.

// UIMessageEventKind tags a UIMessageEvent.
type UIMessageEventKind uint8

const (
	// UIPrint is a (timed) on-screen text message to one player.
	UIPrint UIMessageEventKind = iota
	// UIClear clears the text-message area for one player.
	UIClear
)

// UIMessageEvent is one resolved text-message request for a single recipient.
// Game.Print fans a multi-recipient call out to one event per player so the
// render layer (and tests) see an already-resolved, per-player stream. It
// carries no sim state.
type UIMessageEvent struct {
	// Kind selects which UI message action this event describes.
	Kind     UIMessageEventKind
	Player   int     // recipient slot
	Text     string  // message text (empty for UIClear)
	Duration float64 // seconds on screen; 0 = engine default / permanent
	HasPos   bool    // true when an explicit screen position was given
	Pos      Vec2    // normalized screen position when HasPos
}

// OnUI installs the render/test sink for the text-message surface. nil restores
// headless no-op behavior. The UI message surface is sim-inert, so installing
// a sink cannot change the state hash.
func (g *Game) OnUI(f func(UIMessageEvent)) {
	if g != nil {
		g.onUI = f
	}
}

// PrintOption configures Print (R-API-3 functional option).
type PrintOption func(*printConfig)

type printConfig struct {
	duration float64
	hasPos   bool
	pos      Vec2
}

// For sets how long the message stays on screen, in seconds. Non-positive
// durations are ignored (the engine default applies). JASS: the DisplayTimed*
// text variants collapse onto this option.
func For(seconds float64) PrintOption {
	return func(c *printConfig) {
		if seconds > 0 {
			c.duration = seconds
		}
	}
}

// At places the message at a normalized screen position. JASS: the x/y args of
// DisplayTextToPlayer / DisplayTimedTextToPlayer collapse here.
func At(pos Vec2) PrintOption {
	return func(c *printConfig) {
		c.hasPos = true
		c.pos = pos
	}
}

// Print shows a text message to each player in to. A nil/empty recipient list
// is a no-op (no "to all" footgun — pass the player set explicitly). Invalid
// player handles in the slice are skipped. Each valid recipient yields one
// resolved UIMessageEvent on the sink. JASS: DisplayText{To,Timed}{Player,Force}
// and BlzDisplayChatMessage collapse here (force/player/all targets → []Player,
// dedup D2/D3).
// JASS: BlzDisplayChatMessage, DisplayTextToForce, DisplayTextToPlayer, DisplayTimedTextFromPlayer, DisplayTimedTextToForce, DisplayTimedTextToPlayer
func (g *Game) Print(to []Player, msg string, opts ...PrintOption) {
	if g == nil || g.w == nil || len(to) == 0 {
		return
	}
	cfg := printConfig{}
	for _, o := range opts {
		o(&cfg)
	}
	for _, p := range to {
		if !p.Valid() {
			continue
		}
		g.emitUI(UIMessageEvent{
			Kind:     UIPrint,
			Player:   int(p.idx),
			Text:     msg,
			Duration: cfg.duration,
			HasPos:   cfg.hasPos,
			Pos:      cfg.pos,
		})
	}
}

// ClearMessages clears the text-message area for each player in to. JASS:
// ClearTextMessages / ClearTextMessagesBJ collapse here.
// JASS: ClearTextMessages, ClearTextMessagesBJ
func (g *Game) ClearMessages(to []Player) {
	if g == nil || g.w == nil || len(to) == 0 {
		return
	}
	for _, p := range to {
		if !p.Valid() {
			continue
		}
		g.emitUI(UIMessageEvent{Kind: UIClear, Player: int(p.idx)})
	}
}

// emitUI forwards a resolved event to the UI sink if one is installed.
func (g *Game) emitUI(ev UIMessageEvent) {
	if g.onUI != nil {
		g.onUI(ev)
	}
}

// --- g.UI() screen surface (#526, R-UI-1) ---------------------------------
//
// The text-message surface above ships the sim-inert *event* half of the UI;
// g.UI() extends the same posture to whole screens (main menu, skirmish setup,
// victory/defeat terminal screen, HUD panels) so the menu / match-flow / settings
// issues (#201, #211, #311) have the public surface their decks call for. It is
// the API foundation only: like OnAudio/OnCamera/OnUI, a screen request is
// validated, fanned to a resolved UIScreenEvent, and forwarded to an optional
// sink the render layer installs (the G3N/hud canvas binding is the render-side
// consumer, downstream of this surface). Strings are locale KEYS (D-17) — the
// render layer resolves them; the API never carries resolved user-facing text
// or any G3N type (R-API-6). Sim-inert: headless (nil sink) every call is a
// deterministic no-op that can never touch the state hash.

// UIScreenEventKind tags a UIScreenEvent.
type UIScreenEventKind uint8

const (
	// UIScreenShow builds and shows a screen (replacing one with the same ID).
	UIScreenShow UIScreenEventKind = iota
	// UIScreenHide hides/destroys the screen with the carried ID.
	UIScreenHide
)

// UIButton is one button on a screen: a stable ID for click routing, a locale
// key for the label (D-17), and an optional application command tag the render
// layer emits back through the input pipeline when the button is clicked.
type UIButton struct {
	ID       string // stable, screen-unique button id
	LabelKey string // locale key for the label
	Command  string // optional app command tag emitted on click
}

// UIScreen specifies a screen (menu / terminal / panel). All text fields are
// locale KEYS, not resolved strings.
type UIScreen struct {
	ID          string     // stable screen id (Show replaces, Hide targets)
	TitleKey    string     // locale key for the title
	SubtitleKey string     // locale key for the subtitle (optional)
	Buttons     []UIButton // ordered buttons (optional)
}

// UIScreenEvent is one resolved screen request forwarded to the sink. It
// carries no sim or G3N state. For UIScreenHide only Screen.ID is meaningful.
type UIScreenEvent struct {
	Kind   UIScreenEventKind
	Screen UIScreen
}

// OnUIScreen installs the render/test sink for the g.UI() screen surface. nil
// restores headless no-op behavior. Sim-inert: installing a sink cannot change
// the state hash.
func (g *Game) OnUIScreen(f func(UIScreenEvent)) {
	if g != nil {
		g.onUIScreen = f
	}
}

// UI is the screen-builder handle returned by Game.UI() (R-UI-1). Like every
// public noun it is a small copyable handle (a back-pointer to the Game); its
// methods forward resolved UIScreenEvents to the sink.
type UI struct{ g *Game }

// UI returns the screen-builder handle. JASS: the frame/dialog natives that
// have a v1 backend collapse onto this surface.
func (g *Game) UI() UI { return UI{g: g} }

// Valid reports whether the handle is attached to a live game (R-API-5). A
// zero-value UI{} (or one whose game was torn down) is invalid and makes every
// screen verb a no-op returning false.
func (u UI) Valid() bool { return u.g != nil && u.g.w != nil }

// Show validates and shows a screen. It returns whether the spec was accepted
// (stable id, non-empty title key, every button with a non-empty id+label and
// no duplicate id); an invalid spec is rejected with no event (fail closed).
// The accepted return is independent of whether a sink is installed, so a
// headless caller still gets deterministic validation feedback.
func (u UI) Show(s UIScreen) bool {
	if u.g == nil || u.g.w == nil {
		return false
	}
	if s.ID == "" || s.TitleKey == "" {
		return false
	}
	seen := make(map[string]bool, len(s.Buttons))
	for _, b := range s.Buttons {
		if b.ID == "" || b.LabelKey == "" || seen[b.ID] {
			return false
		}
		seen[b.ID] = true
	}
	u.g.emitUIScreen(UIScreenEvent{Kind: UIScreenShow, Screen: s})
	return true
}

// Hide hides the screen with the given id. Returns false for an empty id or a
// detached handle.
func (u UI) Hide(id string) bool {
	if u.g == nil || u.g.w == nil || id == "" {
		return false
	}
	u.g.emitUIScreen(UIScreenEvent{Kind: UIScreenHide, Screen: UIScreen{ID: id}})
	return true
}

// emitUIScreen forwards a resolved screen event to the sink if one is installed.
func (g *Game) emitUIScreen(ev UIScreenEvent) {
	if g.onUIScreen != nil {
		g.onUIScreen(ev)
	}
}
