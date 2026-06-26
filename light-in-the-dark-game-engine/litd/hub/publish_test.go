package hub

// FSV for the #176 publish intake — the world-sharing hard gate (D-20). SoT =
// the served dir contents + the reindexed /index.json before/after each upload,
// plus the rejection findings. Synthetic archives with known content exercise
// the happy path, the hash gate (a member byte that disagrees with the
// manifest), the sandbox gate (Lua reaching for os), and idempotent re-publish.

import (
	"archive/zip"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
)

// buildArchiveBytes builds a .litdworld in memory with one lua member of the
// given body. If corruptMember, the manifest records a WRONG content hash for
// the member (and a matching aggregate) so the per-member hash gate must reject
// it. Returns the archive bytes.
func buildArchiveBytes(t *testing.T, luaBody string, corruptMember bool) []byte {
	t.Helper()
	body := []byte(luaBody)
	realHash := sha256.Sum256(body)
	memHash := hex.EncodeToString(realHash[:])
	if corruptMember {
		memHash = strings.Repeat("0", 64) // disagrees with the actual content
	}
	rel := "scripts/main.lua"

	agg := sha256.New()
	agg.Write([]byte(memHash + "\n"))
	var man strings.Builder
	man.WriteString("litdworld-version: 1\n")
	man.WriteString("engine-range: >=0.1.0 <0.2.0\n")
	man.WriteString("author: Pat\n")
	man.WriteString("title: Publishable\n")
	man.WriteString("description: test world\n")
	fmt.Fprintf(&man, "aggregate-sha256: %s\n", hex.EncodeToString(agg.Sum(nil)))
	man.WriteString("files: 1\n")
	fmt.Fprintf(&man, "%s %d %s\n", memHash, len(body), rel)

	var buf strings.Builder
	zw := zip.NewWriter(&zwWriter{&buf})
	mw, _ := zw.Create(".litdworld-manifest")
	mw.Write([]byte(man.String()))
	w, _ := zw.Create(rel)
	w.Write(body)
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	return []byte(buf.String())
}

// zwWriter adapts strings.Builder to io.Writer for zip.NewWriter.
type zwWriter struct{ b *strings.Builder }

func (z *zwWriter) Write(p []byte) (int, error) { return z.b.Write(p) }

func countWorlds(t *testing.T, srv *Server) int {
	t.Helper()
	idx, _, _, err := BuildIndex(srv.dir, "", nil)
	if err != nil {
		t.Fatalf("BuildIndex: %v", err)
	}
	return len(idx.Worlds)
}

func dirParts(t *testing.T, dir string) int {
	t.Helper()
	ents, _ := os.ReadDir(dir)
	n := 0
	for _, e := range ents {
		if strings.HasPrefix(e.Name(), ".publish-") {
			n++
		}
	}
	return n
}

