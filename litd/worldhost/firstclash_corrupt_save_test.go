package worldhost_test

// FSV for corrupt-save loud refusal during an AI match (#653, ultimate-test-plan
// Phase 5; #204 fail-closed constraint). A save corrupted three ways is REFUSED
// with a reason that names what failed, and NO partial state is applied — the
// restore target's sim hash is unchanged across the failed load. SoT = the
// refusal error string + the target Game.StateHash() before vs after.

import (
	"bytes"
	"hash/crc32"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/savegame"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

func validFirstclashSave(t *testing.T) []byte {
	t.Helper()
	hs, err := worldhost.Load(firstclashDir, slSeed, 50_000_000)
	if err != nil {
		t.Fatalf("save load: %v", err)
	}
	defer hs.Close()
	hs.Game.Advance(3000)
	var buf bytes.Buffer
	if err := savegame.Write(&buf, hs.Game, hs.L, hs.Reg, fcFP); err != nil {
		t.Fatalf("Write: %v", err)
	}
	return buf.Bytes()
}

func TestFirstclashCorruptSaveRefusedFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("firstclash save + 3 corrupt-load attempts; full preflight gate")
	}
	good := validFirstclashSave(t)
	t.Logf("FSV valid container: %d bytes", len(good))

	cases := []struct {
		name    string
		mutate  func(b []byte) []byte
		wantErr string // substring the refusal must name
	}{
		{
			name:    "flipped CRC byte",
			mutate:  func(b []byte) []byte { c := append([]byte(nil), b...); c[len(c)-1] ^= 0xFF; return c },
			wantErr: "CRC",
		},
		{
			name: "bumped format version byte",
			// version is the single byte right after the 8-byte magic. Bump it so
			// the version check fires (the CRC also breaks, but version is checked
			// only after CRC — so to isolate version we must repair the CRC; here
			// we just assert the load is refused loudly, naming corruption).
			mutate:  func(b []byte) []byte { c := append([]byte(nil), b...); c[8] ^= 0x01; return c },
			wantErr: "", // any loud refusal; CRC fires first on a raw flip
		},
		{
			name:    "truncated file",
			mutate:  func(b []byte) []byte { return append([]byte(nil), b[:len(b)/2]...) },
			wantErr: "",
		},
	}

	for _, tc := range cases {
		corrupt := tc.mutate(good)

		// Fresh restore target; capture its sim hash BEFORE the failed load.
		hr, err := worldhost.Load(firstclashDir, slSeed, 50_000_000)
		if err != nil {
			t.Fatalf("%s: restore-target load: %v", tc.name, err)
		}
		before := hr.Game.StateHash()
		loadErr := savegame.Load(bytes.NewReader(corrupt), hr.Game, hr.L, hr.Reg, fcFP)
		after := hr.Game.StateHash()
		hr.Close()

		t.Logf("FSV %s: err=%v  simHash before=%#016x after=%#016x", tc.name, loadErr, before, after)
		if loadErr == nil {
			t.Fatalf("%s: corrupt save was ACCEPTED — must be refused (fail-closed)", tc.name)
		}
		if tc.wantErr != "" && !strings.Contains(loadErr.Error(), tc.wantErr) {
			t.Fatalf("%s: refusal %q does not name %q", tc.name, loadErr.Error(), tc.wantErr)
		}
		if after != before {
			t.Fatalf("%s: sim state changed across a FAILED load (%#016x -> %#016x) — partial state applied, not fail-closed", tc.name, before, after)
		}
	}

	// A repaired-CRC version bump must be refused by the VERSION check specifically
	// (proves validation is layered, not just CRC). Recompute the CRC over the
	// version-bumped body so CRC passes and the version mismatch is what fires.
	vb := append([]byte(nil), good...)
	vb[8] ^= 0x01 // bump version
	// savegame frames payload..crc(4); recompute crc over the new body.
	body := vb[:len(vb)-4]
	crc := crc32.ChecksumIEEE(body)
	vb[len(vb)-4] = byte(crc)
	vb[len(vb)-3] = byte(crc >> 8)
	vb[len(vb)-2] = byte(crc >> 16)
	vb[len(vb)-1] = byte(crc >> 24)

	hr, err := worldhost.Load(firstclashDir, slSeed, 50_000_000)
	if err != nil {
		t.Fatalf("version-case restore-target load: %v", err)
	}
	defer hr.Close()
	before := hr.Game.StateHash()
	loadErr := savegame.Load(bytes.NewReader(vb), hr.Game, hr.L, hr.Reg, fcFP)
	after := hr.Game.StateHash()
	t.Logf("FSV repaired-CRC version bump: err=%v before=%#016x after=%#016x", loadErr, before, after)
	if loadErr == nil || !strings.Contains(loadErr.Error(), "version") {
		t.Fatalf("version-bumped save must be refused naming the version: got %v", loadErr)
	}
	if after != before {
		t.Fatalf("version refusal applied partial state (%#016x -> %#016x)", before, after)
	}
	t.Log("FSV #653: corrupt saves (CRC / version / truncation) all refused loudly; sim untouched on every failed load")
}
