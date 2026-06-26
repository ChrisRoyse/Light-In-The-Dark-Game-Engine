package net

import (
	"fmt"
	"strings"
)

// Lobby is the pre-game session-bootstrap state model (#80): the slot list, ready
// states, and the lockstep start parameters that the lobby UI renders and that the
// start broadcast carries. It is deliberately separate from Host (which owns the
// live sessions during the match) and from the join guard in join.go (the
// buildHash+seed check that runs BEFORE a client ever reaches a slot): a join here
// is the slot-assignment + ready-tracking layer the UI drives, fed by host events.
//
// Pure, deterministic state — no IO, no goroutines, no map iteration — so the UI
// (and tests) read an authoritative snapshot. Fail-closed throughout: a full lobby
// refuses with a reason string, a started lobby rejects further mutation, and Start
// is impossible until every occupied non-host slot is ready.
//
// Slot 0 is always the host. The host's readiness is implied by initiating Start;
// it never ready-toggles. Start requires at least two occupied slots (a 1-player
// "session" is not multiplayer) and every other occupied slot marked ready.

// SlotState is the occupancy/readiness of one lobby slot.
type SlotState uint8

const (
	// SlotEmpty: no player; joinable.
	SlotEmpty SlotState = iota
	// SlotOccupied: a player is present but not yet ready.
	SlotOccupied
	// SlotReady: an occupied slot whose player has marked ready.
	SlotReady
)

func (s SlotState) String() string {
	switch s {
	case SlotEmpty:
		return "empty"
	case SlotOccupied:
		return "occupied"
	case SlotReady:
		return "ready"
	default:
		return "unknown"
	}
}

// LobbySlot is one row of the lobby.
type LobbySlot struct {
	State  SlotState `json:"state"`
	Name   string    `json:"name,omitempty"`
	IsHost bool      `json:"isHost"`
}

// StartParams are the lockstep session parameters fixed at Start and carried in the
// start broadcast: the shared seed, the per-turn tick span, and the initial input
// delay (turns of latency the lockstep gate buffers).
type StartParams struct {
	Seed       uint64 `json:"seed"`
	TurnLen    int    `json:"turnLen"`
	InputDelay int    `json:"inputDelay"`
}

// Lobby is the authoritative pre-game state.
type Lobby struct {
	slots   []LobbySlot
	params  StartParams
	started bool
}

const (
	minLobbyPlayers = 2
	maxLobbyPlayers = 8
)

// NewLobby builds a lobby for capacity total slots (2–8). Slot 0 is the host,
// occupied with hostName. params are validated (turn length 2–4, input delay >= 0).
func NewLobby(capacity int, hostName string, params StartParams) (*Lobby, error) {
	if capacity < minLobbyPlayers || capacity > maxLobbyPlayers {
		return nil, fmt.Errorf("net: lobby capacity %d out of [%d,%d]", capacity, minLobbyPlayers, maxLobbyPlayers)
	}
	if strings.TrimSpace(hostName) == "" {
		return nil, fmt.Errorf("net: lobby host name must be non-empty")
	}
	if params.TurnLen < minTurnLen || params.TurnLen > maxTurnLen {
		return nil, fmt.Errorf("net: lobby turn length %d out of [%d,%d]", params.TurnLen, minTurnLen, maxTurnLen)
	}
	if params.InputDelay < 0 {
		return nil, fmt.Errorf("net: lobby input delay %d must be >= 0", params.InputDelay)
	}
	slots := make([]LobbySlot, capacity)
	slots[HostPlayer] = LobbySlot{State: SlotOccupied, Name: hostName, IsHost: true}
	return &Lobby{slots: slots, params: params}, nil
}

// Capacity is the total slot count.
func (l *Lobby) Capacity() int { return len(l.slots) }

// Started reports whether Start has fired (the lobby is now read-only).
func (l *Lobby) Started() bool { return l.started }

// OccupiedCount counts non-empty slots (host included).
func (l *Lobby) OccupiedCount() int {
	n := 0
	for i := range l.slots {
		if l.slots[i].State != SlotEmpty {
			n++
		}
	}
	return n
}

