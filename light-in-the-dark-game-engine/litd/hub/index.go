// Package hub is the world-archive index + download service (#175, decisions
// D-2026-06-11-23 static-friendly index and -14 archives carry hosting metadata).
// It scans a directory of .litdworld archives, FULLY verifies each through the
// worldarchive read path, and produces a plain-JSON index any static host/CDN can
// serve — plus a zero-auth HTTP handler that serves the index and the archives.
//
// Separation of concerns: BuildIndex is pure (dir in, index + download map out) so
// it is exercised headlessly; Server wraps it with net/http. Fail-closed
// (doctrine §2.4): an archive that does not verify is NOT indexed and is reported
// loudly as a skip — never silently served, never aborting the whole index.
package hub

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
)

// Blocklist maps a taken-down archive's content hash (sha256 of the .litdworld
// file bytes — the same hash the index publishes) to its takedown reason
// category (#181, D-2026-06-11-23). A blocklisted hash is dropped from the index
// and its download returns HTTP 410; a re-upload of the same hash is refused at
// the next scan. A genuinely revised world has a different hash and is judged on
// its own. The reason category is the only takedown detail the public sees.
type Blocklist map[string]string

// Blocked reports whether hash is taken down and, if so, its reason category.
func (b Blocklist) Blocked(hash string) (string, bool) {
	if b == nil {
		return "", false
	}
	r, ok := b[hash]
	return r, ok
}

// LoadBlocklist parses a blocklist file: one entry per line, "<hexhash>[ reason
// words...]"; blank lines and lines beginning with '#' are ignored. An absent
// file is an empty blocklist (no takedowns yet), not an error — but a present
// file with a malformed hash fails CLOSED (a typo must not silently un-block a
// world). The reason defaults to "policy" when omitted.
func LoadBlocklist(path string) (Blocklist, error) {
	bl := Blocklist{}
	if path == "" {
		return bl, nil
	}
	f, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return bl, nil // no takedowns filed yet
		}
		return nil, fmt.Errorf("hub: open blocklist %q: %w", path, err)
	}
	defer f.Close()

	sc := bufio.NewScanner(f)
	for line := 1; sc.Scan(); line++ {
		text := strings.TrimSpace(sc.Text())
		if text == "" || strings.HasPrefix(text, "#") {
			continue
		}
		hash, reason, _ := strings.Cut(text, " ")
		hash = strings.ToLower(strings.TrimSpace(hash))
		reason = strings.TrimSpace(reason)
		if len(hash) != 64 || !isHex(hash) {
			return nil, fmt.Errorf("hub: blocklist %q line %d: %q is not a 64-char hex content hash", path, line, hash)
		}
		if reason == "" {
			reason = "policy"
		}
		bl[hash] = reason
	}
	if err := sc.Err(); err != nil {
		return nil, fmt.Errorf("hub: read blocklist %q: %w", path, err)
	}
	return bl, nil
}

// IndexVersion pins the index schema (see docs/hub/index-format.md).
const IndexVersion = 1

// archiveExt is the world-archive file extension the hub indexes.
const archiveExt = ".litdworld"

// Entry is one indexed world. Hash is the SHA-256 of the archive FILE bytes (the
// download-integrity hash a client checks after GET), distinct from the archive's
// internal aggregate fingerprint. URL is the content-addressed download path.
type Entry struct {
	Hash        string `json:"hash"`         // sha256 of the .litdworld file bytes
	EngineRange string `json:"engine_range"` // from the verified manifest
	Title       string `json:"title"`
	Author      string `json:"author"`
	Description string `json:"description"`
	SizeBytes   int64  `json:"size_bytes"`
	URL         string `json:"url"`          // /worlds/<hash>.litdworld
	PublishedAt string `json:"published_at"` // RFC3339, archive file mtime
}

// Index is the served table of contents. Worlds is sorted by Hash for a
// deterministic, diff-stable index a static host can cache.
type Index struct {
	Version int     `json:"version"`
	Worlds  []Entry `json:"worlds"`
}

// Skip records an archive that was present but not indexed, with the reason —
// surfaced (logged) rather than silently dropped (§1.3).
type Skip struct {
	Path   string
	Reason string
}

// BuildIndex scans dir for *.litdworld archives, verifies each through
// worldarchive.Open, and returns the index, a hash->absolute-path map for
// serving downloads, and the list of archives skipped because they failed
// verification. err is non-nil only for a failure to read dir itself (an empty
// or absent dir yields a valid empty index, not an error). engineVersion is
// passed to the verifier; "" indexes every well-formed archive regardless of
// engine compatibility (clients filter by engine_range). Archives whose file
// hash is in blocklist are excluded from the index and reported as a
// "blocklisted" skip — the index-side half of the takedown (#181); the server
// additionally returns 410 for those hashes.
func BuildIndex(dir, engineVersion string, blocklist Blocklist) (Index, map[string]string, []Skip, error) {
	idx := Index{Version: IndexVersion, Worlds: []Entry{}}
	byHash := map[string]string{}
	var skips []Skip

	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return idx, byHash, skips, nil // absent dir == empty index
		}
		return Index{}, nil, nil, fmt.Errorf("hub: read data dir %q: %w", dir, err)
	}

	for _, de := range entries {
		if de.IsDir() || !strings.EqualFold(filepath.Ext(de.Name()), archiveExt) {
			continue
		}
		path := filepath.Join(dir, de.Name())
		data, rerr := os.ReadFile(path)
		if rerr != nil {
			skips = append(skips, Skip{path, "read: " + rerr.Error()})
			continue
		}
		// Verify the archive end-to-end before indexing it — a corrupt or tampered
		// archive must never appear in the index (§2.4).
		arc, oerr := worldarchive.Open(path, engineVersion)
		if oerr != nil {
			skips = append(skips, Skip{path, "verify: " + oerr.Error()})
			continue
		}
		man := arc.Manifest
		arc.Close()

		sum := sha256.Sum256(data)
		hash := hex.EncodeToString(sum[:])
		// Takedown: a blocklisted hash is delisted (and refused on re-upload) —
		// never indexed, never served (the server 410s it). Reported loudly, not
		// silently dropped (§1.3).
		if reason, blocked := blocklist.Blocked(hash); blocked {
			skips = append(skips, Skip{path, "blocklisted: " + reason})
			continue
		}
		if prev, dup := byHash[hash]; dup {
			skips = append(skips, Skip{path, "duplicate of " + prev})
			continue
		}
		published := ""
		if info, serr := de.Info(); serr == nil {
			published = info.ModTime().UTC().Format(time.RFC3339)
		}
		byHash[hash] = path
		idx.Worlds = append(idx.Worlds, Entry{
			Hash:        hash,
			EngineRange: man.EngineRange,
			Title:       man.Title,
			Author:      man.Author,
			Description: man.Description,
			SizeBytes:   int64(len(data)),
			URL:         downloadURL(hash),
			PublishedAt: published,
		})
	}

	sort.Slice(idx.Worlds, func(i, j int) bool { return idx.Worlds[i].Hash < idx.Worlds[j].Hash })
	return idx, byHash, skips, nil
}

// downloadURL is the content-addressed path for an archive of the given file hash.
func downloadURL(hash string) string { return "/worlds/" + hash + archiveExt }
