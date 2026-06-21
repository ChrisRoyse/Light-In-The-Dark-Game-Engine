package hub

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"log"
	"net/http"
	"os"
	"strings"
	"sync/atomic"
	"time"
)

// snapshot is one atomically-published index generation: the marshaled index
// bytes served verbatim, plus the hash->path map for downloads. Published as a
// unit via atomic.Pointer so a GET never observes a torn index/download pairing
// while a Reindex is in flight (the "atomic, no torn reads" constraint).
type snapshot struct {
	indexJSON []byte
	byHash    map[string]string
	blocklist Blocklist // taken-down hashes served as 410 (#181)
}

// Server serves a static-friendly world index and the archives it lists, with no
// authentication (D-2026-06-11-23: anyone may GET the index and download a world).
type Server struct {
	dir           string
	engineVersion string
	blocklistPath string
	cur           atomic.Pointer[snapshot]
}

// NewServer returns a hub over dir. engineVersion is forwarded to verification
// ("" indexes every well-formed archive). Call Reindex before serving.
func NewServer(dir, engineVersion string) *Server {
	return &Server{dir: dir, engineVersion: engineVersion}
}

// SetBlocklistPath arms the takedown blocklist (#181). The file is reloaded on
// every Reindex, so adding a content hash and reindexing delists the world live:
// it drops from the index and its download returns HTTP 410. "" disarms it.
func (s *Server) SetBlocklistPath(path string) { s.blocklistPath = path }

// Reindex rebuilds the index from disk and atomically publishes it. Skipped
// (unverifiable) archives are logged loudly, never silently dropped (§1.3). The
// previous snapshot keeps serving until the new one is fully built and swapped.
func (s *Server) Reindex() error {
	blocklist, err := LoadBlocklist(s.blocklistPath)
	if err != nil {
		return err // a malformed blocklist fails closed — don't serve a stale/empty one
	}
	idx, byHash, skips, err := BuildIndex(s.dir, s.engineVersion, blocklist)
	if err != nil {
		return err
	}
	for _, sk := range skips {
		log.Printf("hub: skipping archive %s: %s", sk.Path, sk.Reason)
	}
	body, err := json.MarshalIndent(idx, "", "  ")
	if err != nil {
		return err
	}
	body = append(body, '\n')
	s.cur.Store(&snapshot{indexJSON: body, byHash: byHash, blocklist: blocklist})
	return nil
}

// ServeHTTP routes GET /index.json (rebuilt per request so a newly added archive
// appears on the next fetch) and GET /worlds/<hash>.litdworld (content-addressed
// download). Everything else is 404. Only GET/HEAD are accepted.
func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	switch {
	case r.URL.Path == "/index.json":
		// Rebuild so an archive added while serving is reflected on the next fetch;
		// the swap is atomic so concurrent downloads see a consistent pairing.
		if err := s.Reindex(); err != nil {
			http.Error(w, "index unavailable", http.StatusInternalServerError)
			log.Printf("hub: reindex on request failed: %v", err)
			return
		}
		snap := s.cur.Load()
		w.Header().Set("Content-Type", "application/json")
		w.Write(snap.indexJSON)
	case strings.HasPrefix(r.URL.Path, "/worlds/"):
		s.serveArchive(w, r)
	default:
		http.NotFound(w, r)
	}
}

// serveArchive serves a content-addressed archive, refusing path traversal and
// unknown hashes (404). It serves the on-disk bytes the current index points at.
func (s *Server) serveArchive(w http.ResponseWriter, r *http.Request) {
	name := strings.TrimPrefix(r.URL.Path, "/worlds/")
	hash := strings.TrimSuffix(name, archiveExt)
	// Hashes are hex; reject anything else (defense in depth vs. traversal).
	if name == hash || !isHex(hash) {
		http.NotFound(w, r)
		return
	}
	snap := s.cur.Load()
	if snap == nil {
		http.Error(w, "index not built", http.StatusInternalServerError)
		return
	}
	// Takedown (#181): a blocklisted hash is Gone, not Not-Found — 410 with a
	// notice category, so a client (or CDN cache) learns the world was removed by
	// policy rather than mistaking it for a typo. Checked before the byHash lookup
	// (a blocklisted archive is never in byHash anyway; this also 410s a hash whose
	// file lingers on disk). The public sees only the category, never the dossier.
	if reason, blocked := snap.blocklist.Blocked(hash); blocked {
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		w.WriteHeader(http.StatusGone)
		fmt.Fprintf(w, "410 Gone: world %s was taken down (%s).\n"+
			"See docs/hub/abuse-takedown.md. Already-downloaded copies are unaffected.\n", hash, reason)
		return
	}
	path, ok := snap.byHash[hash]
	if !ok {
		http.NotFound(w, r) // unknown world: 404, service stays up
		return
	}
	f, err := os.Open(path)
	if err != nil {
		http.NotFound(w, r)
		return
	}
	defer f.Close()
	w.Header().Set("Content-Type", "application/octet-stream")
	info, _ := f.Stat()
	http.ServeContent(w, r, hash+archiveExt, modOrZero(info), f)
}

// modOrZero returns the file mod time, or the zero time if info is nil (which
// makes http.ServeContent skip Last-Modified / If-Modified-Since handling).
func modOrZero(info fs.FileInfo) time.Time {
	if info == nil {
		return time.Time{}
	}
	return info.ModTime()
}

func isHex(s string) bool {
	if s == "" {
		return false
	}
	for _, c := range s {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')) {
			return false
		}
	}
	return true
}
