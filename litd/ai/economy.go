package ai

import (
	"encoding/binary"
	"errors"
	"fmt"
)

// Harvest / build-order natives (#279; jass-mapping/ai-natives.md harvest/build
// family; execution-model.md §6). The common.ai economy family —
// HarvestGold/HarvestWood worker assignment and the SetBuildUnit/SetBuildAll
// build-order class — as typed harvest/build intents over a sim-authoritative
// control surface.
//
// As with training (#277) and waves (#278), the AI names WHAT, the sim chooses
// WHO/WHERE (R-EXEC-3): a harvest intent names a resource and a worker count,
// and the sim picks the concrete workers and nodes (entity-id order, nearest
// node); a build intent names a structure type, and the sim runs a deterministic
// placement search around the town center and picks the builder. The AI never
// supplies raw worker ids, node ids, or build coordinates, and never mutates a
// queue directly. A failed placement is a recorded no-op the AI observes.

// EconomyControl is the sim-authoritative economy surface the AI drives,
// satisfied by a sim adapter at the integration boundary (ints / integer world
// units here; EntityID / fixed-point in the sim).
type EconomyControl interface {
	// AssignHarvest assigns up to count workers to the resource, incrementally
	// (keeps those already on it), and returns how many were NEWLY assigned.
	AssignHarvest(player, resource, count int) int
	// HarvestersOn reports how many of player's workers currently gather resource.
	HarvestersOn(player, resource int) int
	// PlaceBuilding runs the sim's deterministic placement search around
	// (centerX, centerY) for typeID and issues the build; false when no site or
	// no idle builder was found (a recorded no-op).
	PlaceBuilding(player, typeID int, centerX, centerY int32) bool
}

// Economy is one player's harvest-intent surface over the sim control.
type Economy struct {
	ctrl   EconomyControl
	player int
}

// NewEconomy binds an economy intent surface for player.
func NewEconomy(ctrl EconomyControl, player int) Economy {
	return Economy{ctrl: ctrl, player: player}
}

// SetHarvest targets count workers on resource (the HarvestGold/HarvestWood
// analogue): the sim tops up toward count, assigning only the shortfall. Returns
// the number newly assigned this call (0 once already at/over the target — the
// reassignment is incremental, never churn-everything).
func (e Economy) SetHarvest(resource, count int) int {
	return e.ctrl.AssignHarvest(e.player, resource, count)
}

// HarvestersOn reports the current worker count on resource (the economy SoT).
func (e Economy) HarvestersOn(resource int) int { return e.ctrl.HarvestersOn(e.player, resource) }

// BuildItem is one entry of a build order: build Count structures of TypeID.
type BuildItem struct {
	TypeID int
	Count  int
}

// BuildOrder is the SetBuildUnit/SetBuildAll-class build-order sequencer: a
// priority list of structure quotas the AI works through in order, one placement
// attempt per Tick, retrying a failed placement next tick rather than dropping
// it (WC3 build-order semantics — the list is a goal, not a fire-and-forget
// queue). All running state is flat data, so it serializes directly.
type BuildOrder struct {
	ctrl    EconomyControl
	player  int
	cx, cy  int32
	items   []BuildItem
	issued  []int // per-item placements successfully issued so far
	attempts int  // total placement attempts (observable)
	fails   int   // total placement failures (observable)
}

// NewBuildOrder binds a build-order sequencer for player, placing around the
// town center (centerX, centerY). The items list is the priority order.
func NewBuildOrder(ctrl EconomyControl, player int, centerX, centerY int32, items []BuildItem) *BuildOrder {
	return &BuildOrder{
		ctrl:   ctrl,
		player: player,
		cx:     centerX,
		cy:     centerY,
		items:  append([]BuildItem(nil), items...),
		issued: make([]int, len(items)),
	}
}

// Tick attempts to advance the build order: it finds the first unfinished item
// in priority order and tries to place ONE structure of it (a single builder per
// tick — more free up over time). A successful placement increments that item's
// issued count; a failure is recorded and the SAME item is retried next tick (no
// advance, no drop). Returns true if a placement was issued this tick.
func (b *BuildOrder) Tick() bool {
	for i := range b.items {
		if b.issued[i] >= b.items[i].Count {
			continue // this item satisfied — move to the next priority
		}
		b.attempts++
		if b.ctrl.PlaceBuilding(b.player, b.items[i].TypeID, b.cx, b.cy) {
			b.issued[i]++
			return true
		}
		b.fails++
		return false // placement failed: retry this item next tick (build order holds)
	}
	return false // build order complete
}

