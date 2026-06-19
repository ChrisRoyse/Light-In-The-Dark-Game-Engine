package hub

import (
	"archive/zip"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

// packArchive writes a minimal-but-VALID .litdworld at out: a zip carrying a
// manifest (with the given metadata + correct per-file and aggregate hashes) and
// one tiny lua payload, so worldarchive.Open verifies it end to end. Mirrors the
// worldarchive test's packDir; kept local because worldpack is package main.
func packArchive(t *testing.T, out, engineRange, title, author string) {
	t.Helper()
	type ent struct {
		rel, hash string
		size      int64
		body      []byte
	}
	files := map[string][]byte{
		"scripts/main.lua": []byte(fmt.Sprintf("-- %s by %s\nlocal x = 1\n", title, author)),
	}
	var ents []ent
	for rel, body := range files {
		sum := sha256.Sum256(body)
		ents = append(ents, ent{rel, hex.EncodeToString(sum[:]), int64(len(body)), body})
	}
	sort.Slice(ents, func(i, j int) bool { return ents[i].rel < ents[j].rel })

	agg := sha256.New()
	for _, e := range ents {
		agg.Write([]byte(e.hash + "\n"))
	}
	var man strings.Builder
	man.WriteString("litdworld-version: 1\n")
	fmt.Fprintf(&man, "engine-range: %s\n", engineRange)
	fmt.Fprintf(&man, "author: %s\n", author)
	fmt.Fprintf(&man, "title: %s\n", title)
	man.WriteString("description: test world\n")
	fmt.Fprintf(&man, "aggregate-sha256: %s\n", hex.EncodeToString(agg.Sum(nil)))
	fmt.Fprintf(&man, "files: %d\n", len(ents))
	for _, e := range ents {
		fmt.Fprintf(&man, "%s %d %s\n", e.hash, e.size, e.rel)
	}

	f, err := os.Create(out)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	zw := zip.NewWriter(f)
	mw, _ := zw.Create(".litdworld-manifest")
	mw.Write([]byte(man.String()))
	for _, e := range ents {
		w, _ := zw.Create(e.rel)
		w.Write(e.body)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
}

func get(t *testing.T, ts *httptest.Server, path string) (*http.Response, []byte) {
	t.Helper()
	resp, err := http.Get(ts.URL + path)
	if err != nil {
		t.Fatalf("GET %s: %v", path, err)
	}
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	return resp, body
}

// TestHubServesVerifiedIndexAndDownloadsFSV is the #175 keystone. SoT = the served
// index JSON and the downloaded archive bytes. Two worlds are seeded; every index
// field is inspected, and each downloaded archive's sha256 is checked to equal the
// index hash it was listed under.
func TestHubServesVerifiedIndexAndDownloadsFSV(t *testing.T) {
	dir := t.TempDir()
	packArchive(t, filepath.Join(dir, "alpha.litdworld"), ">=0.1.0 <0.2.0", "Alpha World", "Ada")
	packArchive(t, filepath.Join(dir, "beta.litdworld"), ">=0.1.0 <0.3.0", "Beta World", "Bo")

	srv := NewServer(dir, "")
	if err := srv.Reindex(); err != nil {
		t.Fatalf("reindex: %v", err)
	}
	ts := httptest.NewServer(srv)
	defer ts.Close()

	resp, body := get(t, ts, "/index.json")
	if resp.StatusCode != 200 {
		t.Fatalf("index status %d", resp.StatusCode)
	}
	if ct := resp.Header.Get("Content-Type"); ct != "application/json" {
		t.Fatalf("index content-type %q", ct)
	}
	var idx Index
	if err := json.Unmarshal(body, &idx); err != nil {
		t.Fatalf("index not valid JSON: %v\n%s", err, body)
	}
	if idx.Version != IndexVersion {
		t.Fatalf("index version %d want %d", idx.Version, IndexVersion)
	}
	if len(idx.Worlds) != 2 {
		t.Fatalf("index lists %d worlds, want 2:\n%s", len(idx.Worlds), body)
	}

	// Inspect EVERY field of each entry against expectations + download integrity.
	titles := map[string]bool{}
	for _, e := range idx.Worlds {
		titles[e.Title] = true
		if !isHex(e.Hash) || len(e.Hash) != 64 {
			t.Errorf("%q: hash %q not a sha256", e.Title, e.Hash)
		}
		if e.EngineRange == "" || e.Author == "" || e.Description != "test world" {
			t.Errorf("%q: missing manifest fields: range=%q author=%q desc=%q", e.Title, e.EngineRange, e.Author, e.Description)
		}
		if e.SizeBytes <= 0 {
			t.Errorf("%q: size %d", e.Title, e.SizeBytes)
		}
		if e.URL != "/worlds/"+e.Hash+".litdworld" {
			t.Errorf("%q: url %q", e.Title, e.URL)
		}
		if e.PublishedAt == "" {
			t.Errorf("%q: empty published_at", e.Title)
		}

		// Download via the index's URL; its sha256 MUST equal the index hash.
		dResp, dBody := get(t, ts, e.URL)
		if dResp.StatusCode != 200 {
			t.Fatalf("download %s status %d", e.URL, dResp.StatusCode)
		}
		sum := sha256.Sum256(dBody)
		got := hex.EncodeToString(sum[:])
		if got != e.Hash {
			t.Fatalf("%q: downloaded sha256 %s != index hash %s", e.Title, got, e.Hash)
		}
		if int64(len(dBody)) != e.SizeBytes {
			t.Fatalf("%q: downloaded %d bytes, index says %d", e.Title, len(dBody), e.SizeBytes)
		}
		t.Logf("FSV #175: %q hash=%s size=%d range=%q author=%q published=%s — download sha256 MATCHES index",
			e.Title, e.Hash[:12]+"…", e.SizeBytes, e.EngineRange, e.Author, e.PublishedAt)
	}
	if !titles["Alpha World"] || !titles["Beta World"] {
		t.Fatalf("missing expected titles: %v", titles)
	}
}

// TestHubEmptyDirYieldsEmptyIndexFSV — edge (1): empty data dir → valid empty
// index (200, version set, worlds:[]), not a 500.
func TestHubEmptyDirYieldsEmptyIndexFSV(t *testing.T) {
	srv := NewServer(t.TempDir(), "")
	if err := srv.Reindex(); err != nil {
		t.Fatalf("reindex empty: %v", err)
	}
	ts := httptest.NewServer(srv)
	defer ts.Close()
	resp, body := get(t, ts, "/index.json")
	if resp.StatusCode != 200 {
		t.Fatalf("empty index status %d (want 200, not 500)", resp.StatusCode)
	}
	var idx Index
	if err := json.Unmarshal(body, &idx); err != nil {
		t.Fatalf("empty index not JSON: %v", err)
	}
	if idx.Version != IndexVersion || len(idx.Worlds) != 0 {
		t.Fatalf("empty index = %+v, want version %d & 0 worlds", idx, IndexVersion)
	}
	t.Logf("FSV #175 edge1: empty dir → 200 with %d worlds: %s", len(idx.Worlds), strings.TrimSpace(string(body)))
}

// TestHubReflectsArchiveAddedWhileServingFSV — edge (2): an archive added after
// the server is up appears on the NEXT index fetch, with a correct hash.
func TestHubReflectsArchiveAddedWhileServingFSV(t *testing.T) {
	dir := t.TempDir()
	packArchive(t, filepath.Join(dir, "first.litdworld"), ">=0.1.0 <0.2.0", "First", "Ada")
	srv := NewServer(dir, "")
	srv.Reindex()
	ts := httptest.NewServer(srv)
	defer ts.Close()

	_, before := get(t, ts, "/index.json")
	var idxBefore Index
	json.Unmarshal(before, &idxBefore)
	if len(idxBefore.Worlds) != 1 {
		t.Fatalf("before: %d worlds, want 1", len(idxBefore.Worlds))
	}

	// Add a second archive while serving.
	packArchive(t, filepath.Join(dir, "second.litdworld"), ">=0.1.0 <0.2.0", "Second", "Bo")

	_, after := get(t, ts, "/index.json")
	var idxAfter Index
	json.Unmarshal(after, &idxAfter)
	if len(idxAfter.Worlds) != 2 {
		t.Fatalf("after add: %d worlds, want 2:\n%s", len(idxAfter.Worlds), after)
	}
	// The new world is downloadable with a matching hash.
	for _, e := range idxAfter.Worlds {
		if e.Title == "Second" {
			dResp, dBody := get(t, ts, e.URL)
			if dResp.StatusCode != 200 {
				t.Fatalf("new world download status %d", dResp.StatusCode)
			}
			sum := sha256.Sum256(dBody)
			if hex.EncodeToString(sum[:]) != e.Hash {
				t.Fatal("new world hash mismatch")
			}
		}
	}
	t.Logf("FSV #175 edge2: index grew 1→%d after archive added while serving", len(idxAfter.Worlds))
}

// TestHubUnknownAndTraversalAreNotFoundFSV — edge (3) + security: a request for a
// nonexistent archive is 404 and the service stays up; a path-traversal attempt
// is refused (404), never escaping the data dir.
func TestHubUnknownAndTraversalAreNotFoundFSV(t *testing.T) {
	dir := t.TempDir()
	packArchive(t, filepath.Join(dir, "only.litdworld"), ">=0.1.0 <0.2.0", "Only", "Ada")
	srv := NewServer(dir, "")
	srv.Reindex()
	ts := httptest.NewServer(srv)
	defer ts.Close()

	// Unknown hash → 404.
	resp, _ := get(t, ts, "/worlds/"+strings.Repeat("a", 64)+".litdworld")
	if resp.StatusCode != 404 {
		t.Fatalf("unknown archive status %d, want 404", resp.StatusCode)
	}
	// Traversal attempts → 404 (rejected as non-hex), never serve outside dir.
	for _, p := range []string{"/worlds/../../etc/passwd", "/worlds/..%2f..%2fetc%2fpasswd", "/worlds/main.lua"} {
		resp, _ := get(t, ts, p)
		if resp.StatusCode != 404 {
			t.Fatalf("traversal %q status %d, want 404", p, resp.StatusCode)
		}
	}
	// Method other than GET/HEAD → 405.
	postResp, err := http.Post(ts.URL+"/index.json", "text/plain", strings.NewReader("x"))
	if err == nil {
		if postResp.StatusCode != http.StatusMethodNotAllowed {
			t.Fatalf("POST status %d, want 405", postResp.StatusCode)
		}
		postResp.Body.Close()
	}
	// Service still alive: index still served.
	alive, _ := get(t, ts, "/index.json")
	if alive.StatusCode != 200 {
		t.Fatalf("service died: index status %d after bad requests", alive.StatusCode)
	}
	t.Logf("FSV #175 edge3: unknown→404, traversal→404, bad method→405, service stayed alive")
}

// TestHubSkipsUnverifiableArchiveFSV — fail-closed: a corrupt archive in the dir
// is NOT indexed (and not served), while a valid one beside it still is.
func TestHubSkipsUnverifiableArchiveFSV(t *testing.T) {
	dir := t.TempDir()
	packArchive(t, filepath.Join(dir, "good.litdworld"), ">=0.1.0 <0.2.0", "Good", "Ada")
	// A bogus file with the archive extension that is not a valid zip/archive.
	if err := os.WriteFile(filepath.Join(dir, "bad.litdworld"), []byte("not a zip"), 0o644); err != nil {
		t.Fatal(err)
	}
	idx, byHash, skips, err := BuildIndex(dir, "")
	if err != nil {
		t.Fatalf("BuildIndex: %v", err)
	}
	if len(idx.Worlds) != 1 || idx.Worlds[0].Title != "Good" {
		t.Fatalf("expected only the good world indexed, got %+v", idx.Worlds)
	}
	if len(skips) != 1 || !strings.Contains(skips[0].Reason, "verify") {
		t.Fatalf("expected 1 verify-skip for bad.litdworld, got %+v", skips)
	}
	if len(byHash) != 1 {
		t.Fatalf("download map has %d entries, want 1 (bad archive not servable)", len(byHash))
	}
	t.Logf("FSV #175 fail-closed: corrupt archive skipped (%s), valid world still indexed", skips[0].Reason)
}