// Join assigns name to the lowest empty slot and returns its index. It refuses
// (with a reason string the UI shows verbatim) a full or already-started lobby, or
// an empty name.
func (l *Lobby) Join(name string) (uint8, error) {
	if l.started {
		return 0, fmt.Errorf("session already started — no new players")
	}
	if strings.TrimSpace(name) == "" {
		return 0, fmt.Errorf("player name must be non-empty")
	}
	for i := range l.slots {
		if l.slots[i].State == SlotEmpty {
			l.slots[i] = LobbySlot{State: SlotOccupied, Name: name}
			return uint8(i), nil
		}
	}
	return 0, fmt.Errorf("lobby full (%d/%d)", l.OccupiedCount(), len(l.slots))
}

// Leave frees an occupied non-host slot. The host slot cannot leave (closing the
// lobby is a separate host action). A started lobby rejects departures.
func (l *Lobby) Leave(slot uint8) error {
	if err := l.validSlot(slot); err != nil {
		return err
	}
	if l.started {
		return fmt.Errorf("session already started — cannot leave slot %d", slot)
	}
	if l.slots[slot].IsHost {
		return fmt.Errorf("the host slot %d cannot leave its own lobby", slot)
	}
	if l.slots[slot].State == SlotEmpty {
		return fmt.Errorf("slot %d is already empty", slot)
	}
	l.slots[slot] = LobbySlot{State: SlotEmpty}
	return nil
}

// SetReady toggles the ready flag of an occupied non-host slot. The host does not
// ready-toggle (its readiness is implied by initiating Start). A started lobby
// rejects ready changes.
func (l *Lobby) SetReady(slot uint8, ready bool) error {
	if err := l.validSlot(slot); err != nil {
		return err
	}
	if l.started {
		return fmt.Errorf("session already started — cannot change ready on slot %d", slot)
	}
	if l.slots[slot].IsHost {
		return fmt.Errorf("the host slot %d does not ready-toggle (Start implies it)", slot)
	}
	if l.slots[slot].State == SlotEmpty {
		return fmt.Errorf("slot %d is empty — nothing to ready", slot)
	}
	if ready {
		l.slots[slot].State = SlotReady
	} else {
		l.slots[slot].State = SlotOccupied
	}
	return nil
}

// CanStart reports whether Start would succeed: at least two occupied slots and
// every occupied non-host slot ready.
func (l *Lobby) CanStart() bool {
	if l.started || l.OccupiedCount() < minLobbyPlayers {
		return false
	}
	for i := range l.slots {
		s := l.slots[i]
		if s.State == SlotEmpty || s.IsHost {
			continue
		}
		if s.State != SlotReady {
			return false
		}
	}
	return true
}

// Start fixes the session and returns the start parameters. Fail-closed: it errors
// (and starts nothing) unless CanStart holds. Idempotent rejection — a second Start
// errors rather than re-firing.
func (l *Lobby) Start() (StartParams, error) {
	if l.started {
		return StartParams{}, fmt.Errorf("session already started")
	}
	if l.OccupiedCount() < minLobbyPlayers {
		return StartParams{}, fmt.Errorf("need at least %d players to start (have %d)", minLobbyPlayers, l.OccupiedCount())
	}
	if !l.CanStart() {
		return StartParams{}, fmt.Errorf("cannot start: not all occupied slots are ready")
	}
	l.started = true
	return l.params, nil
}

// Slots returns a copy of the slot list (host at index 0).
func (l *Lobby) Slots() []LobbySlot {
	return append([]LobbySlot(nil), l.slots...)
}

// Params returns the configured start parameters (also returned by Start).
func (l *Lobby) Params() StartParams { return l.params }

func (l *Lobby) validSlot(slot uint8) error {
	if int(slot) >= len(l.slots) {
		return fmt.Errorf("slot %d out of range [0,%d)", slot, len(l.slots))
	}
	return nil
}
