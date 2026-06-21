package hub

// FSV for the download + install client (#178). SoT = the worlds-dir contents
// before/after each install + the verification error text. Synthetic archives
// with known content hashes are served from a real hub (happy path) or a
// hand-built tampering handler (the fail-closed edges); after each attempt the
// worlds dir is listed to prove what did — or did NOT — land.

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
)

// worldsDirState returns the installed world hashes + any leftover temp parts,
// so a test can assert exactly what is (and isn't) in the dir.
func worldsDirState(t *testing.T, dir string) (worlds []string, parts int) {
	t.Helper()
	ents, err := os.ReadDir(dir)
	if err != nil {
		return nil, 0
	}
	for _, e := range ents {
		name := e.Name()
		switch {
		case filepath.Ext(name) == archiveExt:
			worlds = append(worlds, name)
		case filepath.Ext(name) == ".part" || len(name) > 4 && name[:4] == ".dl-":
			parts++
		}
	}
	return worlds, parts
}

func TestClientDownloadInstallFSV(t *testing.T) {
	// A real hub over a dir holding one valid archive (engine-range >=0.1.0 <0.2.0).
	srvDir := t.TempDir()
	packArchive(t, filepath.Join(srvDir, "w.litdworld"), ">=0.1.0 <0.2.0", "Downloadable", "Dee")
	srv := NewServer(srvDir, "")
	if err := srv.Reindex(); err != nil {
		t.Fatalf("reindex: %v", err)
	}
	ts := httptest.NewServer(srv)
	defer ts.Close()

	worldsDir := t.TempDir()
	cli := NewClient(ts.URL, worldsDir, "0.1.5", ts.Client())

	idx, err := cli.FetchIndex(context.Background())
	if err != nil {
		t.Fatalf("FetchIndex: %v", err)
	}
	if len(idx.Worlds) != 1 {
		t.Fatalf("expected 1 world in index, got %d", len(idx.Worlds))
	}
	entry := idx.Worlds[0]

	// BEFORE: worlds dir empty.
	w, _ := worldsDirState(t, worldsDir)
	t.Logf("FSV BEFORE: worlds dir has %d archives", len(w))
	if len(w) != 0 {
		t.Fatalf("worlds dir should start empty, has %v", w)
	}

	// HAPPY PATH: install → file lands, both gates pass, it really loads.
	dest, err := cli.Install(context.Background(), entry)
	if err != nil {
		t.Fatalf("Install: %v", err)
	}
	w, parts := worldsDirState(t, worldsDir)
	t.Logf("FSV AFTER install: worlds=%v leftover-parts=%d dest=%s", w, parts, dest)
	if len(w) != 1 || w[0] != entry.Hash+archiveExt {
		t.Fatalf("expected installed %s, got %v", entry.Hash+archiveExt, w)
	}
	if parts != 0 {
		t.Fatalf("a temp part leaked into the worlds dir")
	}
	// The installed world genuinely verifies end-to-end (gate 2, independently).
	arc, err := worldarchive.Open(dest, "0.1.5")
	if err != nil {
		t.Fatalf("installed archive must load: %v", err)
	}
	arc.Close()
	if got := cli.LocalWorlds(); len(got) != 1 || got[0] != entry.Hash {
		t.Fatalf("LocalWorlds = %v, want [%s]", got, entry.Hash)
	}
	t.Logf("FSV: downloaded world installed + verifies + appears in local list")
}

func TestClientFailsClosedFSV(t *testing.T) {
	// Build a valid archive so we know its REAL file hash, then serve a TAMPERING
	// hub: a truthful index (publishing the real hash) but a download endpoint
	// that returns different bytes — the classic man-in-the-middle.
	srvDir := t.TempDir()
	archivePath := filepath.Join(srvDir, "w.litdworld")
	packArchive(t, archivePath, ">=0.1.0 <0.2.0", "Tamper Target", "Tess")
	realHash := fileHash(t, archivePath)
	url := "/worlds/" + realHash + archiveExt

	idxJSON, _ := json.Marshal(Index{Version: IndexVersion, Worlds: []Entry{{
		Hash: realHash, EngineRange: ">=0.1.0 <0.2.0", Title: "Tamper Target", URL: url,
	}}})

	mux := http.NewServeMux()
	mux.HandleFunc("/index.json", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Write(idxJSON)
	})
	mux.HandleFunc("/worlds/", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, "TAMPERED BYTES not the real archive") // wrong content -> wrong hash
	})
	ts := httptest.NewServer(mux)
	defer ts.Close()

	worldsDir := t.TempDir()
	cli := NewClient(ts.URL, worldsDir, "0.1.5", ts.Client())
	idx, err := cli.FetchIndex(context.Background())
	if err != nil {
		t.Fatalf("FetchIndex: %v", err)
	}
	entry := idx.Worlds[0]

	// EDGE 1 — tampered download: gate 1 mismatch, both hashes named, dir untouched.
	_, err = cli.Install(context.Background(), entry)
	if err == nil {
		t.Fatal("tampered download must be refused")
	}
	t.Logf("FSV EDGE1 tampered: %v", err)
	if !contains(err.Error(), "hash mismatch") || !contains(err.Error(), realHash) {
		t.Fatalf("error must name the mismatch + expected hash, got %v", err)
	}
	if w, parts := worldsDirState(t, worldsDir); len(w) != 0 || parts != 0 {
		t.Fatalf("tampered download must leave the worlds dir clean, got worlds=%v parts=%d", w, parts)
	}

	// EDGE 2 — version-incompatible: a client built as 0.5.0 cannot run a
	// >=0.1.0 <0.2.0 world; refused with the range, nothing installed.
	cliOld := NewClient(ts.URL, worldsDir, "0.5.0", ts.Client())
	_, err = cliOld.Install(context.Background(), entry)
	if err == nil {
		t.Fatal("version-incompatible install must be refused")
	}
	t.Logf("FSV EDGE2 version: %v", err)
	if !contains(err.Error(), "engine-range") || !contains(err.Error(), "0.5.0") {
		t.Fatalf("version refusal must name the engine + range, got %v", err)
	}
	if w, _ := worldsDirState(t, worldsDir); len(w) != 0 {
		t.Fatalf("version-incompatible refusal must install nothing, got %v", w)
	}

	// EDGE 3 — a serving error (404 from a real hub for an unknown hash) is
	// refused, dir clean (no partial).
	realSrv := NewServer(t.TempDir(), "")
	realSrv.Reindex()
	tsReal := httptest.NewServer(realSrv)
	defer tsReal.Close()
	cli404 := NewClient(tsReal.URL, worldsDir, "0.1.5", tsReal.Client())
	_, err = cli404.Install(context.Background(), entry) // entry not on this hub
	if err == nil {
		t.Fatal("download of an absent world must error")
	}
	t.Logf("FSV EDGE3 absent: %v", err)
	if w, parts := worldsDirState(t, worldsDir); len(w) != 0 || parts != 0 {
		t.Fatalf("absent download must leave dir clean, got worlds=%v parts=%d", w, parts)
	}
}

func contains(s, sub string) bool {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}