// Complete reports whether every item has reached its target placement count.
func (b *BuildOrder) Complete() bool {
	for i := range b.items {
		if b.issued[i] < b.items[i].Count {
			return false
		}
	}
	return true
}

// Issued returns the placements issued so far for item index i.
func (b *BuildOrder) Issued(i int) int {
	if i < 0 || i >= len(b.issued) {
		return 0
	}
	return b.issued[i]
}

// Attempts / Failures expose the placement-attempt and failure counters (the
// retry behavior is observable).
func (b *BuildOrder) Attempts() int { return b.attempts }
func (b *BuildOrder) Failures() int { return b.fails }

// ---- serialization ----

var buildOrderMagic = [8]byte{'L', 'I', 'T', 'D', 'B', 'L', 'D', 'O'}

const buildOrderVersion uint16 = 1

var (
	errBuildOrderMagic   = errors.New("ai: bad build-order save magic")
	errBuildOrderVersion = errors.New("ai: unsupported build-order save version")
	errBuildOrderShape   = errors.New("ai: build-order save item count mismatch")
)

// Save serializes the running build-order state (issued counts + attempt/failure
// tallies + the priority list) so a build order resumes mid-sequence on load and
// reaches the identical completion ticks.
func (b *BuildOrder) Save(dst []byte) []byte {
	dst = append(dst, buildOrderMagic[:]...)
	dst = binary.LittleEndian.AppendUint16(dst, buildOrderVersion)
	dst = binary.LittleEndian.AppendUint32(dst, uint32(b.player))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(b.cx))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(b.cy))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(b.attempts))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(b.fails))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(len(b.items)))
	for i, it := range b.items {
		dst = binary.LittleEndian.AppendUint32(dst, uint32(it.TypeID))
		dst = binary.LittleEndian.AppendUint32(dst, uint32(it.Count))
		dst = binary.LittleEndian.AppendUint32(dst, uint32(b.issued[i]))
	}
	return dst
}

// Load restores build-order state from blob into an order with the SAME item
// list shape (the items themselves are not mutated). Fail-closed: validates the
// whole blob before committing, so a bad blob leaves the order untouched.
func (b *BuildOrder) Load(blob []byte) error {
	const header = 8 + 2 + 4*5 + 4
	if len(blob) < header {
		return fmt.Errorf("ai: build-order blob too short (%d bytes)", len(blob))
	}
	for i := range buildOrderMagic {
		if blob[i] != buildOrderMagic[i] {
			return errBuildOrderMagic
		}
	}
	off := 8
	if v := binary.LittleEndian.Uint16(blob[off:]); v != buildOrderVersion {
		return fmt.Errorf("%w: %d (want %d)", errBuildOrderVersion, v, buildOrderVersion)
	}
	off += 2
	player := int(int32(binary.LittleEndian.Uint32(blob[off:])))
	off += 4
	cx := int32(binary.LittleEndian.Uint32(blob[off:]))
	off += 4
	cy := int32(binary.LittleEndian.Uint32(blob[off:]))
	off += 4
	attempts := int(binary.LittleEndian.Uint32(blob[off:]))
	off += 4
	fails := int(binary.LittleEndian.Uint32(blob[off:]))
	off += 4
	n := int(binary.LittleEndian.Uint32(blob[off:]))
	off += 4
	if n != len(b.items) {
		return errBuildOrderShape
	}
	if off+n*12 > len(blob) {
		return fmt.Errorf("ai: build-order item table exceeds blob")
	}
	issued := make([]int, n)
	for i := 0; i < n; i++ {
		typeID := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		count := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		iss := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		if typeID != b.items[i].TypeID || count != b.items[i].Count {
			return fmt.Errorf("%w: item %d differs from live order", errBuildOrderShape, i)
		}
		issued[i] = iss
	}
	// validated — commit
	b.player = player
	b.cx, b.cy = cx, cy
	b.attempts, b.fails = attempts, fails
	b.issued = issued
	return nil
}
