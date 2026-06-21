package main

// Tests for the EVENT_ coverage gate (#466). FSV against the manifest the
// generator produces from the REAL common.j (happy path), plus the four
// mandated fail-closed edges driven on synthetic fixtures with known inputs.

import (
	"os"
	"strings"
	"testing"
)

// realCommonJ loads the vendored common.j the generator reads in production.
func realCommonJ(t *testing.T) string {
	t.Helper()
	// tests run from the package dir; common.j lives two levels up (repo root).
	b, err := os.ReadFile("../../" + defaultScriptsDir + "/common.j")
	if err != nil {
		t.Fatalf("read common.j: %v", err)
	}
	return string(b)
}

// TestEventCovRealManifestFullyAccounted — happy path: against the real
// common.j, every EVENT_ constant resolves to a verdict with zero errors, the
// family counts are exact, and mapped+tombstoned == total.
func TestEventCovRealManifestFullyAccounted(t *testing.T) {
	m, errs := buildEventCoverage(realCommonJ(t))
	if len(errs) != 0 {
		t.Fatalf("expected 0 validation errors, got %d: %v", len(errs), errs)
	}
	if m.Total != 136 {
		t.Fatalf("total = %d, want 136", m.Total)
	}
	if m.Mapped+m.Tombstoned != m.Total {
		t.Fatalf("mapped(%d)+tombstoned(%d) != total(%d)", m.Mapped, m.Tombstoned, m.Total)
	}
	want := map[string]int{"gameevent": 16, "playerevent": 22, "playerunitevent": 47, "unitevent": 48, "widgetevent": 1, "dialogevent": 2}
	for fam, n := range want {
		if m.Families[fam] != n {
			t.Fatalf("family %s = %d, want %d", fam, m.Families[fam], n)
		}
	}
	t.Logf("FSV: total=%d mapped=%d tombstoned=%d families=%v", m.Total, m.Mapped, m.Tombstoned, m.Families)

	// every mapped row names a known litd EventKind; every tombstone has a reason.
	for _, r := range m.Events {
		switch r.Status {
		case "mapped":
			if !knownEventKinds[r.Kind] {
				t.Fatalf("%s mapped to unknown kind %q", r.Name, r.Kind)
			}
		case "tombstoned":
			if strings.TrimSpace(r.Reason) == "" {
				t.Fatalf("%s tombstoned with empty reason", r.Name)
			}
		default:
			t.Fatalf("%s has bad status %q", r.Name, r.Status)
		}
	}
}

// fixtureSrc is a minimal synthetic common.j slice with three real-shaped
// EVENT_ declarations across families.
const fixtureSrc = `
    constant unitevent       EVENT_UNIT_DEATH   = ConvertUnitEvent(53)
    constant playerevent     EVENT_PLAYER_CHAT  = ConvertPlayerEvent(20)
    constant gameevent       EVENT_GAME_VICTORY = ConvertGameEvent(8)
`

// goodFixtureTable is a verdict table that fully accounts for fixtureSrc.
func goodFixtureTable() map[string]eventVerdict {
	return map[string]eventVerdict{
		"EVENT_UNIT_DEATH":   mapped("EventUnitDeath"),
		"EVENT_PLAYER_CHAT":  tombstone("UI input"),
		"EVENT_GAME_VICTORY": mapped("EventVictory"),
	}
}

// Edge 1: an EVENT_ present in source but absent from the table fails loudly.
func TestEventCovEdgeUnaccountedFails(t *testing.T) {
	src := fixtureSrc + "    constant unitevent EVENT_UNIT_FAKE = ConvertUnitEvent(999)\n"
	_, errs := buildEventCoverageWith(src, goodFixtureTable())
	if !hasErrContaining(errs, "EVENT_UNIT_FAKE") || !hasErrContaining(errs, "neither mapped nor tombstoned") {
		t.Fatalf("expected unaccounted-constant error for EVENT_UNIT_FAKE, got: %v", errs)
	}
	t.Logf("FSV edge1 (fake EVENT_): %v", errs)
}

// Edge 2: a tombstone with an empty reason fails loudly.
func TestEventCovEdgeTombstoneMissingReasonFails(t *testing.T) {
	tbl := goodFixtureTable()
	tbl["EVENT_PLAYER_CHAT"] = eventVerdict{} // neither kind nor reason
	_, errs := buildEventCoverageWith(fixtureSrc, tbl)
	if !hasErrContaining(errs, "EVENT_PLAYER_CHAT") || !hasErrContaining(errs, "empty verdict") {
		t.Fatalf("expected empty-verdict error for EVENT_PLAYER_CHAT, got: %v", errs)
	}
	t.Logf("FSV edge2 (tombstone missing reason): %v", errs)
}

// Edge 3: a mapped kind that is not a known litd EventKind fails loudly.
func TestEventCovEdgePhantomKindFails(t *testing.T) {
	tbl := goodFixtureTable()
	tbl["EVENT_UNIT_DEATH"] = mapped("EventDoesNotExist")
	_, errs := buildEventCoverageWith(fixtureSrc, tbl)
	if !hasErrContaining(errs, "EventDoesNotExist") || !hasErrContaining(errs, "not a known litd EventKind") {
		t.Fatalf("expected phantom-kind error, got: %v", errs)
	}
	t.Logf("FSV edge3 (phantom mapped kind): %v", errs)
}

// Edge 3b: a stale table entry (not in source) fails loudly.
func TestEventCovEdgeStaleEntryFails(t *testing.T) {
	tbl := goodFixtureTable()
	tbl["EVENT_UNIT_REMOVED_FROM_WC3"] = tombstone("gone")
	_, errs := buildEventCoverageWith(fixtureSrc, tbl)
	if !hasErrContaining(errs, "stale entry") {
		t.Fatalf("expected stale-entry error, got: %v", errs)
	}
	t.Logf("FSV edge3b (stale table entry): %v", errs)
}

// Edge 4: the manifest is byte-identical across two runs (deterministic).
func TestEventCovEdgeByteIdentical(t *testing.T) {
	src := realCommonJ(t)
	m1, e1 := buildEventCoverage(src)
	m2, e2 := buildEventCoverage(src)
	if len(e1)+len(e2) != 0 {
		t.Fatalf("unexpected errors: %v %v", e1, e2)
	}
	b1, b2 := marshalEventCov(m1), marshalEventCov(m2)
	if string(b1) != string(b2) {
		t.Fatal("manifest not byte-identical across runs")
	}
	t.Logf("FSV edge4 (deterministic): %d bytes, identical on re-run", len(b1))
}

func hasErrContaining(errs []error, sub string) bool {
	for _, e := range errs {
		if strings.Contains(e.Error(), sub) {
			return true
		}
	}
	return false
}
