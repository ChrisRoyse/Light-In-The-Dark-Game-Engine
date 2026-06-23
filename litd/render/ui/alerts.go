package ui

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// AlertFeed (#315): minimap pings + screen-edge alerts. It drains alert-class
// render events (under-attack, unit-ready) from the published snapshot into
// pings — a world point + kind + time-to-live that render on the minimap (#165)
// and as a world decal — and tracks the newest alert for the screen-edge
// banner. A player ping (alt-click) adds a manual ping. Clicking an alert or
// ping yields the camera-jump target. Fixed-capacity pool: when full the oldest
// ping is evicted (cosmetic degradation, explicitly NOT a sim rule). Reads fed
// world positions only — the caller resolves an event's entity to a point from
// the snapshot, so this widget never touches the sim.
const AlertPoolCap = 16

type PingKind uint8

const (
	PingPlayer      PingKind = iota // manual alt-click ping
	PingUnderAttack                 // RenderUnderAttack
	PingUnitReady                   // RenderUnitReady
)

// Ping is one live alert marker.
type Ping struct {
	X    float32  `json:"x"`
	Z    float32  `json:"z"`
	Kind PingKind `json:"kind"`
	TTL  uint16   `json:"ttl"` // remaining ms
}

type AlertFeed struct {
	DefaultTTL uint16

	pings   [AlertPoolCap]Ping
	count   int
	latest  int // index of newest non-player alert, -1 if none
	evicted int
}

func NewAlertFeed(defaultTTL uint16) AlertFeed {
	return AlertFeed{DefaultTTL: defaultTTL, latest: -1}
}

// Pings returns the live pings (valid until the next mutation).
func (a *AlertFeed) Pings() []Ping { return a.pings[:a.count] }

// Evicted reports how many pings have been dropped to make room (cosmetic).
func (a *AlertFeed) Evicted() int { return a.evicted }

// PlayerPing adds a manual alt-click ping at a world point.
func (a *AlertFeed) PlayerPing(x, z float32) {
	a.add(Ping{X: x, Z: z, Kind: PingPlayer, TTL: a.DefaultTTL})
}

// FromEvent maps an alert-class render event to a ping at the fed world point.
// Non-alert kinds are ignored (returns false). The caller has already resolved
// ev.Ent to (x,z) from the snapshot.
func (a *AlertFeed) FromEvent(ev sim.RenderEvent, x, z float32) bool {
	var k PingKind
	switch ev.Kind {
	case sim.RenderUnderAttack:
		k = PingUnderAttack
	case sim.RenderUnitReady:
		k = PingUnitReady
	default:
		return false
	}
	a.add(Ping{X: x, Z: z, Kind: k, TTL: a.DefaultTTL})
	a.latest = a.count - 1
	return true
}

// add appends a ping, evicting the oldest (shift-down) when the pool is full.
func (a *AlertFeed) add(p Ping) {
	if a.count < AlertPoolCap {
		a.pings[a.count] = p
		a.count++
		return
	}
	// Pool full: drop the oldest, shift the rest down, append at the end.
	copy(a.pings[:], a.pings[1:])
	a.pings[AlertPoolCap-1] = p
	a.evicted++
	if a.latest >= 0 {
		a.latest-- // the shift moved every index down one
	}
}

// Tick ages every ping by dtMS and removes expired ones (TTL underflow-safe).
func (a *AlertFeed) Tick(dtMS uint16) {
	n := 0
	newLatest := -1
	for i := 0; i < a.count; i++ {
		p := a.pings[i]
		if p.TTL <= dtMS {
			continue // expired
		}
		p.TTL -= dtMS
		a.pings[n] = p
		if i == a.latest {
			newLatest = n
		}
		n++
	}
	a.count = n
	a.latest = newLatest
}

// LatestAlert returns the world point of the newest non-player alert for the
// screen-edge banner / camera jump, or ok=false when none is live.
func (a *AlertFeed) LatestAlert() (x, z float32, ok bool) {
	if a.latest < 0 || a.latest >= a.count {
		return 0, 0, false
	}
	p := a.pings[a.latest]
	return p.X, p.Z, true
}

// ClickPing returns the camera-jump target for the ping at index i.
func (a *AlertFeed) ClickPing(i int) (x, z float32, ok bool) {
	if i < 0 || i >= a.count {
		return 0, 0, false
	}
	p := a.pings[i]
	return p.X, p.Z, true
}