func TestPublishIntakeFSV(t *testing.T) {
	dir := t.TempDir()
	srv := NewServer(dir, "")

	clean := buildArchiveBytes(t, "-- clean world\nlocal x = 1\nprint('hi')\n", false)

	// BEFORE: empty served set.
	if n := countWorlds(t, srv); n != 0 {
		t.Fatalf("expected empty hub, got %d worlds", n)
	}
	t.Logf("FSV BEFORE: 0 worlds")

	// CLEAN PUBLISH: both gates pass → stored, appears in index.
	res, findings, err := srv.Publish(clean)
	if err != nil || findings != nil {
		t.Fatalf("clean publish should pass, got findings=%v err=%v", findings, err)
	}
	if !res.Stored {
		t.Fatal("clean publish should store a new world")
	}
	if n := countWorlds(t, srv); n != 1 {
		t.Fatalf("after clean publish expected 1 world, got %d", n)
	}
	t.Logf("FSV AFTER clean publish: stored %s, index has 1 world", res.Hash)

	// IDEMPOTENT: re-publish identical bytes → no-op, still ONE entry.
	res2, findings2, err := srv.Publish(clean)
	if err != nil || findings2 != nil {
		t.Fatalf("idempotent re-publish should pass, got findings=%v err=%v", findings2, err)
	}
	if res2.Stored || res2.Hash != res.Hash {
		t.Fatalf("re-publish must be an idempotent no-op, got %+v", res2)
	}
	if n := countWorlds(t, srv); n != 1 {
		t.Fatalf("idempotent re-publish must keep a single entry, got %d", n)
	}
	t.Logf("FSV idempotent: re-publish kept a single index entry (stored=false)")

	// EDGE 1 — hash-tampered member: rejected at the hash gate naming the mismatch.
	tampered := buildArchiveBytes(t, "-- clean world\nlocal x = 1\nprint('hi')\n", true)
	_, findings, err = srv.Publish(tampered)
	if err != nil {
		t.Fatalf("tampered publish should reject via findings, not error: %v", err)
	}
	if len(findings) == 0 || !strings.Contains(strings.Join(findings, "\n"), "hash") {
		t.Fatalf("tampered archive must be rejected naming a hash mismatch, got %v", findings)
	}
	t.Logf("FSV EDGE1 hash-tampered rejected: %v", findings)
	if n := countWorlds(t, srv); n != 1 {
		t.Fatalf("rejected upload must not change the served set, got %d", n)
	}
	if p := dirParts(t, dir); p != 0 {
		t.Fatalf("rejected upload left %d temp parts", p)
	}

	// EDGE 2 — Lua sandbox violation: correct hashes, but the chunk reaches for os.
	bad := buildArchiveBytes(t, "local os = require('os')\nos.execute('rm -rf /')\n", false)
	_, findings, err = srv.Publish(bad)
	if err != nil {
		t.Fatalf("sandbox-violating publish should reject via findings, not error: %v", err)
	}
	joined := strings.Join(findings, "\n")
	if len(findings) == 0 || !strings.Contains(joined, "main.lua") {
		t.Fatalf("sandbox violation must name the chunk, got %v", findings)
	}
	t.Logf("FSV EDGE2 sandbox-rejected: %v", findings)
	if n := countWorlds(t, srv); n != 1 {
		t.Fatalf("sandbox-rejected upload must not change the served set, got %d", n)
	}
	if p := dirParts(t, dir); p != 0 {
		t.Fatalf("sandbox-rejected upload left %d temp parts", p)
	}
}

// TestPublishEndpointGatedFSV verifies the HTTP surface: publish is FORBIDDEN
// unless AllowPublish is set, and when enabled it accepts clean and rejects
// sandbox-violating uploads with the right status codes.
func TestPublishEndpointGatedFSV(t *testing.T) {
	dir := t.TempDir()

	// Read-only by default: POST /publish is 403.
	ro := httptest.NewServer(NewServer(dir, ""))
	defer ro.Close()
	resp, err := http.Post(ro.URL+"/publish", "application/octet-stream", strings.NewReader("x"))
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	t.Logf("FSV read-only hub: POST /publish -> %d", resp.StatusCode)
	if resp.StatusCode != http.StatusForbidden {
		t.Fatalf("read-only hub must 403 publish, got %d", resp.StatusCode)
	}

	// Publish-enabled hub.
	pub := NewServer(dir, "")
	pub.AllowPublish = true
	ts := httptest.NewServer(pub)
	defer ts.Close()

	clean := buildArchiveBytes(t, "-- clean\nlocal x = 1\n", false)
	resp, err = http.Post(ts.URL+"/publish", "application/octet-stream", bytesReader(clean))
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("clean publish over HTTP must be 200, got %d", resp.StatusCode)
	}
	t.Logf("FSV enabled hub: clean POST /publish -> 200")

	bad := buildArchiveBytes(t, "io.open('/etc/passwd')\n", false)
	resp, err = http.Post(ts.URL+"/publish", "application/octet-stream", bytesReader(bad))
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusUnprocessableEntity {
		t.Fatalf("sandbox-violating publish must be 422, got %d", resp.StatusCode)
	}
	t.Logf("FSV enabled hub: sandbox-violating POST /publish -> 422")

	// Final SoT: exactly the one clean world is on disk.
	worlds, _ := worldsDirState(t, dir)
	if len(worlds) != 1 {
		t.Fatalf("served dir should hold exactly the one accepted world, got %v", worlds)
	}
}

func bytesReader(b []byte) *strings.Reader { return strings.NewReader(string(b)) }
