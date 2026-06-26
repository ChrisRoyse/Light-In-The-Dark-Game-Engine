package ai

// AI scheduler domain (#272). One isolated, serializable cooperative scheduler
// per AI player, ticked in a dedicated sub-phase of tick phase 2 after the
// map-script domain (tick-and-scheduler.md §3.4). See context.go for the
// isolation boundary; this file is the host and the tick phase.

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// Domain owns every AI player's context. Contexts are kept dense and in fixed
// insertion order; Tick drains them in that order so the AI sub-phase is
// deterministic and part of the phase contract. The domain has no reference to
// map-script state, so it can be disabled or dropped wholesale without touching
// anything outside itself.
type Domain struct {
	ctxs []*Context
	// diag is where overrun diagnostics are written. Defaults to os.Stderr;
	// tests redirect it. The write happens only on the failure path, never at
	// steady state, so it costs no steady-state allocation.
	diag io.Writer

	// Last-tick watchdog state — the source of truth a caller (or test) reads
	// to see whether the AI domain stayed within its counted slice this tick.
	overrun       bool
	overrunPlayer int
	overrunBudget int
	overrunCount  int
}

// NewDomain returns an empty AI domain.
func NewDomain() *Domain { return &Domain{diag: os.Stderr} }

// SetDiagnostics redirects overrun diagnostics (default os.Stderr). Pass nil to
// silence them; the watchdog state (Overrun, OverrunPlayer) is set regardless.
func (d *Domain) SetDiagnostics(w io.Writer) { d.diag = w }

// AddPlayer creates an isolated context for player, wires it to view and cmd,
// installs ctrl, and returns the context. The controller's Install registers
// its decision loops and kicks off the first suspension. Panics on a duplicate
// player id — two contexts for one player would split that player's AI state.
func (d *Domain) AddPlayer(player int, view AIView, cmd AICommander, ctrl AIController) *Context {
	if c := d.find(player); c != nil {
		panic(fmt.Sprintf("ai: duplicate AddPlayer for player %d", player))
	}
	c := &Context{player: player, s: sched.New(), view: view, cmd: cmd}
	d.ctxs = append(d.ctxs, c)
	ctrl.Install(c)
	return c
}

// find returns the context for player, or nil. Linear scan over the dense
// context slice — AI players are few, and a linear scan avoids any map (and the
// map-iteration nondeterminism the sim forbids).
func (d *Domain) find(player int) *Context {
	for _, c := range d.ctxs {
		if c.player == player {
			return c
		}
	}
	return nil
}

// Context returns the context for player, or nil if there is none.
func (d *Domain) Context(player int) *Context { return d.find(player) }

// PlayerCount returns how many AI contexts the domain hosts.
func (d *Domain) PlayerCount() int { return len(d.ctxs) }

// Tick advances every enabled context's scheduler by one tick, in fixed player
// (insertion) order — the AI sub-phase of tick phase 2.
//
// budgetPerPlayer is the counted tick-slice for each player this tick: a
// resumption budget, measured by the per-continuation meter (counted work,
// never wall-clock — so the verdict is bit-identical on every machine). A
// player whose decision loops resume more than the budget trips the watchdog: a
// loud structured diagnostic is emitted and Overrun() reports the offending
// player, but the tick still completes — every due continuation has already
// run, every other player is still ticked, and the sim tick is not blocked.
// (A true mid-resume preempt is the per-instruction Lua quota of §3.5, deferred
// to the Lua execution surface; this Go-continuation domain reports the
// overrun rather than tearing a continuation in half.) A budget <= 0 disables
// the watchdog. Returns the total resumptions across all enabled contexts.
//
// At steady state (no overrun) Tick allocates nothing.
func (d *Domain) Tick(budgetPerPlayer int) int {
	d.overrun = false
	d.overrunPlayer = 0
	d.overrunBudget = 0
	d.overrunCount = 0
	total := 0
	for _, c := range d.ctxs {
		if c.disabled {
			continue
		}
		c.resumes = 0
		c.s.Step()
		total += c.resumes
		if budgetPerPlayer > 0 && c.resumes > budgetPerPlayer && !d.overrun {
			d.overrun = true
			d.overrunPlayer = c.player
			d.overrunBudget = budgetPerPlayer
			d.overrunCount = c.resumes
			if d.diag != nil {
				fmt.Fprintf(d.diag,
					"ai: WATCHDOG tick=%d player=%d resumed %d continuations > budget %d "+
						"(sim tick completes; AI slice overrun — investigate the player's decision loop)\n",
					c.s.Now(), c.player, c.resumes, budgetPerPlayer)
			}
		}
	}
	return total
}

// Overrun reports whether the last Tick tripped the slice watchdog for any
// player. The fields below detail it; this is the source of truth a caller
// reads to react to a runaway AI.
func (d *Domain) Overrun() bool { return d.overrun }

// OverrunPlayer returns the first player that overran its slice last Tick (only
// meaningful when Overrun() is true).
func (d *Domain) OverrunPlayer() int { return d.overrunPlayer }

// OverrunDetail returns the offending player's resume count and the budget it
// exceeded last Tick (only meaningful when Overrun() is true).
func (d *Domain) OverrunDetail() (resumes, budget int) { return d.overrunCount, d.overrunBudget }

// Enable re-enables ticking for player. No-op if already enabled or unknown.
func (d *Domain) Enable(player int) {
	if c := d.find(player); c != nil {
		c.disabled = false
	}
}

// Disable stops ticking player's context without destroying its state — its
// suspensions stay frozen and still serialize. Because the domain holds no
// hooks into map-script state, disabling an AI player cannot perturb anything
// outside its own context. No-op if unknown.
func (d *Domain) Disable(player int) {
	if c := d.find(player); c != nil {
		c.disabled = true
	}
}

