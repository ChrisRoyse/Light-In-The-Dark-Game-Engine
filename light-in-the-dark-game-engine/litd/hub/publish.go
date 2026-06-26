package hub

// Publish intake (#176): the world-sharing hard gate (D-2026-06-11-20, -23). An
// uploaded archive is accepted into the served set ONLY if it passes
// worldarchive.Open — the SAME verification the client loader runs — fail-closed.
// That single gate enforces both of #176's required checks:
//
//	hash gate:    the manifest schema, the aggregate fingerprint, and EVERY
//	              member's content hash are re-verified — a single flipped byte
//	              rejects the upload naming the member.
//	sandbox gate: every Lua chunk is run through the restricted-VM lint (no
//	              io/os/net reachable, no require/loadfile/dofile) inside Open's
//	              verify (R-SEC-1, §2.5) — D-20's "no sharing feature ships unless
//	              world Lua runs in the sandbox", naming the chunk + violation.
//
// Using the loader's own verify is deliberate: the hub stores exactly what the
// loader will accept, so a published world can never fail at a downloader's load.
// Only then is the archive stored immutably, keyed by its content hash. A
// rejected upload leaves ZERO trace (no index entry, no file on disk). Re-publish
// of an identical archive is idempotent — the content-addressed name already
// exists, so it is a no-op, not a duplicate.

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
)

// PublishResult reports an accepted publish: the content hash it is stored under
// and whether this call actually wrote it (false = it was already present, so the
// idempotent no-op path).
type PublishResult struct {
	Hash   string `json:"hash"`
	Stored bool   `json:"stored"`
}

// Publish runs the intake pipeline on archive bytes. It returns either an
// accepted result (findings nil) or the rejection findings (result zero, nothing
// stored) — a malformed/oversized upload returns a non-nil error. Nothing is
// written to the served dir unless BOTH gates pass.
func (s *Server) Publish(data []byte) (PublishResult, []string, error) {
	if len(data) == 0 {
		return PublishResult{}, nil, fmt.Errorf("empty upload")
	}
	if len(data) > maxArchiveBytes {
		return PublishResult{}, nil, fmt.Errorf("upload exceeds %d-byte limit", maxArchiveBytes)
	}
	if err := os.MkdirAll(s.dir, 0o755); err != nil {
		return PublishResult{}, nil, err
	}

	// Stage the bytes in a temp file in the served dir (same fs => atomic rename).
	// Removed on every path except a committed store, so a rejected upload leaves
	// no trace.
	tmp, err := os.CreateTemp(s.dir, ".publish-*.litdworld.part")
	if err != nil {
		return PublishResult{}, nil, err
	}
	tmpPath := tmp.Name()
	committed := false
	defer func() {
		tmp.Close()
		if !committed {
			os.Remove(tmpPath)
		}
	}()
	if _, err := tmp.Write(data); err != nil {
		return PublishResult{}, nil, err
	}
	if err := tmp.Sync(); err != nil {
		return PublishResult{}, nil, err
	}

	// The hard gate: hash/manifest + per-member sandbox lint, via the loader's own
	// verify. Any failure (a member hash mismatch OR a Lua chunk that reaches for
	// io/os/net) rejects the upload with a message naming the member/chunk; nothing
	// is stored.
	arc, err := worldarchive.Open(tmpPath, "")
	if err != nil {
		return PublishResult{}, []string{err.Error()}, nil
	}
	arc.Close()

	// Gates passed. Store immutably, content-addressed.
	sum := sha256.Sum256(data)
	hash := hex.EncodeToString(sum[:])
	dest := filepath.Join(s.dir, hash+archiveExt)
	if _, statErr := os.Stat(dest); statErr == nil {
		return PublishResult{Hash: hash, Stored: false}, nil, nil // idempotent no-op
	}
	if err := os.Rename(tmpPath, dest); err != nil {
		return PublishResult{}, nil, err
	}
	committed = true
	return PublishResult{Hash: hash, Stored: true}, nil, nil
}

// servePublish is the POST /publish handler (enabled by AllowPublish). 200 +
// {hash,stored} on accept; 422 + newline-separated findings on a gate rejection;
// 400/413 on a malformed/oversized body.
func (s *Server) servePublish(w http.ResponseWriter, r *http.Request) {
	data, err := io.ReadAll(io.LimitReader(r.Body, maxArchiveBytes+1))
	if err != nil {
		http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
		return
	}
	res, findings, err := s.Publish(data)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if len(findings) > 0 {
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		w.WriteHeader(http.StatusUnprocessableEntity)
		fmt.Fprintf(w, "publish rejected — %d finding(s):\n", len(findings))
		for _, f := range findings {
			fmt.Fprintln(w, "  "+f)
		}
		return
	}
	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	w.WriteHeader(http.StatusOK)
	verb := "stored"
	if !res.Stored {
		verb = "already present"
	}
	fmt.Fprintf(w, "published %s (%s)\n", res.Hash, verb)
}
