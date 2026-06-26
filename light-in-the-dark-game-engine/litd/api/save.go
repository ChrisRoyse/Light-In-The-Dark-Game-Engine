package litd

// Mid-game save/load seam (D-9, #204/#270). The deterministic sim core
// (litd/sim) owns the binary save format — full entity state plus the stackless
// scheduler's descriptive suspension records (timers, Go threads, Lua-coroutine
// waits). This exposes it on the public Game so a host (the app shell, or the
// Lua save layer in litd/luabind) can save/restore a match without reaching into
// sim internals, keeping the api the single public surface (R-API-1).
//
// Fail-closed (§2.4): a suspended GO script thread's live stack cannot be
// written to a save file (api/thread.go), so SaveState refuses while any is
// parked rather than silently dropping it. Lua coroutine waits do NOT trip this
// — their VM state is serializable and persists alongside the sim blob via
// litd/luabind (#270); they are descriptive records here, not Go stacks.

import (
	"fmt"
	"io"
)

// SaveState writes a deterministic snapshot of the match to out, tagged with
// fingerprint (the world/map identity the save is bound to; LoadState refuses a
// mismatch). It must be called between ticks (never mid-Advance). It fails
// closed if any Go script thread (Game.Run) is suspended, since a parked Go
// stack is not serializable; resolve those before saving, or use Lua coroutines.
// A nil game is an error.
func (g *Game) SaveState(out io.Writer, fingerprint uint64) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("litd: SaveState on a nil game")
	}
	if n := g.SuspendedThreadCount(); n > 0 {
		return fmt.Errorf("litd: cannot save with %d suspended Go thread(s) — a parked Go stack is not serializable; finish them or model the wait as a Lua coroutine", n)
	}
	return g.w.SaveState(out, fingerprint)
}

// LoadState restores a match previously written by SaveState, requiring
// fingerprint to match the saved tag (a mismatch — wrong/edited map — is a loud
// refusal, never a partial load). On any error the game is left unusable and the
// caller must discard it. A nil game is an error.
func (g *Game) LoadState(in io.Reader, fingerprint uint64) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("litd: LoadState on a nil game")
	}
	return g.w.LoadState(in, fingerprint)
}
