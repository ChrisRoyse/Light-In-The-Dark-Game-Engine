package hub

import (
	"encoding/json"
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
}

// Server serves a static-friendly world index and the archives it lists, with no
// authentication (D-2026-06-11-23: anyone may GET the index and download a world).
type Server struct {
	dir           string
	engineVersion string
	cur           atomic.Pointer[snapshot]
}

// NewServer returns a hub over dir. engineVersion is forwarded to verification
// ("" indexes every well-formed archive). Call Reindex before serving.
func NewServer(dir, engineVersion string) *Server {
	return &Server{dir: dir, engineVersion: engineVersion}
}

// Reindex rebuilds the index from disk and atomically publishes it. Skipped
// (unverifiable) archives are logged loudly, never silently dropped (§1.3). The
// previous snapshot keeps serving until the new one is fully built and swapped.
func (s *Server) Reindex() error {
	idx, byHash, skips, err := BuildIndex(s.dir, s.engineVersion)
	if err != nil {
		return err
	}
	for _, sk := range skips {
		log.Printf("hub: skipping unverifiable archive %s: %s", sk.Path, sk.Reason)
	}
	body, err := json.MarshalIndent(idx, "", "  ")
	if err != nil {
		return err
	}
	body = append(body, '\n')
	s.cur.Store(&snapshot{indexJSON: body, byHash: byHash})
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