// Enabled reports whether player's context is currently ticked.
func (d *Domain) Enabled(player int) bool {
	if c := d.find(player); c != nil {
		return !c.disabled
	}
	return false
}

// --- serialization (R-SIM-6) ----------------------------------------------
//
// Domain save format v1, all integers little-endian:
//
//	[8]  magic "LITDAIDM"
//	[2]  version 1
//	[4]  playerCount
//	     playerCount × {
//	       [4] player
//	       [1] enabled (0/1)
//	       [4] schedBlobLen
//	       [schedBlobLen] scheduler blob (sched.Save — canonical)
//	     } in fixed (insertion) order
//
// The continuation registry is code, not state: a blob references continuations
// by stable ContID only, and Load requires the live domain to already hold the
// same contexts (same players, controllers re-installed) before restoring into
// them — exactly the sched.Load contract, lifted to the domain.

var aiSaveMagic = [8]byte{'L', 'I', 'T', 'D', 'A', 'I', 'D', 'M'}

const aiSaveVersion uint16 = 1

// Save appends the canonical encoding of every context to dst and returns it.
// Deterministic: contexts are emitted in fixed insertion order; each scheduler
// blob is itself canonical (sched.Save).
func (d *Domain) Save(dst []byte) []byte {
	dst = append(dst, aiSaveMagic[:]...)
	dst = binary.LittleEndian.AppendUint16(dst, aiSaveVersion)
	dst = binary.LittleEndian.AppendUint32(dst, uint32(len(d.ctxs)))
	for _, c := range d.ctxs {
		dst = binary.LittleEndian.AppendUint32(dst, uint32(c.player))
		if c.disabled {
			dst = append(dst, 0)
		} else {
			dst = append(dst, 1)
		}
		blob := c.s.Save(nil)
		dst = binary.LittleEndian.AppendUint32(dst, uint32(len(blob)))
		dst = append(dst, blob...)
	}
	return dst
}

var (
	errAIMagic   = errors.New("ai: bad domain save magic")
	errAIVersion = errors.New("ai: unsupported domain save version")
)

// Load restores every context's scheduler from blob. The domain must already
// hold exactly the same set of players (controllers re-installed so the ContIDs
// resolve) — Load matches each saved record to a live context by player id and
// rejects any mismatch. It is atomic: every live context is snapshotted first,
// the whole blob is validated and applied, and on any error all contexts are
// rolled back to their snapshots, so a corrupt or partial blob leaves the
// domain exactly as it was (fail-closed, zero partial application).
func (d *Domain) Load(blob []byte) error {
	if len(blob) < len(aiSaveMagic)+2+4 {
		return fmt.Errorf("ai: blob too short (%d bytes)", len(blob))
	}
	for i := range aiSaveMagic {
		if blob[i] != aiSaveMagic[i] {
			return errAIMagic
		}
	}
	off := 8
	if v := binary.LittleEndian.Uint16(blob[off:]); v != aiSaveVersion {
		return fmt.Errorf("%w: %d (want %d)", errAIVersion, v, aiSaveVersion)
	}
	off += 2
	count := int(binary.LittleEndian.Uint32(blob[off:]))
	off += 4
	if count != len(d.ctxs) {
		return fmt.Errorf("ai: save has %d players but domain hosts %d", count, len(d.ctxs))
	}

	// Parse the whole blob into locals first — no mutation until everything
	// validates, so a bad blob never touches the live domain.
	type rec struct {
		ctx     *Context
		enabled bool
		blob    []byte
	}
	recs := make([]rec, 0, count)
	seen := make([]bool, len(d.ctxs)) // dup-player guard, index-aligned with d.ctxs
	for i := 0; i < count; i++ {
		if off+4+1+4 > len(blob) {
			return fmt.Errorf("ai: truncated player header at offset %d", off)
		}
		player := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		en := blob[off] != 0
		off++
		n := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		if n < 0 || off+n > len(blob) {
			return fmt.Errorf("ai: scheduler blob length %d for player %d exceeds remaining bytes", n, player)
		}
		idx := -1
		for j, c := range d.ctxs {
			if c.player == player {
				idx = j
				break
			}
		}
		if idx < 0 {
			return fmt.Errorf("ai: save references player %d with no live context", player)
		}
		if seen[idx] {
			return fmt.Errorf("ai: save references player %d twice", player)
		}
		seen[idx] = true
		recs = append(recs, rec{ctx: d.ctxs[idx], enabled: en, blob: blob[off : off+n]})
		off += n
	}
	if off != len(blob) {
		return fmt.Errorf("ai: %d trailing bytes after domain save data", len(blob)-off)
	}

	// Snapshot every context for rollback, then apply. sched.Load is itself
	// fail-closed per scheduler; the snapshot makes the *domain* fail-closed
	// across all of them.
	snaps := make([][]byte, len(d.ctxs))
	disabled := make([]bool, len(d.ctxs))
	for j, c := range d.ctxs {
		snaps[j] = c.s.Save(nil)
		disabled[j] = c.disabled
	}
	for _, r := range recs {
		if err := r.ctx.s.Load(r.blob); err != nil {
			// Roll everyone back to the pre-Load snapshot.
			for j, c := range d.ctxs {
				_ = c.s.Load(snaps[j])
				c.disabled = disabled[j]
			}
			return fmt.Errorf("ai: restoring player %d: %w", r.ctx.player, err)
		}
		r.ctx.disabled = !r.enabled
	}
	return nil
}
