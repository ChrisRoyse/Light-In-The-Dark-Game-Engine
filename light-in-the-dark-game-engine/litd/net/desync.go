package net

// desync.go: state-hash piggyback + desync detection + diagnostic dump (#77,
// D-2026-06-11-26, R-FSV-2). Lockstep guarantees every peer's sim is bit-equal;
// this is the safety net that PROVES it each second and localizes any break.
//
// Each client piggybacks a HashReport — the 64-bit full state hash plus the
// per-system sub-hashes (movement, combat, …, in the sim's fixed registration
// order) — every Kth turn on the reliable stream. The star center (host/relay)
// collects all clients' reports for a turn and compares: equal full hashes →
// silent; a mismatch raises a DesyncEvent that names the tick, every client's
// hashes, and the FIRST system whose sub-hash split (bisection, determinism.md
// §forensics), and writes a per-client diagnostic dump to a deterministic path.
//
// A report that is missing/late from any client DEFERS the comparison — never a
// false desync. litd/net stays sim-free: the detector works on a shared
// system-name list + numeric reports; the sim supplies them (#68).

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
)

// HashReport is one client's piggybacked hash for a turn: the full top hash and
// the per-system sub-hashes in the sim's fixed registration order (parallel to
// the detector's system-name list).
type HashReport struct {
	Turn uint64
	Top  uint64
	Subs []uint64
}

// DesyncEvent is raised when clients' top hashes disagree for a turn.
type DesyncEvent struct {
	Turn            uint64            `json:"turn"`
	ReferenceClient uint8             `json:"referenceClient"`
	DivergingSystem string            `json:"divergingSystem"` // "" if not localizable
	Tops            map[uint8]uint64  `json:"tops"`            // client → top hash
	DumpPaths       map[uint8]string  `json:"dumpPaths"`       // client → dump file
}

// HashCadenceTurns returns K so the hash cadence is ≈ 1/s at the 20 Hz sim, given
// the session's turn length in ticks. 20 ticks/s ÷ turnLenTicks turns.
func HashCadenceTurns(turnLenTicks int) int {
	if turnLenTicks < 1 {
		turnLenTicks = 1
	}
	return (20 + turnLenTicks - 1) / turnLenTicks
}

// DesyncDetector lives at the star center. Not safe for concurrent use; drive it
// from the host turn loop.
type DesyncDetector struct {
	systems []string
	clients []uint8
	dumpDir string
	pending map[uint64]map[uint8]HashReport
	comps   int
}

// NewDesyncDetector builds a detector for the given system-name list (the sim's
// registration order, shared by all peers) and expected client ids. dumpDir is
// where divergence dumps are written (created on demand).
func NewDesyncDetector(systems []string, clients []uint8, dumpDir string) (*DesyncDetector, error) {
	if len(systems) == 0 {
		return nil, fmt.Errorf("net: desync detector needs a non-empty system list")
	}
	if len(clients) == 0 {
		return nil, fmt.Errorf("net: desync detector needs at least one client")
	}
	cs := append([]uint8(nil), clients...)
	sort.Slice(cs, func(i, j int) bool { return cs[i] < cs[j] })
	return &DesyncDetector{
		systems: append([]string(nil), systems...),
		clients: cs,
		dumpDir: dumpDir,
		pending: make(map[uint64]map[uint8]HashReport),
	}, nil
}

// Comparisons is the count of turns compared with all clients agreeing (for the
// no-false-positive audit).
func (d *DesyncDetector) Comparisons() int { return d.comps }

