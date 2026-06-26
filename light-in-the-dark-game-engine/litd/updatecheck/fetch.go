package updatecheck

import (
	"fmt"
	"io"
	"net/http"
	"time"
)

// HTTPFetcher fetches the manifest over HTTPS (D-2026-06-11-22: the page is
// served over HTTPS and the manifest carries artifact sha256s). It bounds the
// read and the time so an update check can never hang the menu; a non-200, a
// timeout, or a refused connection surfaces as an error, which Check turns into a
// silent Skip.
type HTTPFetcher struct {
	URL    string
	Client *http.Client // nil → a 5s-timeout default
}

const maxManifestBytes = 1 << 20 // 1 MiB cap — a release manifest is tiny

func (h HTTPFetcher) Fetch() ([]byte, error) {
	c := h.Client
	if c == nil {
		c = &http.Client{Timeout: 5 * time.Second}
	}
	resp, err := c.Get(h.URL)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("manifest http status %d", resp.StatusCode)
	}
	return io.ReadAll(io.LimitReader(resp.Body, maxManifestBytes))
}
