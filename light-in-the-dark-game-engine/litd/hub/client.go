package hub

// Client is the world download + install side of the hub (#178). It fetches the
// served index, downloads a content-addressed archive, and installs it into a
// local worlds directory through TWO independent content-hash gates plus the
// engine-version gate, fail-closed at every step (D-2026-06-11-14, -20):
//
//	gate 1 (download): the downloaded file's sha256 must equal the index entry's
//	                   published hash, checked BEFORE anything touches the worlds
//	                   dir — tampered bytes are discarded with both hashes named.
//	version gate:      the engine must satisfy the world's engine-range
//	                   (litd/semver, #180) — refused with the range, never
//	                   installed-then-broken.
//	gate 2 (load):     worldarchive.Open re-verifies the manifest + every member
//	                   hash from scratch (independent of gate 1) before the file
//	                   is committed — a writer/index bug can't mask a bad member.
//
// Install is atomic: bytes land in a temp file and are renamed into place only
// after all gates pass, so a crash or a rejected download never leaves a partial
// or unverified archive in the worlds dir. A downloaded world is byte-identical
// to one placed by hand and runs in the same sandbox (D-20).

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/semver"
)

// maxArchiveBytes caps a single download so a hostile or broken hub cannot
// exhaust disk/memory streaming an unbounded body (defense in depth).
const maxArchiveBytes = 256 << 20 // 256 MiB

// Client downloads and installs worlds from one hub base URL into worldsDir.
// engineVersion is this build's semver; "" disables the version gate (dev only).
type Client struct {
	BaseURL       string
	WorldsDir     string
	EngineVersion string
	HTTP          *http.Client
}

// NewClient returns a download client. httpc may be nil (http.DefaultClient).
func NewClient(baseURL, worldsDir, engineVersion string, httpc *http.Client) *Client {
	if httpc == nil {
		httpc = http.DefaultClient
	}
	return &Client{BaseURL: baseURL, WorldsDir: worldsDir, EngineVersion: engineVersion, HTTP: httpc}
}

// FetchIndex GETs <base>/index.json and decodes it.
func (c *Client) FetchIndex(ctx context.Context) (Index, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.BaseURL+"/index.json", nil)
	if err != nil {
		return Index{}, err
	}
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return Index{}, fmt.Errorf("hub: fetch index: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return Index{}, fmt.Errorf("hub: fetch index: status %d", resp.StatusCode)
	}
	var idx Index
	if err := json.NewDecoder(io.LimitReader(resp.Body, maxArchiveBytes)).Decode(&idx); err != nil {
		return Index{}, fmt.Errorf("hub: decode index: %w", err)
	}
	return idx, nil
}

// Install downloads the archive named by entry and installs it into WorldsDir,
// returning the installed path. It fails closed (no file installed) on a
// download hash mismatch, an engine-version incompatibility, or a load-time
// verification failure — and never leaves a partial file behind.
func (c *Client) Install(ctx context.Context, entry Entry) (string, error) {
	if !isHex(entry.Hash) || entry.Hash == "" {
		return "", fmt.Errorf("hub: install: entry has no valid content hash")
	}
	// Version gate first, against the index's declared range — refuse early and
	// clearly, before spending a download (gate 2 re-checks the manifest range).
	if c.EngineVersion != "" && entry.EngineRange != "" {
		if !semver.Satisfies(c.EngineVersion, entry.EngineRange) {
			return "", fmt.Errorf("hub: install refused: engine %s does not satisfy world engine-range %q",
				c.EngineVersion, entry.EngineRange)
		}
	}

	url := c.BaseURL + entry.URL
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return "", err
	}
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return "", fmt.Errorf("hub: download %s: %w", entry.Hash, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("hub: download %s: status %d", entry.Hash, resp.StatusCode)
	}

	if err := os.MkdirAll(c.WorldsDir, 0o755); err != nil {
		return "", err
	}
	// Stream to a uniquely-named temp file in the SAME dir (so the final rename is
	// atomic on one filesystem). A crash leaves only this temp — never a
	// <hash>.litdworld the loader would pick up — and we remove it on any failure.
	tmp, err := os.CreateTemp(c.WorldsDir, ".dl-*.litdworld.part")
	if err != nil {
		return "", err
	}
	tmpPath := tmp.Name()
	committed := false
	defer func() {
		tmp.Close()
		if !committed {
			os.Remove(tmpPath) // no partial/unverified archive ever lingers
		}
	}()

	// Gate 1: hash the bytes as they stream; compare to the index entry BEFORE
	// the file is allowed near the worlds dir as a real world.
	h := sha256.New()
	if _, err := io.Copy(io.MultiWriter(tmp, h), io.LimitReader(resp.Body, maxArchiveBytes+1)); err != nil {
		return "", fmt.Errorf("hub: download %s: %w", entry.Hash, err)
	}
	if err := tmp.Sync(); err != nil {
		return "", err
	}
	got := hex.EncodeToString(h.Sum(nil))
	if got != entry.Hash {
		return "", fmt.Errorf("hub: download hash mismatch: index published %s, downloaded bytes hash %s — discarded, not installed",
			entry.Hash, got)
	}

	// Gate 2: independent end-to-end verification (manifest + every member hash +
	// engine-range satisfaction) through the real loader path, on the temp file,
	// before it is committed.
	arc, err := worldarchive.Open(tmpPath, c.EngineVersion)
	if err != nil {
		return "", fmt.Errorf("hub: install refused, archive failed load-time verification: %w", err)
	}
	arc.Close()

	dest := filepath.Join(c.WorldsDir, entry.Hash+archiveExt)
	if err := os.Rename(tmpPath, dest); err != nil {
		return "", fmt.Errorf("hub: install %s: %w", entry.Hash, err)
	}
	committed = true
	return dest, nil
}

// LocalWorlds lists the content hashes of installed archives in WorldsDir — the
// local world list a browser surfaces. Best-effort: an unreadable dir yields an
// empty list, not an error (nothing installed yet).
func (c *Client) LocalWorlds() []string {
	ents, err := os.ReadDir(c.WorldsDir)
	if err != nil {
		return nil
	}
	var out []string
	for _, e := range ents {
		if e.IsDir() || filepath.Ext(e.Name()) != archiveExt {
			continue
		}
		hash := e.Name()[:len(e.Name())-len(archiveExt)]
		if isHex(hash) {
			out = append(out, hash)
		}
	}
	return out
}