// PendingTurns lists turns awaiting reports from some clients (deferred
// comparisons), ascending — for inspecting the late-report case.
func (d *DesyncDetector) PendingTurns() []uint64 {
	out := make([]uint64, 0, len(d.pending))
	for t := range d.pending {
		out = append(out, t)
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

// Report records client's hash for r.Turn. When every expected client has
// reported for that turn, it compares: on agreement it increments Comparisons
// and returns (nil, nil); on a top-hash split it returns a DesyncEvent (and has
// written per-client dump files). A duplicate or unknown client is a fail-closed
// error. Sub length must match the system list.
func (d *DesyncDetector) Report(client uint8, r HashReport) (*DesyncEvent, error) {
	if !d.knownClient(client) {
		return nil, fmt.Errorf("net: desync: unknown client %d", client)
	}
	if len(r.Subs) != len(d.systems) {
		return nil, fmt.Errorf("net: desync: client %d turn %d has %d sub-hashes, want %d", client, r.Turn, len(r.Subs), len(d.systems))
	}
	turn := r.Turn
	m := d.pending[turn]
	if m == nil {
		m = make(map[uint8]HashReport)
		d.pending[turn] = m
	}
	if _, dup := m[client]; dup {
		return nil, fmt.Errorf("net: desync: client %d already reported turn %d", client, turn)
	}
	m[client] = r
	if len(m) < len(d.clients) {
		return nil, nil // deferred — not all clients in yet
	}

	// All reported — compare against the lowest-id reference.
	ref := d.clients[0]
	refRep := m[ref]
	diverged := false
	for _, c := range d.clients {
		if m[c].Top != refRep.Top {
			diverged = true
			break
		}
	}
	if !diverged {
		d.comps++
		delete(d.pending, turn)
		return nil, nil
	}

	ev := &DesyncEvent{
		Turn:            turn,
		ReferenceClient: ref,
		Tops:            make(map[uint8]uint64, len(d.clients)),
		DumpPaths:       make(map[uint8]string, len(d.clients)),
	}
	for _, c := range d.clients {
		ev.Tops[c] = m[c].Top
	}
	// Localize: first system whose sub-hash differs from the reference, in any
	// diverging client.
	ev.DivergingSystem = d.firstDivergingSystem(refRep, m)
	// Diagnostic dumps for every client.
	for _, c := range d.clients {
		path, err := d.writeDump(turn, c, m[c])
		if err != nil {
			return nil, fmt.Errorf("net: desync: writing dump for client %d: %w", c, err)
		}
		ev.DumpPaths[c] = path
	}
	delete(d.pending, turn)
	return ev, nil
}

func (d *DesyncDetector) knownClient(c uint8) bool {
	for _, k := range d.clients {
		if k == c {
			return true
		}
	}
	return false
}

// firstDivergingSystem returns the name of the first system (registration order)
// whose sub-hash differs between the reference and any client. "" if none (e.g.
// the top split came from a system list disagreement we cannot localize).
func (d *DesyncDetector) firstDivergingSystem(ref HashReport, all map[uint8]HashReport) string {
	for i := range d.systems {
		for _, c := range d.clients {
			if all[c].Subs[i] != ref.Subs[i] {
				return d.systems[i]
			}
		}
	}
	return ""
}

type dumpSystem struct {
	Name string `json:"name"`
	Hash uint64 `json:"hash"`
}

type dumpFile struct {
	Turn    uint64       `json:"turn"`
	Client  uint8        `json:"client"`
	Top     uint64       `json:"top"`
	Systems []dumpSystem `json:"systems"`
}

// writeDump writes one client's full sub-hash table for turn to a deterministic
// path and returns it.
func (d *DesyncDetector) writeDump(turn uint64, client uint8, r HashReport) (string, error) {
	if err := os.MkdirAll(d.dumpDir, 0o755); err != nil {
		return "", err
	}
	df := dumpFile{Turn: turn, Client: client, Top: r.Top, Systems: make([]dumpSystem, len(d.systems))}
	for i, name := range d.systems {
		df.Systems[i] = dumpSystem{Name: name, Hash: r.Subs[i]}
	}
	blob, err := json.MarshalIndent(df, "", "  ")
	if err != nil {
		return "", err
	}
	path := filepath.Join(d.dumpDir, fmt.Sprintf("desync-tick%d-client%d.json", turn, client))
	if err := os.WriteFile(path, blob, 0o644); err != nil {
		return "", err
	}
	return path, nil
}
