package render

// Sim-event → sound trigger (#313, render side). The sim publishes one-shot
// presentation cues on the NON-HASHING render-event channel (sim.EmitRenderEvent →
// Snapshot.Events; see discovery #449 — never g.OnEvent, whose subscriptions hash
// and would diverge an audio-on game from an audio-off one). Render resolves each
// render event to an AudioCue (kind→category, entity→unit-type+position) and hands
// it here. The SoundTrigger maps it through the data-driven unit sound-set table
// (#313 data layer) into an AudioEvent and routes it to the audio Manager, where
// the 32-voice budget + admission control (#230) and the 3D/2D domains (#231)
// apply. It holds NO sim reference, so audio can never perturb the sim hash.

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
)

// AudioCue is one gameplay moment to sonify, already resolved render-side from a
// sim RenderEvent: which category, which unit type (for the sound-set lookup),
// where (for 3D world cues), the unit's stable key (for throttling), and the tick.
type AudioCue struct {
	Category audio.SoundCategory
	UnitType string
	Unit     uint32 // stable per-unit key for throttle windows; 0 = none
	Pos      api.Vec2
	Tick     uint32
}

// TriggerOutcome reports what the trigger did with a cue (the observable SoT for
// FSV — distinct from the Manager's downstream admission outcome).
type TriggerOutcome uint8

const (
	CueRouted    TriggerOutcome = iota // handed to the Manager (then subject to admission)
	CueThrottled                       // suppressed by a per-category throttle window
	CueNoSet                           // no sound set for this unit type — silently skipped
)

func (o TriggerOutcome) String() string {
	switch o {
	case CueRouted:
		return "routed"
	case CueThrottled:
		return "throttled"
	case CueNoSet:
		return "no-set"
	}
	return "invalid"
}

// SoundTrigger maps gameplay-moment cues to admitted voices through the audio
// Manager, with per-category throttles. Render-side; holds no sim references.
type SoundTrigger struct {
	sets          *audio.SoundSetTable
	mgr           *audio.Manager
	underAtkTicks uint32
	lastUnderAtk  map[uint32]uint32 // unit -> tick of its last under-attack stinger
}

// NewSoundTrigger builds a trigger over the given sound-set table, routing to mgr.
// underAttackThrottleTicks is the minimum gap between under-attack stingers for one
// unit (the anti-machine-gun warning guard); 0 disables that throttle.
func NewSoundTrigger(mgr *audio.Manager, sets *audio.SoundSetTable, underAttackThrottleTicks uint32) *SoundTrigger {
	return &SoundTrigger{
		sets:          sets,
		mgr:           mgr,
		underAtkTicks: underAttackThrottleTicks,
		lastUnderAtk:  make(map[uint32]uint32),
	}
}

// worldCategory reports whether a category routes to the 3D world domain
// (positional, budgeted) rather than the 2D UI domain.
func worldCategory(c audio.SoundCategory) bool {
	switch c {
	case audio.CatAttack, audio.CatDeath, audio.CatUnderAttack:
		return true
	default: // CatReady, CatAck, CatOrderAck
		return false
	}
}

// Fire routes one cue. Under-attack stingers are throttled to at most one per
// underAtkTicks per unit. Returns what the trigger did with the cue.
func (t *SoundTrigger) Fire(c AudioCue) TriggerOutcome {
	set, ok := t.sets.Lookup(c.UnitType)
	if !ok {
		return CueNoSet
	}
	if c.Category == audio.CatUnderAttack && t.underAtkTicks > 0 && c.Unit != 0 {
		if last, seen := t.lastUnderAtk[c.Unit]; seen && c.Tick-last < t.underAtkTicks {
			return CueThrottled
		}
		t.lastUnderAtk[c.Unit] = c.Tick
	}

	ev := api.AudioEvent{Cue: api.CueID(set.Cue(c.Category)), Volume: 1}
	if worldCategory(c.Category) {
		ev.Kind = api.AudioPlayAt
		ev.HasPos = true
		ev.Pos = c.Pos
		ev.Channel = api.ChannelEffects
	} else {
		ev.Kind = api.AudioPlay
		ev.Channel = api.ChannelUI
	}
	t.mgr.Handle(ev)
	return CueRouted
}

// SoundDriver bridges the api render-event stream to the SoundTrigger: it maps each
// render event's unit TYPE (resolved value) back to its sound-set code via a small
// map built once, then fires the trigger. Per frame: call Drain after Advance. This
// closes the #313 pipe — sim death cue → api render event → sound voice — entirely
// off the non-hashing render channel (#449).
type SoundDriver struct {
	trig        *SoundTrigger
	codeOf      map[api.UnitType]string
	localPlayer int // owner-filtered cues (order-ack, under-attack) play only for this player
	buf         []api.RenderEvent
}

// NewSoundDriver resolves every sound-set unit-type code to its api.UnitType once
// (the reverse of the render-event's resolved type), so Drain is allocation-free.
func NewSoundDriver(g *api.Game, trig *SoundTrigger, sets *audio.SoundSetTable, localPlayer int) *SoundDriver {
	m := make(map[api.UnitType]string, sets.Len())
	for _, code := range sets.Types() {
		if ut := g.UnitType(code); !ut.IsZero() {
			m[ut] = code
		}
	}
	return &SoundDriver{trig: trig, codeOf: m, localPlayer: localPlayer}
}

// renderCategory maps a render-event kind to its sound category.
func renderCategory(k api.RenderEventKind) (audio.SoundCategory, bool) {
	switch k {
	case api.RenderUnitDied:
		return audio.CatDeath, true
	case api.RenderUnitReady:
		return audio.CatReady, true
	case api.RenderUnitAttack:
		return audio.CatAttack, true
	case api.RenderUnitOrderAck:
		return audio.CatOrderAck, true
	case api.RenderUnitUnderAttack:
		return audio.CatUnderAttack, true
	}
	return 0, false
}

// ownerFiltered reports whether a category is a local-player-only cue (you hear
// your own units acknowledge / warn, not the enemy's). The sim is player-agnostic,
// so the local-player filter lives here, render-side.
func ownerFiltered(c audio.SoundCategory) bool {
	return c == audio.CatOrderAck || c == audio.CatUnderAttack
}

// Drain processes this tick's render events into sound cues and returns how many
// the trigger routed (the observable SoT for FSV).
func (d *SoundDriver) Drain(g *api.Game) int {
	d.buf = g.RenderEvents(d.buf)
	routed := 0
	for _, ev := range d.buf {
		cat, ok := renderCategory(ev.Kind)
		if !ok {
			continue
		}
		if ownerFiltered(cat) && ev.Owner != d.localPlayer {
			continue // another player's acknowledgement/warning — silent for us
		}
		code, ok := d.codeOf[ev.UnitType]
		if !ok {
			continue // no sound set for this unit type — silently skipped
		}
		if d.trig.Fire(AudioCue{Category: cat, UnitType: code, Unit: ev.UnitKey, Pos: ev.Pos, Tick: g.Tick()}) == CueRouted {
			routed++
		}
	}
	return routed
}
