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
