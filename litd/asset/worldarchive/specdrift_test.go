package worldarchive

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// #540 drift gate: bind the world-archive format spec (docs/specs/world-archive-v1.md,
// the public surface of #187) to what this package actually enforces, so the two
// cannot silently diverge. The spec's "Header fields" table marks some fields
// Required=yes; this test parses that list and asserts the implementation REJECTS
// an archive missing each one. If a future edit relaxes the impl (drops a required-
// field check) while the spec still promises it — or marks a new field required in
// the spec without the impl enforcing it — this gate goes red naming the field.
//
// SoT (two): the spec markdown table, and worldarchive.Open's accept/reject verdict
// on archives built to omit exactly one field. No mocks — real archives, real Open.

// specRequiredFields parses the "### Header fields" table and returns the
// single-token fields whose Required column begins with "yes". Combined rows
// (e.g. "players-min / players-max / ...") and "no" rows are skipped.
func specRequiredFields(t *testing.T) []string {
	t.Helper()
	specPath := filepath.Join("..", "..", "..", "docs", "specs", "world-archive-v1.md")
	b, err := os.ReadFile(specPath)
	if err != nil {
		t.Fatalf("read spec: %v", err)
	}
	body := string(b)
	start := strings.Index(body, "### Header fields")
	if start < 0 {
		t.Fatal("spec has no '### Header fields' section — did the spec layout change?")
	}
	body = body[start:]
	if end := strings.Index(body, "### Payload rows"); end >= 0 {
		body = body[:end]
	}
	// table row: | `field` | yes... | meaning |
	rowRe := regexp.MustCompile("^\\|\\s*`([^`]+)`\\s*\\|\\s*([^|]+)\\|")
	var required []string
	for _, line := range strings.Split(body, "\n") {
		m := rowRe.FindStringSubmatch(strings.TrimSpace(line))
		if m == nil {
			continue
		}
		field := strings.TrimSpace(m[1])
		req := strings.TrimSpace(m[2])
		if strings.Contains(field, "/") { // combined optional rows
			continue
		}
		if strings.HasPrefix(strings.ToLower(req), "yes") {
			required = append(required, field)
		}
	}
	return required
}

func TestWorldArchiveSpecMatchesEnforcementFSV(t *testing.T) {
	required := specRequiredFields(t)
	t.Logf("FSV #540: spec marks %d header fields Required=yes: %v", len(required), required)
	if len(required) < 5 {
		t.Fatalf("parsed only %d required fields — spec table parse likely broken", len(required))
	}

	stage := stageFirstFlame(t)
	const engineRange = ">=0.1.0 <0.2.0"

	// Control: the complete archive is accepted (so omission, not payload, is what
	// fails the variants below).
	full := filepath.Join(t.TempDir(), "full.litdworld")
	packDir(t, stage, full, engineRange, "")
	if _, err := Open(full, ""); err != nil {
		t.Fatalf("control: a complete archive must Open cleanly, got %v", err)
	}
	t.Log("FSV control: complete archive Opens cleanly")

	// Every spec-required field, when omitted, must make Open fail.
	for _, field := range required {
		out := filepath.Join(t.TempDir(), "omit_"+strings.ReplaceAll(field, "-", "_")+".litdworld")
		packDir(t, stage, out, engineRange, field)
		_, err := Open(out, "")
		t.Logf("FSV #540: omit %-18s → Open err=%v", field, err)
		if err == nil {
			t.Fatalf("DRIFT: spec marks %q Required=yes but worldarchive.Open accepts an archive missing it — "+
				"the spec over-promises a guarantee the impl does not enforce", field)
		}
	}
}

func TestWorldArchiveSpecVersionMatchesProducerFSV(t *testing.T) {
	// The spec declares litdworld-version 1; the producer (packDir, mirroring
	// worldpack) writes version 1. If the impl bumps to v2, this binds the spec to
	// follow (a v2 needs a -v2 spec or this gate fails).
	specPath := filepath.Join("..", "..", "..", "docs", "specs", "world-archive-v1.md")
	b, err := os.ReadFile(specPath)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), "litdworld-version: 1") && !strings.Contains(string(b), "`1` for this spec") {
		t.Fatal("spec no longer declares version 1 — reconcile with the producer/impl version")
	}

	stage := stageFirstFlame(t)
	out := filepath.Join(t.TempDir(), "v.litdworld")
	packDir(t, stage, out, ">=0.1.0 <0.2.0", "")
	arc, err := Open(out, "")
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	t.Logf("FSV #540: spec declares v1; producer/impl manifest version = %d", arc.Manifest.Version)
	if arc.Manifest.Version != 1 {
		t.Fatalf("producer wrote version %d but spec is world-archive-v1 — spec/impl drift", arc.Manifest.Version)
	}
}
