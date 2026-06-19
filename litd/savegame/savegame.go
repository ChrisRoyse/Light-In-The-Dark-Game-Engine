// Package savegame is the mid-game save/load container (#204, decision D-9): it
// bundles a full game snapshot — the deterministic sim state (api.Game.SaveState)
// AND the suspended Lua scheduler state (luabind.SaveScripts: coroutines, timers,
// event subscriptions, D-25 persister) — into ONE versioned, integrity-checked
// file, so a match can be saved at any tick and resumed bit-identically (the D-28
// stackless-scheduler guarantee).
//
// It is the coordinator layer that sits above both subsystems (api cannot import
// luabind, luabind imports api; neither owns the other's blob). The save-file UX
// (pause-menu entry + hotkey, #204's other half) is render/match-flow gated and
// lives in the demo/UI; this package is the headless-verifiable wiring it calls.
//
// Fail-closed (R-FMT-2, doctrine §2.4/§2.5): Load verifies the magic, the format
// version, the world fingerprint, AND a CRC32 over the whole payload BEFORE it
// mutates the game or the Lua state. A truncated, version-mismatched, wrong-world,
// or single-byte-corrupted file is refused loudly with NO partial application.
package savegame

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"hash/crc32"
	"io"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

// magic tags a Light in the Dark save container.
var magic = [8]byte{'L', 'I', 'T', 'D', 'S', 'A', 'V', 'E'}

// FormatVersion is the container schema version. Load refuses any other version
// (no silent migration); bump it when the container layout changes.
const FormatVersion uint8 = 1

// Write serializes a full mid-game save to w: the sim state and the Lua scheduler
// state under one versioned, CRC-protected envelope. fingerprint is the world's
// identity tag (the same value Load must be given), so a save cannot be loaded
// against the wrong world.
func Write(w io.Writer, g *api.Game, L *lua.LState, reg *luabind.ChunkRegistry, fingerprint uint64) error {
	if g == nil || L == nil || reg == nil {
		return fmt.Errorf("savegame: Write requires non-nil game, LState, and registry")
	}
	var sim bytes.Buffer
	if err := g.SaveState(&sim, fingerprint); err != nil {
		return fmt.Errorf("savegame: sim state: %w", err)
	}
	var scripts bytes.Buffer
	if err := luabind.SaveScripts(L, reg, &scripts); err != nil {
		return fmt.Errorf("savegame: script state: %w", err)
	}

	// Build the payload (everything the CRC covers) in memory, then frame it.
	var payload bytes.Buffer
	payload.Write(magic[:])
	payload.WriteByte(FormatVersion)
	writeU64(&payload, fingerprint)
	writeU64(&payload, uint64(sim.Len()))
	payload.Write(sim.Bytes())
	writeU64(&payload, uint64(scripts.Len()))
	payload.Write(scripts.Bytes())

	crc := crc32.ChecksumIEEE(payload.Bytes())
	if _, err := w.Write(payload.Bytes()); err != nil {
		return fmt.Errorf("savegame: write payload: %w", err)
	}
	if err := binary.Write(w, binary.LittleEndian, crc); err != nil {
		return fmt.Errorf("savegame: write crc: %w", err)
	}
	return nil
}

// Load restores a save written by Write into g / L / reg. It validates the whole
// container (magic, version, fingerprint, CRC, framing) BEFORE applying anything,
// so a corrupt or mismatched file is refused with no partial state. The caller
// supplies a fresh game + LState + a registry already holding the world's chunks
// (LoadScripts resolves coroutine protos against reg, exactly as the world loader
// registers them).
func Load(r io.Reader, g *api.Game, L *lua.LState, reg *luabind.ChunkRegistry, fingerprint uint64) error {
	if g == nil || L == nil || reg == nil {
		return fmt.Errorf("savegame: Load requires non-nil game, LState, and registry")
	}
	all, err := io.ReadAll(r)
	if err != nil {
		return fmt.Errorf("savegame: read: %w", err)
	}
	// Minimum: magic(8)+version(1)+fp(8)+simLen(8)+scriptLen(8)+crc(4) = 37 bytes.
	if len(all) < 37 {
		return fmt.Errorf("savegame: file too short (%d bytes) — truncated or not a save", len(all))
	}
	body, want := all[:len(all)-4], binary.LittleEndian.Uint32(all[len(all)-4:])
	if got := crc32.ChecksumIEEE(body); got != want {
		return fmt.Errorf("savegame: CRC mismatch (got %08x want %08x) — file is corrupt, refusing", got, want)
	}
	p := body
	if !bytes.Equal(p[:8], magic[:]) {
		return fmt.Errorf("savegame: bad magic — not a Light in the Dark save")
	}
	if v := p[8]; v != FormatVersion {
		return fmt.Errorf("savegame: format version %d, this engine reads %d — refusing (no silent migration)", v, FormatVersion)
	}
	if fp := binary.LittleEndian.Uint64(p[9:17]); fp != fingerprint {
		return fmt.Errorf("savegame: world fingerprint %#x does not match this world %#x — wrong save", fp, fingerprint)
	}
	off := 17
	simBytes, off, err := readFramed(p, off, "sim")
	if err != nil {
		return err
	}
	scriptBytes, off, err := readFramed(p, off, "script")
	if err != nil {
		return err
	}
	if off != len(p) {
		return fmt.Errorf("savegame: %d trailing bytes after framed sections — malformed", len(p)-off)
	}

	// Validation passed; only now mutate. Sim first, then scripts (the scheduler's
	// suspension records reference sim handles the sim restore re-establishes).
	if err := g.LoadState(bytes.NewReader(simBytes), fingerprint); err != nil {
		return fmt.Errorf("savegame: restore sim: %w", err)
	}
	if err := luabind.LoadScripts(L, reg, bytes.NewReader(scriptBytes)); err != nil {
		return fmt.Errorf("savegame: restore scripts: %w", err)
	}
	return nil
}

// readFramed reads a u64 length prefix at off and returns the following slice.
func readFramed(p []byte, off int, what string) ([]byte, int, error) {
	if off+8 > len(p) {
		return nil, off, fmt.Errorf("savegame: truncated %s length prefix", what)
	}
	n := int(binary.LittleEndian.Uint64(p[off : off+8]))
	off += 8
	if n < 0 || off+n > len(p) {
		return nil, off, fmt.Errorf("savegame: %s section claims %d bytes, only %d remain", what, n, len(p)-off)
	}
	return p[off : off+n], off + n, nil
}

func writeU64(b *bytes.Buffer, v uint64) {
	var tmp [8]byte
	binary.LittleEndian.PutUint64(tmp[:], v)
	b.Write(tmp[:])
}
